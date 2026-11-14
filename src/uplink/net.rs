use std::collections::{HashMap, HashSet};
use std::io;
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::channel::mpsc::UnboundedSender;

use super::devices::Access;
use super::protocol::{self, Frame, Hello, Message};
use super::wire::{self, Channel, Identity, Key, Mode};
use crate::terminal::Feed;

pub const SERVICE: &str = "_nucli-uplink._tcp.local.";
const HANDSHAKE: Duration = Duration::from_secs(10);
const PING: Duration = Duration::from_secs(15);
const SILENCE: Duration = Duration::from_secs(45);
const DIAL: Duration = Duration::from_secs(4);
const BACKLOG: usize = 8388608;
const PIECE: usize = 1048576;
const HANDSHAKES: usize = 16;
pub const PAIRING_TIME: Duration = Duration::from_secs(300);
pub const PAIRING_ATTEMPTS: u32 = 3;

pub enum NetEvent {
    Connected(Connection),
    Message(u64, Message),
    Input(u64, String, Vec<u8>),
    Closed(u64),
    DialFailed(Key, String),
    Paired { key: Key, hello: Hello, address: Option<String>, access: Access },
    PairingFailed { attempts: u32, ended: bool },
    JoinFailed(String),
    Nearby(Nearby),
    Gone(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Nearby {
    pub fullname: String,
    pub key: Key,
    pub name: String,
    pub addresses: Vec<SocketAddr>,
    pub pairing: bool,
}

pub struct Pairing {
    pub code: String,
    pub expires: Instant,
    pub attempts: u32,
    pub access: Access,
    busy: bool,
}

impl Pairing {
    pub fn new(access: Access) -> Pairing {
        Pairing { code: wire::new_code(), expires: Instant::now() + PAIRING_TIME, attempts: 0, access, busy: false }
    }
}

pub struct Shared {
    pub identity: Identity,
    pub trusted: Mutex<HashSet<Key>>,
    pub pairing: Mutex<Option<Pairing>>,
    pub hello: Mutex<Hello>,
    pub events: UnboundedSender<NetEvent>,
    handshakes: AtomicUsize,
}

impl Shared {
    pub fn new(identity: Identity, hello: Hello, events: UnboundedSender<NetEvent>) -> Arc<Shared> {
        Arc::new(Shared {
            identity,
            trusted: Mutex::new(HashSet::new()),
            pairing: Mutex::new(None),
            hello: Mutex::new(hello),
            events,
            handshakes: AtomicUsize::new(0),
        })
    }

    fn post(&self, event: NetEvent) {
        let _ = self.events.unbounded_send(event);
    }

    fn trusts(&self, key: &Key) -> bool {
        self.trusted.lock().is_ok_and(|trusted| trusted.contains(key))
    }

    fn hello(&self) -> Hello {
        self.hello.lock().map(|hello| hello.clone()).unwrap_or_default()
    }
}

#[derive(Clone)]
pub struct Outbox {
    sender: Sender<Vec<u8>>,
    queued: Arc<AtomicUsize>,
    closed: Arc<AtomicBool>,
    stream: Arc<TcpStream>,
}

impl Outbox {
    pub fn send(&self, message: &Message) -> bool {
        self.push(protocol::message(message))
    }

    pub fn input(&self, tab: &str, data: &[u8]) -> bool {
        self.push(protocol::input(tab, data))
    }

    pub fn feed(&self, tab: &str, feed: &Feed) -> bool {
        if self.queued.load(Ordering::Acquire) >= BACKLOG {
            return false;
        }
        match feed {
            Feed::Bytes { local, data } if data.len() > PIECE => data
                .chunks(PIECE)
                .all(|piece| self.push(protocol::feed(tab, &Feed::Bytes { local: *local, data: piece.to_vec() }))),
            _ => self.push(protocol::feed(tab, feed)),
        }
    }

    fn push(&self, frame: Vec<u8>) -> bool {
        if self.closed.load(Ordering::Acquire) {
            return false;
        }
        self.queued.fetch_add(frame.len(), Ordering::AcqRel);
        self.sender.send(frame).is_ok()
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    pub fn close(&self) {
        if !self.closed.swap(true, Ordering::AcqRel) {
            let _ = self.stream.shutdown(std::net::Shutdown::Both);
        }
    }
}

pub type Routes = Arc<Mutex<HashMap<String, Sender<Feed>>>>;

pub struct Connection {
    pub id: u64,
    pub key: Key,
    pub hello: Hello,
    pub address: SocketAddr,
    pub initiator: bool,
    pub outbox: Outbox,
    pub routes: Routes,
}

static NEXT: AtomicU64 = AtomicU64::new(1);

fn start(link: Channel, address: SocketAddr, initiator: bool, shared: &Arc<Shared>) -> io::Result<()> {
    let key = link.remote;
    let stream = Arc::new(link.stream.try_clone()?);
    let (mut reader, mut writer) = link.split()?;
    writer.write(&protocol::message(&Message::Hello(shared.hello())))?;
    let Some(Frame::Message(Message::Hello(hello))) = protocol::decode(&reader.read()?) else {
        return Err(io::Error::other("unexpected greeting"));
    };
    stream.set_read_timeout(Some(SILENCE))?;
    stream.set_write_timeout(Some(SILENCE))?;
    let (sender, frames) = channel::<Vec<u8>>();
    let outbox =
        Outbox { sender, queued: Arc::new(AtomicUsize::new(0)), closed: Arc::new(AtomicBool::new(false)), stream };
    let routes: Routes = Arc::new(Mutex::new(HashMap::new()));
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    shared.post(NetEvent::Connected(Connection {
        id,
        key,
        hello,
        address,
        initiator,
        outbox: outbox.clone(),
        routes: routes.clone(),
    }));
    let writing = outbox.clone();
    std::thread::spawn(move || {
        let ping = protocol::message(&Message::Ping);
        loop {
            let (frame, queued) = match frames.recv_timeout(PING) {
                Ok(frame) => (frame, true),
                Err(RecvTimeoutError::Timeout) => (ping.clone(), false),
                Err(RecvTimeoutError::Disconnected) => break,
            };
            if writer.write(&frame).is_err() {
                break;
            }
            if queued {
                writing.queued.fetch_sub(frame.len(), Ordering::AcqRel);
            }
        }
        writing.close();
    });
    let shared = shared.clone();
    let reading = outbox;
    std::thread::spawn(move || {
        while let Ok(bytes) = reader.read() {
            match protocol::decode(&bytes) {
                Some(Frame::Feed(tab, feed)) => {
                    let route = routes.lock().ok().and_then(|routes| routes.get(&tab).cloned());
                    if let Some(route) = route
                        && route.send(feed).is_err()
                        && let Ok(mut routes) = routes.lock()
                    {
                        routes.remove(&tab);
                    }
                }
                Some(Frame::Input(tab, data)) => shared.post(NetEvent::Input(id, tab, data)),
                Some(Frame::Message(Message::Ping | Message::Unknown)) => {}
                Some(Frame::Message(message)) => shared.post(NetEvent::Message(id, message)),
                None => break,
            }
        }
        reading.close();
        if let Ok(mut routes) = routes.lock() {
            routes.clear();
        }
        shared.post(NetEvent::Closed(id));
    });
    Ok(())
}

pub struct Server {
    stop: Arc<AtomicBool>,
    pub port: u16,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

pub fn listen(port: u16, shared: &Arc<Shared>) -> io::Result<Server> {
    let stop = Arc::new(AtomicBool::new(false));
    let mut listeners = vec![TcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED, port))?];
    if let Ok(listener) = TcpListener::bind((std::net::Ipv6Addr::UNSPECIFIED, port)) {
        listeners.push(listener);
    }
    for listener in listeners {
        listener.set_nonblocking(true)?;
        let (stop, shared) = (stop.clone(), shared.clone());
        std::thread::spawn(move || {
            while !stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, address)) => incoming(stream, address, &shared),
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(150))
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(500)),
                }
            }
        });
    }
    Ok(Server { stop, port })
}

struct Slot(Arc<Shared>);

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.handshakes.fetch_sub(1, Ordering::AcqRel);
    }
}

fn prepare(stream: &TcpStream) -> io::Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(HANDSHAKE))?;
    stream.set_write_timeout(Some(HANDSHAKE))
}

fn incoming(mut stream: TcpStream, address: SocketAddr, shared: &Arc<Shared>) {
    if shared.handshakes.fetch_add(1, Ordering::AcqRel) >= HANDSHAKES {
        shared.handshakes.fetch_sub(1, Ordering::AcqRel);
        return;
    }
    let slot = Slot(shared.clone());
    std::thread::spawn(move || {
        let shared = slot.0.clone();
        if prepare(&stream).is_err() {
            return;
        }
        match wire::read_preamble(&mut stream) {
            Ok(Mode::Session) => {
                if let Ok(link) = wire::accept(stream, &shared.identity, |key| shared.trusts(key)) {
                    let _ = start(link, address, false, &shared);
                }
            }
            Ok(Mode::Pair) => host_pairing(stream, address, &shared),
            Err(_) => {}
        }
    });
}

fn exchange_hello(channel: Channel, ours: &Hello) -> io::Result<(Key, Hello)> {
    let remote = channel.remote;
    let (mut reader, mut writer) = channel.split()?;
    writer.write(&protocol::message(&Message::Hello(ours.clone())))?;
    match protocol::decode(&reader.read()?) {
        Some(Frame::Message(Message::Hello(hello))) => Ok((remote, hello)),
        _ => Err(io::Error::other("unexpected pairing reply")),
    }
}

fn host_pairing(stream: TcpStream, address: SocketAddr, shared: &Arc<Shared>) {
    let code = {
        let Ok(mut pairing) = shared.pairing.lock() else { return };
        match pairing.as_mut() {
            Some(current) if !current.busy && current.expires > Instant::now() => {
                current.busy = true;
                current.code.clone()
            }
            _ => return,
        }
    };
    let result = wire::host_pairing(stream, &shared.identity, &code)
        .and_then(|channel| exchange_hello(channel, &shared.hello()));
    let Ok(mut pairing) = shared.pairing.lock() else { return };
    let Some(current) = pairing.as_mut() else { return };
    match result {
        Ok((key, hello)) => {
            let access = current.access;
            *pairing = None;
            let address = (hello.port > 0).then(|| SocketAddr::new(address.ip(), hello.port).to_string());
            shared.post(NetEvent::Paired { key, hello, address, access });
        }
        Err(_) => {
            current.busy = false;
            current.attempts += 1;
            let attempts = current.attempts;
            let ended = attempts >= PAIRING_ATTEMPTS;
            if ended {
                *pairing = None;
            }
            shared.post(NetEvent::PairingFailed { attempts, ended });
        }
    }
}

pub fn resolve(address: &str, default_port: u16) -> Vec<SocketAddr> {
    let address = address.trim();
    if let Ok(socket) = address.parse::<SocketAddr>() {
        return vec![socket];
    }
    if let Ok(ip) = address.trim_matches(['[', ']']).parse::<IpAddr>() {
        return vec![SocketAddr::new(ip, default_port)];
    }
    let with_port = match address.rsplit_once(':') {
        Some((_, port)) if port.parse::<u16>().is_ok() => address.to_string(),
        _ => format!("{address}:{default_port}"),
    };
    with_port.to_socket_addrs().map(|found| found.collect()).unwrap_or_default()
}

fn dial_any(addresses: &[String], default_port: u16) -> io::Result<(TcpStream, SocketAddr)> {
    let mut last = io::Error::new(io::ErrorKind::NotFound, "no address");
    let mut tried = HashSet::new();
    for address in addresses {
        for socket in resolve(address, default_port) {
            if !tried.insert(socket) {
                continue;
            }
            match TcpStream::connect_timeout(&socket, DIAL) {
                Ok(stream) => {
                    prepare(&stream)?;
                    return Ok((stream, socket));
                }
                Err(error) => last = error,
            }
        }
    }
    Err(last)
}

fn reason(error: &io::Error) -> String {
    match error.kind() {
        io::ErrorKind::PermissionDenied => error.to_string(),
        io::ErrorKind::UnexpectedEof | io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionAborted => {
            "it refused this device".into()
        }
        _ => "not reachable".into(),
    }
}

pub fn dial(key: Key, addresses: Vec<String>, default_port: u16, shared: &Arc<Shared>) {
    let shared = shared.clone();
    std::thread::spawn(move || {
        let result = dial_any(&addresses, default_port).and_then(|(stream, socket)| {
            let link = wire::connect(stream, &shared.identity, &key)?;
            start(link, socket, true, &shared)
        });
        if let Err(error) = result {
            shared.post(NetEvent::DialFailed(key, reason(&error)));
        }
    });
}

pub fn join(
    addresses: Vec<String>,
    code: String,
    expected: Option<Key>,
    access: Access,
    default_port: u16,
    shared: &Arc<Shared>,
) {
    let shared = shared.clone();
    std::thread::spawn(move || {
        let result = dial_any(&addresses, default_port).and_then(|(stream, socket)| {
            let channel = wire::join(stream, &shared.identity, &code)?;
            let (key, hello) = exchange_hello(channel, &shared.hello())
                .map_err(|_| io::Error::other("The code did not match, or pairing ended on the other device."))?;
            if expected.is_some_and(|expected| expected != key) {
                return Err(io::Error::other("A different device answered at this address."));
            }
            Ok((key, hello, socket))
        });
        match result {
            Ok((key, hello, socket)) => {
                let address = Some(SocketAddr::new(socket.ip(), hello.port.max(1)).to_string());
                shared.post(NetEvent::Paired { key, hello, address, access });
            }
            Err(error) => shared.post(NetEvent::JoinFailed(error.to_string())),
        }
    });
}

pub struct Discovery {
    daemon: mdns_sd::ServiceDaemon,
    instance: String,
}

impl Discovery {
    pub fn start(shared: &Arc<Shared>) -> Option<Discovery> {
        let daemon = mdns_sd::ServiceDaemon::new().ok()?;
        let own = shared.identity.public;
        let instance = wire::encode_key(&own)[..20].to_string();
        let browse = daemon.browse(SERVICE).ok()?;
        let shared = shared.clone();
        std::thread::spawn(move || {
            while let Ok(event) = browse.recv() {
                match event {
                    mdns_sd::ServiceEvent::ServiceResolved(info) => {
                        let Some(key) = info.get_property_val_str("id").and_then(wire::decode_key) else { continue };
                        if key == own {
                            continue;
                        }
                        let port = info.get_port();
                        let mut addresses: Vec<SocketAddr> = info
                            .get_addresses()
                            .iter()
                            .map(|ip| ip.to_ip_addr())
                            .filter(|ip| !matches!(ip, IpAddr::V6(v6) if v6.segments()[0] & 0xffc0 == 0xfe80))
                            .map(|ip| SocketAddr::new(ip, port))
                            .collect();
                        addresses.sort_by_key(|address| (address.is_ipv6(), address.ip().is_loopback()));
                        shared.post(NetEvent::Nearby(Nearby {
                            fullname: info.get_fullname().to_string(),
                            key,
                            name: info.get_property_val_str("name").unwrap_or_default().chars().take(64).collect(),
                            addresses,
                            pairing: info.get_property_val_str("pairing") == Some("1"),
                        }));
                    }
                    mdns_sd::ServiceEvent::ServiceRemoved(_, fullname) => shared.post(NetEvent::Gone(fullname)),
                    _ => {}
                }
            }
        });
        Some(Discovery { daemon, instance })
    }

    pub fn announce(&self, key: &Key, name: &str, port: u16, pairing: bool) {
        let id = wire::encode_key(key);
        let properties = [("id", id.as_str()), ("name", name), ("pairing", if pairing { "1" } else { "0" })];
        let host = format!("{}.local.", self.instance.to_lowercase());
        if let Ok(info) = mdns_sd::ServiceInfo::new(SERVICE, &self.instance, &host, "", port, &properties[..]) {
            let _ = self.daemon.register(info.enable_addr_auto());
        }
    }
}

impl Drop for Discovery {
    fn drop(&mut self) {
        let _ = self.daemon.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    fn next(events: &mut futures::channel::mpsc::UnboundedReceiver<NetEvent>) -> NetEvent {
        futures::executor::block_on(async {
            let timeout = std::time::Instant::now() + Duration::from_secs(10);
            loop {
                if let Ok(Some(event)) = events.try_next() {
                    return event;
                }
                assert!(std::time::Instant::now() < timeout, "no event");
                std::thread::sleep(Duration::from_millis(5));
            }
        })
    }

    fn node(name: &str) -> (Arc<Shared>, futures::channel::mpsc::UnboundedReceiver<NetEvent>) {
        let (events, received) = futures::channel::mpsc::unbounded();
        let hello = Hello { name: name.into(), os: "test".into(), version: "1".into(), port: 0 };
        (Shared::new(Identity::generate(), hello, events), received)
    }

    fn free_port() -> u16 {
        TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
    }

    #[test]
    fn pairs_then_connects_and_streams() {
        let (host, mut host_events) = node("host");
        let (guest, mut guest_events) = node("guest");
        let port = free_port();
        let _server = listen(port, &host).unwrap();
        let address = format!("127.0.0.1:{port}");

        *host.pairing.lock().unwrap() = Some(Pairing::new(Access::View));
        join(vec![address.clone()], "WRONGCDE".into(), None, Access::Control, port, &guest);
        assert!(matches!(next(&mut guest_events), NetEvent::JoinFailed(_)));
        assert!(matches!(next(&mut host_events), NetEvent::PairingFailed { attempts: 1, ended: false }));

        let code = host.pairing.lock().unwrap().as_ref().unwrap().code.clone();
        join(vec![address.clone()], code, Some(host.identity.public), Access::Control, port, &guest);
        let NetEvent::Paired { key, hello, access, .. } = next(&mut host_events) else { panic!("host not paired") };
        assert_eq!((key, hello.name.as_str(), access), (guest.identity.public, "guest", Access::View));
        let NetEvent::Paired { key, .. } = next(&mut guest_events) else { panic!("guest not paired") };
        assert_eq!(key, host.identity.public);
        assert!(host.pairing.lock().unwrap().is_none());

        dial(host.identity.public, vec![address.clone()], port, &guest);
        assert!(matches!(next(&mut guest_events), NetEvent::DialFailed(..)));

        host.trusted.lock().unwrap().insert(guest.identity.public);
        dial(host.identity.public, vec![address.clone()], port, &guest);
        let NetEvent::Connected(outgoing) = next(&mut guest_events) else { panic!("guest not connected") };
        let NetEvent::Connected(incoming) = next(&mut host_events) else { panic!("host not connected") };
        assert!(outgoing.initiator && !incoming.initiator);

        let (feeds, received) = channel();
        incoming.routes.lock().unwrap().insert("tab".into(), feeds);
        outgoing.outbox.feed("tab", &Feed::Bytes { local: false, data: b"output".to_vec() });
        assert!(
            matches!(received.recv_timeout(Duration::from_secs(5)), Ok(Feed::Bytes { data, .. }) if data == b"output")
        );
        outgoing.outbox.send(&Message::Watch { tab: "x".into() });
        assert!(matches!(next(&mut host_events), NetEvent::Message(_, Message::Watch { .. })));
        incoming.outbox.input("tab", b"ls\r");
        assert!(matches!(next(&mut guest_events), NetEvent::Input(_, tab, data) if tab == "tab" && data == b"ls\r"));

        outgoing.outbox.close();
        assert!(matches!(next(&mut host_events), NetEvent::Closed(id) if id == incoming.id));
        assert!(matches!(next(&mut guest_events), NetEvent::Closed(id) if id == outgoing.id));
    }

    #[test]
    fn three_wrong_codes_end_pairing() {
        let (host, mut host_events) = node("host");
        let (guest, mut guest_events) = node("guest");
        let port = free_port();
        let _server = listen(port, &host).unwrap();
        *host.pairing.lock().unwrap() = Some(Pairing::new(Access::Control));
        for attempt in 1..=PAIRING_ATTEMPTS {
            join(vec![format!("127.0.0.1:{port}")], "00000000".into(), None, Access::Control, port, &guest);
            assert!(matches!(next(&mut guest_events), NetEvent::JoinFailed(_)));
            let NetEvent::PairingFailed { attempts, ended } = next(&mut host_events) else { panic!() };
            assert_eq!((attempts, ended), (attempt, attempt == PAIRING_ATTEMPTS));
        }
        assert!(host.pairing.lock().unwrap().is_none());
        let code = wire::new_code();
        join(vec![format!("127.0.0.1:{port}")], code, None, Access::Control, port, &guest);
        assert!(matches!(next(&mut guest_events), NetEvent::JoinFailed(_)));
    }

    #[test]
    fn resolves_addresses_with_default_ports() {
        assert_eq!(resolve("10.0.0.2", 47100), vec!["10.0.0.2:47100".parse().unwrap()]);
        assert_eq!(resolve("10.0.0.2:9", 47100), vec!["10.0.0.2:9".parse().unwrap()]);
        assert_eq!(resolve("::1", 47100), vec!["[::1]:47100".parse().unwrap()]);
        assert_eq!(resolve("[::1]:5", 47100), vec!["[::1]:5".parse().unwrap()]);
        assert!(resolve("localhost", 47100).iter().all(|address| address.port() == 47100));
    }
}
