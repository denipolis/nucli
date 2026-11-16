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
const APPROVAL: Duration = Duration::from_secs(120);
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
    Chunk(u64, u64, Vec<u8>),
    Closed(u64),
    DialFailed(Key, String),
    Paired { key: Key, hello: Hello, address: Option<String>, access: Access },
    PairingFailed { attempts: u32, ended: bool },
    Approve { key: Key, hello: Hello, sas: String, answer: std::sync::mpsc::Sender<Option<Access>> },
    Declined { key: Key, name: String },
    Confirm { name: String, sas: String },
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
}

pub struct Pairing {
    pub code: String,
    pub expires: Instant,
    pub attempts: u32,
    busy: bool,
}

impl Default for Pairing {
    fn default() -> Pairing {
        Pairing { code: wire::new_code(), expires: Instant::now() + PAIRING_TIME, attempts: 0, busy: false }
    }
}

pub struct Shared {
    pub identity: Identity,
    pub trusted: Mutex<HashSet<Key>>,
    pub pairing: Mutex<Option<Pairing>>,
    pub hello: Mutex<Hello>,
    pub excluded: Mutex<Vec<String>>,
    pub events: UnboundedSender<NetEvent>,
    handshakes: AtomicUsize,
    asking: AtomicBool,
    outgoing: Mutex<Option<TcpStream>>,
}

impl Shared {
    pub fn new(identity: Identity, hello: Hello, events: UnboundedSender<NetEvent>) -> Arc<Shared> {
        Arc::new(Shared {
            identity,
            trusted: Mutex::new(HashSet::new()),
            pairing: Mutex::new(None),
            hello: Mutex::new(hello),
            excluded: Mutex::new(Vec::new()),
            events,
            handshakes: AtomicUsize::new(0),
            asking: AtomicBool::new(false),
            outgoing: Mutex::new(None),
        })
    }

    pub fn cancel_join(&self) {
        if let Some(stream) = self.outgoing.lock().ok().and_then(|mut outgoing| outgoing.take()) {
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
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

    pub fn chunk(&self, id: u64, data: &[u8]) -> bool {
        self.push(protocol::chunk(id, data))
    }

    pub fn backed_up(&self) -> bool {
        self.queued.load(Ordering::Acquire) >= BACKLOG
    }

    pub fn input(&self, tab: &str, data: &[u8]) -> bool {
        data.chunks(PIECE).all(|piece| self.push(protocol::input(tab, piece)))
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
                Some(Frame::Chunk(transfer, data)) => shared.post(NetEvent::Chunk(id, transfer, data)),
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

fn watchdog(stream: &TcpStream, limit: Duration) -> io::Result<Sender<()>> {
    let stream = stream.try_clone()?;
    let (done, wait) = channel::<()>();
    std::thread::spawn(move || {
        if wait.recv_timeout(limit) == Err(RecvTimeoutError::Timeout) {
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
    });
    Ok(done)
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
        if arrived_excluded(&stream, &shared) {
            return;
        }
        let Ok(watchdog) = prepare(&stream).and_then(|_| watchdog(&stream, HANDSHAKE)) else { return };
        match wire::read_preamble(&mut stream) {
            Ok(Mode::Session) => {
                if let Ok(link) = wire::accept(stream, &shared.identity, Mode::Session, |key| shared.trusts(key)) {
                    let _ = start(link, address, false, &shared);
                }
            }
            Ok(Mode::Pair) => host_pairing(stream, address, watchdog, &shared),
            Ok(Mode::Request) => host_request(stream, address, watchdog, &shared),
            Err(_) => {}
        }
    });
}

fn arrived_excluded(stream: &TcpStream, shared: &Shared) -> bool {
    let excluded = shared.excluded.lock().map(|names| names.clone()).unwrap_or_default();
    if excluded.is_empty() {
        return false;
    }
    let Ok(local) = stream.local_addr() else { return true };
    let ip = match local.ip() {
        IpAddr::V6(ip) => ip.to_ipv4_mapped().map_or(IpAddr::V6(ip), IpAddr::V4),
        ip => ip,
    };
    if_addrs::get_if_addrs()
        .unwrap_or_default()
        .iter()
        .any(|interface| interface.ip() == ip && excluded.contains(&interface.name))
}

fn exchange_hello(channel: Channel, ours: &Hello) -> io::Result<(Key, Hello, wire::Reader, wire::Writer)> {
    let remote = channel.remote;
    channel.stream.set_read_timeout(Some(APPROVAL))?;
    let (mut reader, mut writer) = channel.split()?;
    writer.write(&protocol::message(&Message::Hello(ours.clone())))?;
    match protocol::decode(&reader.read()?) {
        Some(Frame::Message(Message::Hello(hello))) => Ok((remote, hello, reader, writer)),
        _ => Err(io::Error::other("unexpected pairing reply")),
    }
}

fn host_pairing(stream: TcpStream, address: SocketAddr, watchdog: Sender<()>, shared: &Arc<Shared>) {
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
    let current = |shared: &Shared| {
        shared.pairing.lock().is_ok_and(|pairing| pairing.as_ref().is_some_and(|current| current.code == code))
    };
    let result = wire::host_pairing(stream, &shared.identity, &code).and_then(|channel| {
        if !current(shared) {
            return Err(io::Error::other("pairing ended"));
        }
        let sas = channel.sas.clone();
        exchange_hello(channel, &shared.hello()).map(|exchanged| (sas, exchanged))
    });
    let (sas, (key, hello, reader, writer)) = match result {
        Ok(paired) => paired,
        Err(error) => {
            let Ok(mut pairing) = shared.pairing.lock() else { return };
            let Some(current) = pairing.as_mut().filter(|current| current.code == code) else { return };
            current.busy = false;
            if error.kind() != io::ErrorKind::PermissionDenied {
                return;
            }
            current.attempts += 1;
            let attempts = current.attempts;
            let ended = attempts >= PAIRING_ATTEMPTS;
            if ended {
                *pairing = None;
            }
            shared.post(NetEvent::PairingFailed { attempts, ended });
            return;
        }
    };
    drop(watchdog);
    let Some(verdict) = ask(key, &hello, sas, reader, shared) else {
        if let Ok(mut pairing) = shared.pairing.lock()
            && let Some(current) = pairing.as_mut().filter(|current| current.code == code)
        {
            current.busy = false;
        }
        return;
    };
    let Ok(mut pairing) = shared.pairing.lock() else { return };
    if pairing.as_ref().is_none_or(|current| current.code != code) {
        return;
    }
    *pairing = None;
    drop(pairing);
    settle(verdict, key, hello, writer, address, shared);
}

fn host_request(stream: TcpStream, address: SocketAddr, watchdog: Sender<()>, shared: &Arc<Shared>) {
    if shared.asking.load(Ordering::Acquire) {
        return;
    }
    let Ok(channel) = wire::accept(stream, &shared.identity, Mode::Request, |_| true) else { return };
    let sas = channel.sas.clone();
    let Ok((key, hello, reader, writer)) = exchange_hello(channel, &shared.hello()) else { return };
    drop(watchdog);
    if let Some(verdict) = ask(key, &hello, sas, reader, shared) {
        settle(verdict, key, hello, writer, address, shared);
    }
}

fn ask(key: Key, hello: &Hello, sas: String, mut reader: wire::Reader, shared: &Shared) -> Option<Option<Access>> {
    if shared.asking.swap(true, Ordering::AcqRel) {
        return None;
    }
    let (answer, answered) = channel();
    let withdrawn = answer.clone();
    std::thread::spawn(move || {
        let _ = reader.read();
        let _ = withdrawn.send(None);
    });
    shared.post(NetEvent::Approve { key, hello: hello.clone(), sas, answer });
    let verdict = answered.recv_timeout(APPROVAL).ok().flatten();
    shared.asking.store(false, Ordering::Release);
    Some(verdict)
}

fn settle(
    verdict: Option<Access>,
    key: Key,
    hello: Hello,
    mut writer: wire::Writer,
    address: SocketAddr,
    shared: &Shared,
) {
    match verdict {
        Some(access) if writer.write(&protocol::message(&Message::Approved)).is_ok() => {
            let address = (hello.port > 0).then(|| SocketAddr::new(address.ip(), hello.port).to_string());
            shared.post(NetEvent::Paired { key, hello, address, access });
        }
        _ => shared.post(NetEvent::Declined { key, name: hello.name }),
    }
    let _ = writer.stream.shutdown(std::net::Shutdown::Both);
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

fn sockets(addresses: &[String], default_port: u16) -> Vec<SocketAddr> {
    let mut tried = HashSet::new();
    addresses.iter().flat_map(|address| resolve(address, default_port)).filter(|socket| tried.insert(*socket)).collect()
}

fn reach(socket: &SocketAddr) -> io::Result<TcpStream> {
    let stream = TcpStream::connect_timeout(socket, DIAL)?;
    prepare(&stream)?;
    Ok(stream)
}

fn dial_any(addresses: &[String], default_port: u16) -> io::Result<(TcpStream, SocketAddr)> {
    let mut last = io::Error::new(io::ErrorKind::NotFound, "no address");
    for socket in sockets(addresses, default_port) {
        match reach(&socket) {
            Ok(stream) => return Ok((stream, socket)),
            Err(error) => last = error,
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
        let mut last = io::Error::new(io::ErrorKind::NotFound, "no address");
        for socket in sockets(&addresses, default_port) {
            let result = reach(&socket).and_then(|stream| {
                let _watchdog = watchdog(&stream, HANDSHAKE)?;
                let link = wire::connect(stream, &shared.identity, Mode::Session, Some(&key))?;
                start(link, socket, true, &shared)
            });
            match result {
                Ok(()) => return,
                Err(error) => last = error,
            }
        }
        shared.post(NetEvent::DialFailed(key, reason(&last)));
    });
}

pub fn join(
    addresses: Vec<String>,
    code: Option<String>,
    expected: Option<Key>,
    access: Access,
    default_port: u16,
    shared: &Arc<Shared>,
) {
    let shared = shared.clone();
    std::thread::spawn(move || {
        let result = dial_any(&addresses, default_port).and_then(|(stream, socket)| {
            if let Ok(mut outgoing) = shared.outgoing.lock() {
                *outgoing = stream.try_clone().ok();
            }
            let watchdog = watchdog(&stream, HANDSHAKE)?;
            let channel = match &code {
                Some(code) => wire::join(stream, &shared.identity, code)?,
                None => wire::connect(stream, &shared.identity, Mode::Request, expected.as_ref()).map_err(|_| {
                    io::Error::other("The other device did not answer. It may be busy with another device.")
                })?,
            };
            let sas = channel.sas.clone();
            let (key, hello, mut reader, _writer) = exchange_hello(channel, &shared.hello()).map_err(|_| {
                io::Error::other(match code {
                    Some(_) => "The code did not match, or pairing ended on the other device.",
                    None => "The other device did not answer. It may be busy with another device.",
                })
            })?;
            if expected.is_some_and(|expected| expected != key) {
                return Err(io::Error::other("A different device answered at this address."));
            }
            drop(watchdog);
            shared.post(NetEvent::Confirm { name: hello.name.clone(), sas });
            match reader.read().ok().and_then(|frame| protocol::decode(&frame)) {
                Some(Frame::Message(Message::Approved)) => Ok((key, hello, socket)),
                _ => Err(io::Error::other(format!("{} did not allow this device.", hello.name))),
            }
        });
        match result {
            Ok((key, hello, socket)) => {
                let address = Some(SocketAddr::new(socket.ip(), hello.port.max(1)).to_string());
                shared.post(NetEvent::Paired { key, hello, address, access });
            }
            Err(error) => shared.post(NetEvent::JoinFailed(error.to_string())),
        }
        if let Ok(mut outgoing) = shared.outgoing.lock() {
            *outgoing = None;
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
        let excluded: Vec<mdns_sd::IfKind> = shared
            .excluded
            .lock()
            .map(|names| names.iter().cloned().map(mdns_sd::IfKind::Name).collect())
            .unwrap_or_default();
        if !excluded.is_empty() {
            daemon.disable_interface(excluded).ok()?;
        }
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
                        addresses.truncate(4);
                        shared.post(NetEvent::Nearby(Nearby {
                            fullname: info.get_fullname().to_string(),
                            key,
                            name: info.get_property_val_str("name").unwrap_or_default().chars().take(64).collect(),
                            addresses,
                        }));
                    }
                    mdns_sd::ServiceEvent::ServiceRemoved(_, fullname) => shared.post(NetEvent::Gone(fullname)),
                    _ => {}
                }
            }
        });
        Some(Discovery { daemon, instance })
    }

    pub fn announce(&self, key: &Key, name: &str, port: u16) {
        let id = wire::encode_key(key);
        let properties = [("id", id.as_str()), ("name", name)];
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

    fn next(events: &mut futures::channel::mpsc::UnboundedReceiver<NetEvent>) -> NetEvent {
        futures::executor::block_on(async {
            let timeout = std::time::Instant::now() + Duration::from_secs(10);
            loop {
                if let Ok(event) = events.try_recv() {
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

    #[test]
    fn connections_on_an_excluded_network_are_refused() {
        let loopback = IpAddr::from([127, 0, 0, 1]);
        let Some(name) = if_addrs::get_if_addrs()
            .unwrap_or_default()
            .into_iter()
            .find(|interface| interface.ip() == loopback)
            .map(|interface| interface.name)
        else {
            return;
        };
        let (shared, _events) = node("host");
        let listener = TcpListener::bind((loopback, 0)).unwrap();
        let _client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (stream, _) = listener.accept().unwrap();
        assert!(!arrived_excluded(&stream, &shared));
        *shared.excluded.lock().unwrap() = vec!["unrelated0".into()];
        assert!(!arrived_excluded(&stream, &shared));
        *shared.excluded.lock().unwrap() = vec![name];
        assert!(arrived_excluded(&stream, &shared));
    }

    #[test]
    fn a_handshake_has_one_deadline() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        let started = Instant::now();
        let _watchdog = watchdog(&client, Duration::from_millis(200)).unwrap();
        let writer = std::thread::spawn(move || {
            for _ in 0..20 {
                if io::Write::write_all(&mut &server, b"x").is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        });
        let mut bytes = [0u8; 64];
        assert!(io::Read::read_exact(&mut client, &mut bytes).is_err());
        assert!(started.elapsed() < Duration::from_millis(900));
        let _ = writer.join();
    }

    #[test]
    fn a_finished_handshake_keeps_its_socket() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        drop(watchdog(&client, Duration::from_millis(50)).unwrap());
        std::thread::sleep(Duration::from_millis(150));
        io::Write::write_all(&mut &server, b"ok").unwrap();
        let mut bytes = [0u8; 2];
        io::Read::read_exact(&mut client, &mut bytes).unwrap();
        assert_eq!(&bytes, b"ok");
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

        *host.pairing.lock().unwrap() = Some(Pairing::default());
        join(vec![address.clone()], Some("WRONGCDE".into()), None, Access::Control, port, &guest);
        assert!(matches!(next(&mut guest_events), NetEvent::JoinFailed(_)));
        assert!(matches!(next(&mut host_events), NetEvent::PairingFailed { attempts: 1, ended: false }));

        let code = host.pairing.lock().unwrap().as_ref().unwrap().code.clone();
        join(vec![address.clone()], Some(code), Some(host.identity.public), Access::Control, port, &guest);
        let NetEvent::Approve { sas, answer, .. } = next(&mut host_events) else { panic!("host not asked") };
        let NetEvent::Confirm { sas: shown, .. } = next(&mut guest_events) else { panic!("guest not told") };
        assert_eq!(sas, shown);
        answer.send(Some(Access::View)).unwrap();
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
    fn requests_pair_without_a_code_once_allowed() {
        let (host, mut host_events) = node("host");
        let (guest, mut guest_events) = node("guest");
        let (other, mut other_events) = node("other");
        let port = free_port();
        let _server = listen(port, &host).unwrap();
        let address = format!("127.0.0.1:{port}");

        join(vec![address.clone()], None, Some(host.identity.public), Access::View, port, &guest);
        let NetEvent::Approve { key, sas, answer, .. } = next(&mut host_events) else { panic!("host not asked") };
        let NetEvent::Confirm { sas: shown, .. } = next(&mut guest_events) else { panic!("guest not told") };
        assert_eq!((key, &sas), (guest.identity.public, &shown));

        join(vec![address.clone()], None, None, Access::View, port, &other);
        assert!(matches!(next(&mut other_events), NetEvent::JoinFailed(_)));

        guest.cancel_join();
        assert!(matches!(next(&mut guest_events), NetEvent::JoinFailed(_)));
        assert!(matches!(next(&mut host_events), NetEvent::Declined { name, .. } if name == "guest"));
        drop(answer);

        join(vec![address.clone()], None, Some(guest.identity.public), Access::View, port, &other);
        assert!(matches!(next(&mut other_events), NetEvent::JoinFailed(_)));

        join(vec![address], None, Some(host.identity.public), Access::View, port, &guest);
        let NetEvent::Approve { answer, .. } = next(&mut host_events) else { panic!("host not asked again") };
        assert!(matches!(next(&mut guest_events), NetEvent::Confirm { .. }));
        answer.send(Some(Access::View)).unwrap();
        assert!(matches!(next(&mut host_events), NetEvent::Paired { key, .. } if key == guest.identity.public));
        assert!(matches!(next(&mut guest_events), NetEvent::Paired { key, .. } if key == host.identity.public));
    }

    #[test]
    fn three_wrong_codes_end_pairing() {
        let (host, mut host_events) = node("host");
        let (guest, mut guest_events) = node("guest");
        let port = free_port();
        let _server = listen(port, &host).unwrap();
        *host.pairing.lock().unwrap() = Some(Pairing::default());
        for attempt in 1..=PAIRING_ATTEMPTS {
            join(vec![format!("127.0.0.1:{port}")], Some("00000000".into()), None, Access::Control, port, &guest);
            assert!(matches!(next(&mut guest_events), NetEvent::JoinFailed(_)));
            let NetEvent::PairingFailed { attempts, ended } = next(&mut host_events) else { panic!() };
            assert_eq!((attempts, ended), (attempt, attempt == PAIRING_ATTEMPTS));
        }
        assert!(host.pairing.lock().unwrap().is_none());
        let code = wire::new_code();
        join(vec![format!("127.0.0.1:{port}")], Some(code), None, Access::Control, port, &guest);
        assert!(matches!(next(&mut guest_events), NetEvent::JoinFailed(_)));
    }

    #[test]
    fn denied_pairing_pairs_nobody() {
        let (host, mut host_events) = node("host");
        let (guest, mut guest_events) = node("guest");
        let port = free_port();
        let _server = listen(port, &host).unwrap();
        *host.pairing.lock().unwrap() = Some(Pairing::default());
        for _ in 0..PAIRING_ATTEMPTS {
            let mut junk = TcpStream::connect(("127.0.0.1", port)).unwrap();
            io::Write::write_all(&mut junk, &wire::preamble(Mode::Pair)).unwrap();
            drop(junk);
        }
        std::thread::sleep(Duration::from_millis(200));
        assert!(host.pairing.lock().unwrap().is_some());
        let code = host.pairing.lock().unwrap().as_ref().unwrap().code.clone();
        join(vec![format!("127.0.0.1:{port}")], Some(code), None, Access::View, port, &guest);
        let NetEvent::Approve { answer, .. } = next(&mut host_events) else { panic!("host not asked") };
        assert!(matches!(next(&mut guest_events), NetEvent::Confirm { .. }));
        answer.send(None).unwrap();
        assert!(matches!(next(&mut host_events), NetEvent::Declined { name, .. } if name == "guest"));
        assert!(matches!(next(&mut guest_events), NetEvent::JoinFailed(_)));
        assert!(host.pairing.lock().unwrap().is_none());
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
