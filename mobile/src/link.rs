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
use nucli_core::uplink::net::{self, Connection, NetEvent, Outbox, Routes, Shared};
use nucli_core::uplink::protocol::{Catalog, Hello, Message, Stop};
use nucli_core::uplink::wire::Key;

const PORT: u16 = 47100;

pub struct Online {
    conn: u64,
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
}

pub fn start(cx: &mut App) -> Entity<Link> {
    let path = data_dir().join("uplink.json");
    let (identity, devices) = devices::load(&path);
    let (events, mut received) = futures::channel::mpsc::unbounded();
    let hello = Hello {
        name: if cfg!(target_os = "ios") { "iPhone" } else { "Nucli Mobile" }.into(),
        os: std::env::consts::OS.into(),
        version: env!("CARGO_PKG_VERSION").into(),
        port: 0,
    };
    let shared = Shared::new(identity, hello, events);
    let link = cx.new(|_| Link {
        shared,
        path,
        devices,
        online: HashMap::new(),
        dialing: HashMap::new(),
        watched: HashMap::new(),
        joining: false,
        note: None,
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

fn data_dir() -> PathBuf {
    let folder = if cfg!(debug_assertions) { "Nucli Mobile Dev" } else { "Nucli Mobile" };
    dirs::data_dir().unwrap_or_else(std::env::temp_dir).join(folder)
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs())
}

impl Link {
    pub fn device_error(&self, key: &Key) -> Option<&str> {
        self.dialing.get(key).and_then(|dialing| dialing.error.as_deref())
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
            addresses.extend(device.addresses.iter().cloned());
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
            NetEvent::Closed(conn) => self.online.retain(|_, online| online.conn != conn),
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
            NetEvent::Input(..) | NetEvent::PairingFailed { .. } | NetEvent::Nearby(_) | NetEvent::Gone(_) => return,
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
            key: nucli_core::uplink::wire::encode_key(&key),
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
