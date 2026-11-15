use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use futures::StreamExt;
use gpui::{App, AppContext, Context, Entity};
use nucli_core::terminal::Feed;
use nucli_core::uplink::devices::{self, Access, Device};
use nucli_core::uplink::invite;
use nucli_core::uplink::net::{self, Connection, Nearby, NetEvent, Outbox, Routes, Shared};
use nucli_core::uplink::protocol::{Catalog, Hello, Message, Request, Response, Stop};
use nucli_core::uplink::wire::{self, Key};

const PORT: u16 = 47100;

pub struct Online {
    conn: u64,
    address: String,
    outbox: Outbox,
    routes: Routes,
    pub catalog: Catalog,
}

struct Dialing {
    busy: bool,
    next: Instant,
    backoff: Duration,
    error: Option<String>,
}

pub struct Link {
    shared: Arc<Shared>,
    path: PathBuf,
    pub devices: Vec<Device>,
    pub online: HashMap<Key, Online>,
    dialing: HashMap<Key, Dialing>,
    watched: HashMap<(Key, String), Sender<Feed>>,
    pub joining: bool,
    pub note: Option<String>,
    pub paired: Option<Key>,
    pub opened: Option<(Key, String)>,
    opening: Option<Key>,
    pub nearby: Vec<Nearby>,
    _browser: Option<crate::nearby::Browser>,
    asks: HashMap<u64, (Key, futures::channel::oneshot::Sender<Response>)>,
    next_ask: u64,
}

pub fn start(name: String, cx: &mut App) -> Entity<Link> {
    let path = crate::prefs::dir().join("uplink.json");
    let (identity, devices) = devices::load(&path);
    let (events, mut received) = futures::channel::mpsc::unbounded();
    let hello = Hello { name, os: std::env::consts::OS.into(), version: env!("CARGO_PKG_VERSION").into(), port: 0 };
    let shared = Shared::new(identity, hello, events);
    let browser = crate::nearby::start(&shared);
    let link = cx.new(|_| Link {
        shared,
        path,
        devices,
        online: HashMap::new(),
        dialing: HashMap::new(),
        watched: HashMap::new(),
        joining: false,
        note: None,
        paired: None,
        opened: None,
        opening: None,
        nearby: Vec::new(),
        _browser: browser,
        asks: HashMap::new(),
        next_ask: 0,
    });
    let handling = link.clone();
    cx.spawn(async move |cx| {
        while let Some(event) = received.next().await {
            handling.update(cx, |link, cx| link.handle(event, cx));
        }
    })
    .detach();
    let ticking = link.clone();
    cx.spawn(async move |cx| {
        loop {
            ticking.update(cx, |link, _| link.dial());
            cx.background_executor().timer(Duration::from_secs(1)).await;
        }
    })
    .detach();
    if let Ok(text) = std::env::var("NUCLI_PAIR") {
        link.update(cx, |link, cx| link.pair(&text, cx));
    }
    link
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs())
}

impl Link {
    pub fn device(&self, key: &Key) -> Option<&Device> {
        self.devices.iter().find(|device| device.public().as_ref() == Some(key))
    }

    fn device_mut(&mut self, key: &Key) -> Option<&mut Device> {
        self.devices.iter_mut().find(|device| device.public().as_ref() == Some(key))
    }

    pub fn device_error(&self, key: &Key) -> Option<&str> {
        self.dialing.get(key).and_then(|dialing| dialing.error.as_deref())
    }

    pub fn fingerprint(&self) -> String {
        wire::fingerprint(&self.shared.identity.public)
    }

    pub fn address(&self, key: &Key) -> Option<String> {
        self.online.get(key).map(|online| online.address.clone())
    }

    pub fn rename(&mut self, name: &str) {
        let hello = match self.shared.hello.lock() {
            Ok(mut hello) => {
                hello.name = name.to_string();
                hello.clone()
            }
            Err(_) => return,
        };
        for online in self.online.values() {
            online.outbox.send(&Message::Hello(hello.clone()));
        }
    }

    pub fn join_code(&mut self, code: &str, address: &str, cx: &mut Context<Self>) {
        let Some(code) = wire::normalize_code(code) else {
            self.note = Some(format!("A pairing code has {} letters and digits.", wire::CODE_LEN));
            cx.notify();
            return;
        };
        let address = address.trim();
        if address.is_empty() {
            self.note = Some("Type the computer's name or address.".into());
            cx.notify();
            return;
        }
        self.joining = true;
        self.note = None;
        net::join(vec![address.to_string()], code, None, Access::View, PORT, &self.shared);
        cx.notify();
    }

    pub fn control(&self, key: &Key) -> bool {
        self.online.get(key).is_some_and(|online| online.catalog.control)
    }

    pub fn ask(
        &mut self,
        key: Key,
        request: Request,
        cx: &mut Context<Self>,
    ) -> futures::channel::oneshot::Receiver<Response> {
        let (sender, receiver) = futures::channel::oneshot::channel();
        let Some(online) = self.online.get(&key) else {
            let _ = sender.send(Response::Failed { reason: "The computer is offline.".into() });
            return receiver;
        };
        self.next_ask += 1;
        let id = self.next_ask;
        online.outbox.send(&Message::Request { id, request });
        self.asks.insert(id, (key, sender));
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(15)).await;
            let _ = this.update(cx, |link, _| {
                if let Some((_, sender)) = link.asks.remove(&id) {
                    let reason = "No answer. Nucli on this computer may need an update.".to_string();
                    let _ = sender.send(Response::Failed { reason });
                }
            });
        })
        .detach();
        receiver
    }

    fn drop_asks(&mut self, key: Key) {
        let ids: Vec<u64> = self.asks.iter().filter(|(_, (asked, _))| *asked == key).map(|(id, _)| *id).collect();
        for id in ids {
            if let Some((_, sender)) = self.asks.remove(&id) {
                let _ = sender.send(Response::Failed { reason: "The computer went offline.".into() });
            }
        }
    }

    pub fn pairable(&self) -> Vec<&Nearby> {
        self.nearby.iter().filter(|nearby| nearby.pairing && self.device(&nearby.key).is_none()).collect()
    }

    pub fn join_nearby(&mut self, key: Key, code: &str, cx: &mut Context<Self>) {
        let Some(code) = wire::normalize_code(code) else {
            self.note = Some(format!("A pairing code has {} letters and digits.", wire::CODE_LEN));
            cx.notify();
            return;
        };
        let addresses: Vec<String> = self
            .nearby
            .iter()
            .filter(|nearby| nearby.key == key)
            .flat_map(|nearby| nearby.addresses.iter().map(|address| address.to_string()))
            .collect();
        self.joining = true;
        self.note = None;
        net::join(addresses, code, Some(key), Access::View, PORT, &self.shared);
        cx.notify();
    }

    pub fn open_tab(&mut self, key: Key, project: &str, launcher: &str) {
        if let Some(online) = self.online.get(&key) {
            self.opening = Some(key);
            online.outbox.send(&Message::Open { project: project.to_string(), launcher: launcher.to_string() });
        }
    }

    pub fn disconnect(&mut self, key: &Key, cx: &mut Context<Self>) {
        if let Some(online) = self.online.remove(key) {
            online.outbox.close();
        }
        if let Some(dialing) = self.dialing.get_mut(key) {
            dialing.next = Instant::now() + Duration::from_secs(30);
        }
        cx.notify();
    }

    pub fn forget(&mut self, key: &Key, cx: &mut Context<Self>) {
        if let Some(online) = self.online.remove(key) {
            online.outbox.send(&Message::Unpair);
            online.outbox.close();
        }
        self.devices.retain(|device| device.public().as_ref() != Some(key));
        self.dialing.remove(key);
        self.watched.retain(|(watched, _), _| watched != key);
        self.save();
        cx.notify();
    }

    pub fn set_manual(&mut self, key: &Key, text: &str, cx: &mut Context<Self>) {
        let addresses: Vec<String> =
            text.split(',').map(str::trim).filter(|address| !address.is_empty()).map(String::from).collect();
        if let Some(device) = self.device_mut(key) {
            device.manual = addresses;
        }
        if let Some(dialing) = self.dialing.get_mut(key) {
            dialing.next = Instant::now();
            dialing.backoff = Duration::from_secs(2);
        }
        self.save();
        cx.notify();
    }

    pub fn pair(&mut self, text: &str, cx: &mut Context<Self>) {
        let Some(invite) = invite::parse(text) else {
            self.note = Some("Copy the pairing link on your computer first.".into());
            cx.notify();
            return;
        };
        self.joining = true;
        self.note = None;
        net::join(invite.addresses, invite.code, invite.key, Access::View, PORT, &self.shared);
        cx.notify();
    }

    pub fn watch(&mut self, key: Key, tab: &str, feeds: Sender<Feed>) {
        self.watched.insert((key, tab.to_string()), feeds.clone());
        if let Some(online) = self.online.get(&key) {
            send_watch(online, tab, feeds);
        }
    }

    pub fn unwatch(&mut self, key: Key, tab: &str) {
        self.watched.remove(&(key, tab.to_string()));
        if let Some(online) = self.online.get(&key) {
            if let Ok(mut routes) = online.routes.lock() {
                routes.remove(tab);
            }
            online.outbox.send(&Message::Unwatch { tab: tab.to_string() });
        }
    }

    pub fn is_watched(&self, key: Key, tab: &str) -> bool {
        self.watched.contains_key(&(key, tab.to_string()))
    }

    pub fn input(&self, key: Key, tab: &str, data: &[u8]) {
        if let Some(online) = self.online.get(&key) {
            online.outbox.input(tab, data);
        }
    }

    pub fn resize(&self, key: Key, tab: &str, cols: usize, rows: usize) {
        if let Some(online) = self.online.get(&key) {
            let (cols, rows) = (cols.min(u16::MAX as usize) as u16, rows.min(u16::MAX as usize) as u16);
            online.outbox.send(&Message::Resize { tab: tab.to_string(), cols, rows });
        }
    }

    fn dial(&mut self) {
        let now = Instant::now();
        for device in self.devices.iter().filter(|device| !device.blocked) {
            let Some(key) = device.public() else { continue };
            if self.online.contains_key(&key) {
                continue;
            }
            let dialing = self.dialing.entry(key).or_insert(Dialing {
                busy: false,
                next: now,
                backoff: Duration::from_secs(2),
                error: None,
            });
            let mut addresses = device.manual.clone();
            for nearby in self.nearby.iter().filter(|nearby| nearby.key == key) {
                addresses.extend(nearby.addresses.iter().map(|address| address.to_string()));
            }
            addresses.extend(device.addresses.iter().cloned());
            let mut seen = std::collections::HashSet::new();
            addresses.retain(|address| seen.insert(address.clone()));
            if dialing.busy || dialing.next > now || addresses.is_empty() {
                continue;
            }
            dialing.busy = true;
            net::dial(key, addresses, PORT, &self.shared);
        }
    }

    fn handle(&mut self, event: NetEvent, cx: &mut Context<Self>) {
        match event {
            NetEvent::Connected(connection) => self.connected(connection),
            NetEvent::Message(conn, message) => self.message(conn, message),
            NetEvent::Closed(conn) => {
                let closed: Vec<Key> =
                    self.online.iter().filter(|(_, online)| online.conn == conn).map(|(key, _)| *key).collect();
                self.online.retain(|_, online| online.conn != conn);
                for key in closed {
                    self.drop_asks(key);
                }
            }
            NetEvent::DialFailed(key, error) => {
                if let Some(dialing) = self.dialing.get_mut(&key) {
                    dialing.busy = false;
                    dialing.next = Instant::now() + dialing.backoff;
                    dialing.backoff = (dialing.backoff * 2).min(Duration::from_secs(30));
                    dialing.error = Some(error);
                }
            }
            NetEvent::Paired { key, hello, address, access } => self.paired(key, hello, address, access),
            NetEvent::JoinFailed(reason) => {
                self.joining = false;
                self.note = Some(reason);
            }
            NetEvent::Nearby(found) => {
                if let Some(dialing) = self.dialing.get_mut(&found.key)
                    && !dialing.busy
                {
                    dialing.next = Instant::now();
                    dialing.backoff = Duration::from_secs(2);
                }
                self.nearby.retain(|nearby| nearby.fullname != found.fullname && nearby.key != found.key);
                if self.nearby.len() < 64 {
                    self.nearby.push(found);
                }
                self.dial();
            }
            NetEvent::Gone(fullname) => self.nearby.retain(|nearby| nearby.fullname != fullname),
            NetEvent::Input(..) | NetEvent::PairingFailed { .. } => return,
        }
        cx.notify();
    }

    fn connected(&mut self, connection: Connection) {
        let key = connection.key;
        if let Some(dialing) = self.dialing.get_mut(&key) {
            dialing.busy = false;
            dialing.backoff = Duration::from_secs(2);
            dialing.error = None;
        }
        let Some(device) =
            self.devices.iter_mut().find(|device| device.public().as_ref() == Some(&key) && !device.blocked)
        else {
            connection.outbox.close();
            return;
        };
        device.seen = now();
        device.learn(connection.address.to_string());
        if !connection.hello.name.is_empty() {
            device.name = connection.hello.name.chars().take(64).collect();
        }
        device.os = connection.hello.os.clone();
        let online = Online {
            conn: connection.id,
            address: connection.address.to_string(),
            outbox: connection.outbox,
            routes: connection.routes,
            catalog: Catalog::default(),
        };
        for ((watched_key, tab), feeds) in &self.watched {
            if *watched_key == key {
                send_watch(&online, tab, feeds.clone());
            }
        }
        if let Some(previous) = self.online.insert(key, online) {
            previous.outbox.close();
        }
        self.save();
    }

    fn message(&mut self, conn: u64, message: Message) {
        let Some(key) = self.online.iter().find(|(_, online)| online.conn == conn).map(|(key, _)| *key) else {
            return;
        };
        match message {
            Message::Catalog(catalog) => {
                if let Some(online) = self.online.get_mut(&key) {
                    online.catalog = catalog;
                }
            }
            Message::Stop { tab, reason: Stop::Behind } => {
                if let (Some(online), Some(feeds)) = (self.online.get(&key), self.watched.get(&(key, tab.clone()))) {
                    send_watch(online, &tab, feeds.clone());
                }
            }
            Message::Stop { tab, .. } => {
                self.watched.remove(&(key, tab));
            }
            Message::Response { id, response } => {
                if let Some((_, sender)) = self.asks.remove(&id) {
                    let _ = sender.send(response);
                }
            }
            Message::Opened { tab } => {
                if self.opening.take() == Some(key) {
                    self.opened = Some((key, tab));
                }
            }
            Message::Hello(hello) => {
                if let Some(device) = self.devices.iter_mut().find(|device| device.public().as_ref() == Some(&key)) {
                    device.name = hello.name.chars().take(64).collect();
                    device.os = hello.os;
                }
                self.save();
            }
            Message::Unpair => {
                self.devices.retain(|device| device.public().as_ref() != Some(&key));
                if let Some(online) = self.online.remove(&key) {
                    online.outbox.close();
                }
                self.save();
            }
            _ => {}
        }
    }

    fn paired(&mut self, key: Key, hello: Hello, address: Option<String>, access: Access) {
        let name: String = hello.name.chars().take(64).collect();
        self.devices.retain(|device| device.public().as_ref() != Some(&key));
        let mut device = Device {
            key: wire::encode_key(&key),
            name: name.clone(),
            os: hello.os,
            access,
            paired: now(),
            seen: now(),
            ..Default::default()
        };
        if let Some(address) = address {
            device.learn(address);
        }
        self.devices.push(device);
        self.joining = false;
        self.note = Some(format!("Paired with {name}."));
        self.paired = Some(key);
        self.dialing.remove(&key);
        self.save();
        self.dial();
    }

    fn save(&self) {
        devices::save(&self.path, &self.shared.identity, &self.devices);
    }
}

fn send_watch(online: &Online, tab: &str, feeds: Sender<Feed>) {
    if let Ok(mut routes) = online.routes.lock() {
        routes.insert(tab.to_string(), feeds);
    }
    online.outbox.send(&Message::Watch { tab: tab.to_string() });
    online.outbox.send(&Message::Seen { tab: tab.to_string() });
}
