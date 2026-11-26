mod act;
pub mod files;
pub(crate) mod host;
pub mod page;
pub mod pairing;
mod remote;
pub mod remote_actions;
pub mod sidebar;
mod transfers;

pub use nucli_core::uplink::{devices, invite, net, protocol, transfer, wire};

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use futures::StreamExt;
use gpui::{App, AppContext, Context, Entity, Global, Task};

use crate::session::{Remote, Session};
use crate::settings;
use crate::store::{self, Launcher};
use crate::terminal::{Feed, Request};
use devices::{Access, Device};
use net::{Connection, Discovery, Nearby, NetEvent, Outbox, Pairing, Routes, Server, Shared};
use protocol::{Catalog, Hello, Message, RemoteLauncher, RemoteProject, RemoteTab, RemoteWorkspace, Stop};
use wire::Key;

pub const MIRROR: &str = "uplink:";
const RACE: Duration = Duration::from_secs(5);
const NEARBY: usize = 64;

pub struct UplinkHandle(pub Entity<Uplink>);

impl Global for UplinkHandle {}

pub fn uplink(cx: &App) -> Option<Entity<Uplink>> {
    cx.try_global::<UplinkHandle>().map(|handle| handle.0.clone())
}

pub fn is_mirror(id: &str) -> bool {
    id.starts_with(MIRROR)
}

pub fn mirror_id(key: &Key, tab: &str) -> String {
    format!("{MIRROR}{}:{tab}", &wire::encode_key(key)[..12])
}

fn valid_tab(id: &str) -> bool {
    (1..=64).contains(&id.len()) && id.chars().all(|char| char.is_ascii_alphanumeric() || char == '-')
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|time| time.as_secs()).unwrap_or(0)
}

pub struct Link {
    pub conn: u64,
    pub hello: Hello,
    pub address: std::net::SocketAddr,
    initiator: bool,
    since: Instant,
    outbox: Outbox,
    routes: Routes,
    pub catalog: Option<Catalog>,
    sent: Option<Catalog>,
}

struct Watcher {
    conn: u64,
    key: Key,
    tab: String,
    alive: Arc<AtomicBool>,
    size: Option<(usize, usize)>,
}

pub struct Target {
    pub key: Key,
    pub tab: String,
    feeds: std::sync::mpsc::Sender<Feed>,
    watched: bool,
}

struct Dialing {
    busy: bool,
    next: Instant,
    backoff: Duration,
    error: Option<String>,
}

pub struct Uplink {
    pub shared: Arc<Shared>,
    pub devices: Vec<Device>,
    path: PathBuf,
    server: Option<Server>,
    discovery: Option<Discovery>,
    announced: Option<(String, u16)>,
    pub error: Option<String>,
    pub links: HashMap<Key, Link>,
    pub nearby: Vec<Nearby>,
    pub dialog: Option<pairing::Dialog>,
    pub note: Option<String>,
    dialing: HashMap<Key, Dialing>,
    watchers: Vec<Watcher>,
    pub mirrors: HashMap<String, Target>,
    outboxes: Arc<Mutex<HashMap<Key, Outbox>>>,
    push: Option<Task<()>>,
    opening: Option<(Key, Option<(String, crate::splits::Direction)>)>,
    pub split: Option<(String, crate::splits::Direction)>,
    transfers: transfers::Transfers,
    pub histories: HashMap<Key, (Instant, Option<Vec<protocol::RemoteConversation>>)>,
}

pub fn init(cx: &mut App) {
    let path = crate::paths::data_dir().join("uplink.json");
    let (identity, list) = devices::load(&path);
    let (events, mut received) = futures::channel::mpsc::unbounded();
    let shared = Shared::new(identity, Hello::default(), events);
    let entity = cx.new(|_| {
        let uplink = Uplink {
            shared,
            devices: list,
            path,
            server: None,
            discovery: None,
            announced: None,
            error: None,
            links: HashMap::new(),
            nearby: Vec::new(),
            dialog: None,
            note: None,
            dialing: HashMap::new(),
            watchers: Vec::new(),
            mirrors: HashMap::new(),
            outboxes: Default::default(),
            push: None,
            opening: None,
            split: None,
            transfers: Default::default(),
            histories: HashMap::new(),
        };
        uplink.trust();
        uplink
    });
    cx.set_global(UplinkHandle(entity.clone()));
    let events_entity = entity.clone();
    cx.spawn(async move |cx| {
        while let Some(event) = received.next().await {
            events_entity.update(cx, |uplink, cx| uplink.handle(event, cx));
        }
    })
    .detach();
    let ticking = entity.clone();
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            ticking.update(cx, |uplink, cx| uplink.tick(cx));
        }
    })
    .detach();
    let settings_entity = entity.clone();
    cx.observe_global::<settings::Settings>(move |cx| settings_entity.update(cx, |uplink, cx| uplink.apply(cx)))
        .detach();
    let store_entity = entity.clone();
    cx.observe(&store::store(cx), move |_, cx| {
        store_entity.update(cx, |uplink, cx| {
            uplink.forget_closed_mirrors(cx);
            uplink.schedule_push(cx);
        })
    })
    .detach();
    entity.update(cx, |uplink, cx| uplink.apply(cx));
}

impl Uplink {
    pub fn enabled(cx: &App) -> bool {
        settings::get(cx).uplink.enabled
    }

    pub fn port(&self) -> Option<u16> {
        self.server.as_ref().map(|server| server.port)
    }

    pub fn identity(&self) -> &Key {
        &self.shared.identity.public
    }

    fn trust(&self) {
        if let Ok(mut trusted) = self.shared.trusted.lock() {
            trusted.clear();
            trusted.extend(self.devices.iter().filter(|device| !device.blocked).filter_map(Device::public));
        }
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        devices::save(&self.path, &self.shared.identity, &self.devices);
        self.trust();
        cx.notify();
    }

    pub fn device(&self, key: &Key) -> Option<&Device> {
        self.devices.iter().find(|device| device.public().as_ref() == Some(key))
    }

    fn device_mut(&mut self, key: &Key) -> Option<&mut Device> {
        self.devices.iter_mut().find(|device| device.public().as_ref() == Some(key))
    }

    pub fn device_name(&self, key: &Key) -> String {
        self.device(key).map(|device| device.name.clone()).unwrap_or_else(|| wire::fingerprint(key))
    }

    pub fn pairing(&self) -> Option<(String, Duration, u32)> {
        let pairing = self.shared.pairing.lock().ok()?;
        let current = pairing.as_ref()?;
        let left = current.expires.saturating_duration_since(Instant::now());
        (!left.is_zero()).then(|| (current.code.clone(), left, current.attempts))
    }

    fn apply(&mut self, cx: &mut Context<Self>) {
        let config = settings::get(cx).uplink.clone();
        if !config.enabled {
            if self.server.is_some() || self.discovery.is_some() || !self.links.is_empty() {
                self.stop(cx);
            }
            return;
        }
        let hello = Hello {
            name: config.device_name(),
            os: std::env::consts::OS.into(),
            version: env!("CARGO_PKG_VERSION").into(),
            port: config.port,
            protocol: protocol::PROTOCOL,
        };
        let renamed = self.shared.hello.lock().map(|current| *current != hello).unwrap_or(true);
        if let Ok(mut current) = self.shared.hello.lock() {
            *current = hello.clone();
        }
        if self.server.as_ref().is_some_and(|server| server.port != config.port) {
            self.server = None;
        }
        if self.server.is_none() {
            match net::listen(config.port, &self.shared) {
                Ok(server) => {
                    self.server = Some(server);
                    self.error = None;
                }
                Err(error) => {
                    self.error = Some(format!("Port {} is not available: {error}", config.port));
                }
            }
        }
        let moved = self.shared.excluded.lock().map(|current| *current != config.excluded_networks).unwrap_or(false);
        if moved {
            if let Ok(mut current) = self.shared.excluded.lock() {
                *current = config.excluded_networks.clone();
            }
            self.discovery = None;
            self.announced = None;
            let incoming: Vec<Key> =
                self.links.iter().filter(|(_, link)| !link.initiator).map(|(key, _)| *key).collect();
            for key in incoming {
                self.disconnect(&key, cx);
            }
        }
        if self.discovery.is_none() {
            self.discovery = Discovery::start(&self.shared);
        }
        self.announce();
        if renamed {
            for link in self.links.values() {
                link.outbox.send(&Message::Hello(hello.clone()));
            }
        }
        cx.notify();
    }

    fn announce(&mut self) {
        let Some(discovery) = &self.discovery else { return };
        let Ok(hello) = self.shared.hello.lock().map(|hello| hello.clone()) else { return };
        let state = (hello.name.clone(), hello.port);
        if self.announced.as_ref() != Some(&state) && self.server.is_some() {
            discovery.announce(&self.shared.identity.public, &state.0, state.1);
            self.announced = Some(state);
        }
    }

    fn stop(&mut self, cx: &mut Context<Self>) {
        self.server = None;
        self.discovery = None;
        self.announced = None;
        self.error = None;
        if let Ok(mut pairing) = self.shared.pairing.lock() {
            *pairing = None;
        }
        for link in self.links.values() {
            link.outbox.close();
        }
        self.links.clear();
        if let Ok(mut outboxes) = self.outboxes.lock() {
            outboxes.clear();
        }
        self.nearby.clear();
        self.dialing.clear();
        self.close_dialog(cx);
        self.note = None;
        let watched: Vec<String> = self
            .watchers
            .drain(..)
            .map(|watcher| {
                watcher.alive.store(false, Ordering::Release);
                watcher.tab
            })
            .collect();
        for tab in watched {
            self.release(&tab, cx);
        }
        let mirrors: Vec<String> = self.mirrors.keys().cloned().collect();
        for id in mirrors {
            self.close_mirror(&id, cx);
        }
        cx.notify();
    }

    fn tick(&mut self, cx: &mut Context<Self>) {
        if !Uplink::enabled(cx) {
            return;
        }
        if self.server.is_none() {
            self.apply(cx);
        }
        if let Ok(mut pairing) = self.shared.pairing.lock()
            && pairing.as_ref().is_some_and(|current| current.expires <= Instant::now())
        {
            *pairing = None;
        }
        self.keep_code(cx);
        self.announce();
        self.dial(cx);
        self.watchers.retain(|watcher| watcher.alive.load(Ordering::Acquire));
    }

    fn dial(&mut self, cx: &mut Context<Self>) {
        let port = settings::get(cx).uplink.port;
        let now = Instant::now();
        let candidates: Vec<(Key, Vec<String>)> = self
            .devices
            .iter()
            .filter(|device| !device.blocked)
            .filter_map(|device| {
                let key = device.public()?;
                if self.links.contains_key(&key) {
                    return None;
                }
                if self.dialing.get(&key).is_some_and(|dialing| dialing.busy || dialing.next > now) {
                    return None;
                }
                let mut addresses: Vec<String> = device.manual.clone();
                for nearby in self.nearby.iter().filter(|nearby| nearby.key == key) {
                    addresses.extend(nearby.addresses.iter().map(|address| address.to_string()));
                }
                addresses.extend(device.addresses.iter().cloned());
                let mut seen = std::collections::HashSet::new();
                addresses.retain(|address| seen.insert(address.clone()));
                (!addresses.is_empty()).then_some((key, addresses))
            })
            .collect();
        for (key, addresses) in candidates {
            let dialing = self.dialing.entry(key).or_insert(Dialing {
                busy: false,
                next: now,
                backoff: Duration::from_secs(2),
                error: None,
            });
            dialing.busy = true;
            net::dial(key, addresses, port, &self.shared);
        }
    }

    pub fn dial_error(&self, key: &Key) -> Option<&str> {
        self.dialing.get(key).and_then(|dialing| dialing.error.as_deref())
    }

    fn handle(&mut self, event: NetEvent, cx: &mut Context<Self>) {
        if !Uplink::enabled(cx) {
            if let NetEvent::Connected(connection) = event {
                connection.outbox.close();
            }
            return;
        }
        match event {
            NetEvent::Connected(connection) => self.connected(connection, cx),
            NetEvent::Message(conn, message) => self.message(conn, message, cx),
            NetEvent::Input(conn, tab, data) => self.host_input(conn, &tab, &data, cx),
            NetEvent::Chunk(conn, id, data) => {
                if let Some((key, _)) = self.link_of(conn) {
                    self.chunk(key, id, data);
                }
            }
            NetEvent::Closed(conn) => self.closed(conn, cx),
            NetEvent::DialFailed(key, error) => {
                if let Some(dialing) = self.dialing.get_mut(&key) {
                    dialing.busy = false;
                    dialing.next = Instant::now() + dialing.backoff;
                    dialing.backoff = (dialing.backoff * 2).min(Duration::from_secs(30));
                    dialing.error = Some(error);
                }
                cx.notify();
            }
            NetEvent::Paired { key, hello, address, access } => self.paired(key, hello, address, access, cx),
            NetEvent::Approve { key, hello, sas, answer } => self.asked(key, hello, sas, answer, cx),
            NetEvent::Declined { key, name } => self.declined(key, name, cx),
            NetEvent::Confirm { name, sas } => self.confirmed(name, sas, cx),
            NetEvent::PairingFailed { attempts, ended } => self.pairing_failed(attempts, ended, cx),
            NetEvent::JoinFailed(reason) => self.join_failed(reason, cx),
            NetEvent::Nearby(found) => {
                if let Some(dialing) = self.dialing.get_mut(&found.key)
                    && !dialing.busy
                {
                    dialing.next = Instant::now();
                    dialing.backoff = Duration::from_secs(2);
                }
                self.nearby.retain(|nearby| nearby.fullname != found.fullname && nearby.key != found.key);
                if self.nearby.len() < NEARBY {
                    self.nearby.push(found);
                }
                cx.notify();
            }
            NetEvent::Gone(fullname) => {
                self.nearby.retain(|nearby| nearby.fullname != fullname);
                cx.notify();
            }
        }
    }

    fn connected(&mut self, connection: Connection, cx: &mut Context<Self>) {
        let key = connection.key;
        if connection.initiator
            && let Some(dialing) = self.dialing.get_mut(&key)
        {
            dialing.busy = false;
            dialing.next = Instant::now() + dialing.backoff;
        }
        if !self.device(&key).is_some_and(|device| !device.blocked) {
            connection.outbox.close();
            return;
        }
        let initiator = |link_initiator: bool| if link_initiator { *self.identity() } else { key };
        if let Some(existing) = self.links.get(&key) {
            let (old, new) = (initiator(existing.initiator), initiator(connection.initiator));
            if existing.since.elapsed() < RACE && old != new && old < new {
                connection.outbox.close();
                return;
            }
            existing.outbox.close();
            let conn = existing.conn;
            self.drop_watchers(conn, cx);
        }
        if let Some(dialing) = self.dialing.get_mut(&key) {
            dialing.backoff = Duration::from_secs(2);
            dialing.error = None;
        }
        let address = if connection.initiator {
            connection.address
        } else {
            std::net::SocketAddr::new(connection.address.ip(), connection.hello.port.max(1))
        };
        let listens = connection.initiator || connection.hello.port > 0;
        if let Some(device) = self.device_mut(&key) {
            device.seen = now();
            if listens {
                device.learn(address.to_string());
            }
            if !connection.hello.name.is_empty() {
                device.name = connection.hello.name.chars().take(64).collect();
            }
            device.os = connection.hello.os.clone();
        }
        if let Ok(mut outboxes) = self.outboxes.lock() {
            outboxes.insert(key, connection.outbox.clone());
        }
        self.links.insert(
            key,
            Link {
                conn: connection.id,
                hello: connection.hello,
                address,
                initiator: connection.initiator,
                since: Instant::now(),
                outbox: connection.outbox,
                routes: connection.routes,
                catalog: None,
                sent: None,
            },
        );
        self.save(cx);
        self.push_catalogs(cx);
        let mirrors: Vec<String> = self
            .mirrors
            .iter()
            .filter(|(_, target)| target.key == key && target.watched)
            .map(|(id, _)| id.clone())
            .collect();
        for id in mirrors {
            self.watch(&id, cx);
        }
    }

    fn link_of(&self, conn: u64) -> Option<(Key, &Link)> {
        self.links.iter().find(|(_, link)| link.conn == conn).map(|(key, link)| (*key, link))
    }

    fn closed(&mut self, conn: u64, cx: &mut Context<Self>) {
        self.drop_watchers(conn, cx);
        let Some((key, _)) = self.link_of(conn) else { return };
        self.drop_transfers(key);
        self.links.remove(&key);
        if let Ok(mut outboxes) = self.outboxes.lock() {
            outboxes.remove(&key);
        }
        if let Some(device) = self.device_mut(&key) {
            device.seen = now();
        }
        self.update_mirrors(&key, cx);
        cx.notify();
    }

    fn drop_watchers(&mut self, conn: u64, cx: &mut Context<Self>) {
        let mut released = Vec::new();
        self.watchers.retain(|watcher| {
            if watcher.conn != conn {
                return true;
            }
            watcher.alive.store(false, Ordering::Release);
            released.push(watcher.tab.clone());
            false
        });
        for tab in released {
            self.release(&tab, cx);
        }
    }

    fn release(&mut self, tab: &str, cx: &mut Context<Self>) {
        let votes: Vec<(usize, usize)> = self
            .watchers
            .iter()
            .filter(|watcher| watcher.tab == tab && watcher.alive.load(Ordering::Acquire))
            .filter(|watcher| {
                self.device(&watcher.key).is_some_and(|device| !device.blocked && device.access == Access::Control)
            })
            .filter_map(|watcher| watcher.size)
            .collect();
        if let Some(session) = store::store(cx).read(cx).session(tab).cloned() {
            session.update(cx, |session, cx| {
                session.terminal.set_votes(votes);
                cx.notify();
            });
        }
    }

    fn size_watcher(&mut self, conn: u64, tab: &str, size: Option<(usize, usize)>, cx: &mut Context<Self>) {
        let watcher = self.watchers.iter_mut().find(|watcher| watcher.conn == conn && watcher.tab == tab);
        if let Some(watcher) = watcher {
            watcher.size = size;
            self.release(tab, cx);
        }
    }

    fn hide_unseen(&mut self, cx: &mut Context<Self>) {
        let store = store::store(cx).read(cx);
        let shown: HashSet<String> = store
            .every_workspace()
            .filter(|workspace| !workspace.closed && store.windows.contains_key(&workspace.id))
            .flat_map(|workspace| workspace.visible_tabs())
            .collect();
        let tabs = self.watchers.iter().map(|watcher| &watcher.tab).chain(self.mirrors.keys());
        let hidden: Vec<Entity<Session>> =
            tabs.filter(|tab| !shown.contains(*tab)).filter_map(|tab| store.session(tab).cloned()).collect();
        let unwatched: Vec<String> = self
            .mirrors
            .iter()
            .filter(|(id, target)| !target.watched && shown.contains(*id))
            .map(|(id, _)| id.clone())
            .collect();
        for session in hidden {
            session.update(cx, |session, _| session.terminal.hide());
        }
        for id in unwatched {
            self.watch(&id, cx);
        }
    }

    fn paired(&mut self, key: Key, hello: Hello, address: Option<String>, access: Access, cx: &mut Context<Self>) {
        let name: String = hello.name.chars().take(64).collect();
        match self.device_mut(&key) {
            Some(device) => {
                device.blocked = false;
                device.access = access;
                device.name = name.clone();
                device.os = hello.os.clone();
                if let Some(address) = address {
                    device.learn(address);
                }
            }
            None => {
                let mut device = Device {
                    key: wire::encode_key(&key),
                    name: name.clone(),
                    os: hello.os.clone(),
                    access,
                    paired: now(),
                    seen: now(),
                    ..Default::default()
                };
                if let Some(address) = address {
                    device.learn(address);
                }
                self.devices.push(device);
            }
        }
        if let Ok(mut pairing) = self.shared.pairing.lock() {
            *pairing = None;
        }
        self.dialing.remove(&key);
        self.save(cx);
        self.dialog_paired(key, name, cx);
        self.dial(cx);
    }

    fn start_pairing(&mut self, cx: &mut Context<Self>) {
        if let Ok(mut pairing) = self.shared.pairing.lock() {
            *pairing = Some(Pairing::default());
        }
        self.note = None;
        cx.notify();
    }

    fn cancel_pairing(&mut self, cx: &mut Context<Self>) {
        if let Ok(mut pairing) = self.shared.pairing.lock() {
            *pairing = None;
        }
        cx.notify();
    }

    pub fn set_access(&mut self, key: &Key, access: Access, cx: &mut Context<Self>) {
        if let Some(device) = self.device_mut(key) {
            device.access = access;
        }
        self.save(cx);
        self.enforce(cx);
    }

    pub fn set_blocked(&mut self, key: &Key, blocked: bool, cx: &mut Context<Self>) {
        if let Some(device) = self.device_mut(key) {
            device.blocked = blocked;
        }
        self.save(cx);
        if blocked {
            self.disconnect(key, cx);
        } else if let Some(dialing) = self.dialing.get_mut(key) {
            dialing.next = Instant::now();
        }
    }

    pub fn set_manual(&mut self, key: &Key, addresses: Vec<String>, cx: &mut Context<Self>) {
        if let Some(device) = self.device_mut(key) {
            device.manual = addresses;
        }
        if let Some(dialing) = self.dialing.get_mut(key) {
            dialing.next = Instant::now();
            dialing.backoff = Duration::from_secs(2);
        }
        self.save(cx);
    }

    pub fn disconnect(&mut self, key: &Key, cx: &mut Context<Self>) {
        if let Some(link) = self.links.get(key) {
            link.outbox.close();
            let conn = link.conn;
            self.closed(conn, cx);
        }
    }

    pub fn forget(&mut self, key: &Key, cx: &mut Context<Self>) {
        self.devices.retain(|device| device.public().as_ref() != Some(key));
        self.save(cx);
        self.dialing.remove(key);
        if let Some(link) = self.links.remove(key) {
            link.outbox.send(&Message::Unpair);
            if let Ok(mut outboxes) = self.outboxes.lock() {
                outboxes.remove(key);
            }
            self.drop_watchers(link.conn, cx);
            cx.spawn(async move |_, cx| {
                cx.background_executor().timer(Duration::from_millis(500)).await;
                link.outbox.close();
            })
            .detach();
        }
        let mirrors: Vec<String> =
            self.mirrors.iter().filter(|(_, target)| target.key == *key).map(|(id, _)| id.clone()).collect();
        for id in mirrors {
            self.close_mirror(&id, cx);
        }
        cx.notify();
    }

    pub fn open_by_id(&mut self, id: &str, workspace: &str, cx: &mut Context<Self>) {
        let Some(rest) = id.strip_prefix(MIRROR) else { return };
        let Some((prefix, tab)) = rest.split_once(':') else { return };
        let key = self.devices.iter().filter(|device| device.key.starts_with(prefix)).find_map(Device::public);
        if let Some(key) = key {
            self.open_mirror(key, tab, workspace, cx);
        }
    }

    pub fn protocol(&self, key: &Key) -> u32 {
        self.links.get(key).map_or(0, |link| link.hello.protocol)
    }

    pub fn watchers_of(&self, tab: &str) -> Vec<Key> {
        let mut keys: Vec<Key> = self
            .watchers
            .iter()
            .filter(|watcher| watcher.tab == tab && watcher.alive.load(Ordering::Acquire))
            .map(|watcher| watcher.key)
            .collect();
        keys.dedup();
        keys
    }

    pub fn watched_count(&self, key: &Key) -> usize {
        self.watchers.iter().filter(|watcher| watcher.key == *key && watcher.alive.load(Ordering::Acquire)).count()
    }

    fn schedule_push(&mut self, cx: &mut Context<Self>) {
        if self.links.is_empty() || self.push.is_some() {
            return;
        }
        self.push = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(150)).await;
            let _ = this.update(cx, |uplink, cx| {
                uplink.push = None;
                uplink.enforce(cx);
            });
        }));
    }

    fn enforce(&mut self, cx: &mut Context<Self>) {
        let store = store::store(cx).read(cx);
        let mut stopped = Vec::new();
        for watcher in &self.watchers {
            if !watcher.alive.load(Ordering::Acquire) {
                continue;
            }
            let reason = match store.session(&watcher.tab) {
                None => Some(Stop::Closed),
                Some(_) if !self.device(&watcher.key).is_some_and(|device| !device.blocked) => Some(Stop::NotShared),
                Some(_) if !shared(&watcher.tab, cx) => Some(Stop::NotShared),
                Some(_) => None,
            };
            if let Some(reason) = reason {
                watcher.alive.store(false, Ordering::Release);
                if let Some(link) = self.links.get(&watcher.key) {
                    link.outbox.send(&Message::Stop { tab: watcher.tab.clone(), reason });
                }
                stopped.push(watcher.tab.clone());
            }
        }
        let watched: HashSet<String> = self.watchers.iter().map(|watcher| watcher.tab.clone()).collect();
        for tab in stopped.into_iter().chain(watched) {
            self.release(&tab, cx);
        }
        self.hide_unseen(cx);
        self.push_catalogs(cx);
    }

    fn push_catalogs(&mut self, cx: &mut Context<Self>) {
        let keys: Vec<Key> = self.links.keys().copied().collect();
        for key in keys {
            let Some(device) = self.device(&key).cloned() else { continue };
            let catalog = catalog(&device, cx);
            if let Some(link) = self.links.get_mut(&key)
                && link.sent.as_ref() != Some(&catalog)
            {
                link.outbox.send(&Message::Catalog(catalog.clone()));
                link.sent = Some(catalog);
            }
        }
    }

    fn message(&mut self, conn: u64, message: Message, cx: &mut Context<Self>) {
        let Some((key, _)) = self.link_of(conn) else { return };
        match message {
            Message::Hello(hello) => {
                let name: String = hello.name.chars().take(64).collect();
                let changed = self
                    .device_mut(&key)
                    .filter(|device| device.name != name || device.os != hello.os)
                    .map(|device| {
                        device.name = name;
                        device.os = hello.os.clone();
                    })
                    .is_some();
                if let Some(link) = self.links.get_mut(&key) {
                    link.hello = hello;
                }
                if changed {
                    self.save(cx);
                    self.update_mirrors(&key, cx);
                }
            }
            Message::Catalog(catalog) => self.receive_catalog(key, catalog, cx),
            Message::Watch { tab } => self.host_watch(conn, key, tab, cx),
            Message::Unwatch { tab } => {
                self.watchers.retain(|watcher| {
                    let gone = watcher.conn == conn && watcher.tab == tab;
                    if gone {
                        watcher.alive.store(false, Ordering::Release);
                    }
                    !gone
                });
                self.release(&tab, cx);
                cx.notify();
            }
            Message::Resize { tab, cols, rows } => {
                self.size_watcher(conn, &tab, Some((cols as usize, rows as usize)), cx)
            }
            Message::Hide { tab } => self.size_watcher(conn, &tab, None, cx),
            Message::Clear { tab } => {
                if let Some(session) = self.controlled(conn, key, &tab, cx) {
                    session.update(cx, |session, cx| {
                        session.terminal.clear();
                        cx.notify();
                    });
                }
            }
            Message::Open { project, launcher, workspace, panel } => {
                self.host_open(key, &project, &launcher, &workspace, panel, cx)
            }
            Message::Opened { tab } => {
                if let Some((opener, split)) = self.opening.take()
                    && opener == key
                {
                    self.open_mirror(key, &tab, "", cx);
                    if let Some((target, direction)) = split {
                        let id = mirror_id(&key, &tab);
                        cx.defer(move |cx| {
                            let store = store::store(cx).read(cx);
                            let Some(workspace) = store.workspace_of(&target).map(|state| state.id.clone()) else {
                                return;
                            };
                            crate::actions::in_window(&workspace, cx, move |view, window, cx| {
                                let zone = crate::splits::Zone::Edge(direction);
                                crate::splits_view::arrange(view, &target, &id, zone, window, cx)
                            });
                        });
                    }
                }
            }
            Message::Stop { tab, reason } => {
                let id = mirror_id(&key, &tab);
                match reason {
                    Stop::Behind => self.watch(&id, cx),
                    Stop::NotShared | Stop::Closed => self.close_mirror(&id, cx),
                }
            }
            Message::Unpair => {
                self.note = Some(format!("{} removed this device.", self.device_name(&key)));
                self.forget(&key, cx);
            }
            Message::Seen { tab } => {
                if self.watchers.iter().any(|watcher| watcher.conn == conn && watcher.tab == tab)
                    && let Some(session) = store::store(cx).read(cx).session(&tab).cloned()
                {
                    session.update(cx, |session, cx| {
                        session.attention = false;
                        cx.notify();
                    });
                    store::store(cx).update(cx, |store, cx| store.update_badge(cx));
                }
            }
            Message::Request { id, request } => self.host_request(key, id, request, cx),
            Message::Response { id, response } => self.answered(key, id, response),
            Message::Sent { id } => self.sent(key, id),
            Message::Sending { .. } | Message::Approved | Message::Ping | Message::Unknown => {}
        }
    }

    fn controlled(&self, conn: u64, key: Key, tab: &str, cx: &App) -> Option<Entity<Session>> {
        let device = self.device(&key)?;
        if device.blocked || device.access != Access::Control {
            return None;
        }
        let watching = self
            .watchers
            .iter()
            .any(|watcher| watcher.conn == conn && watcher.tab == tab && watcher.alive.load(Ordering::Acquire));
        if !watching || !shared(tab, cx) {
            return None;
        }
        store::store(cx).read(cx).session(tab).cloned()
    }

    fn host_watch(&mut self, conn: u64, key: Key, tab: String, cx: &mut Context<Self>) {
        let Some(link) = self.links.get(&key) else { return };
        if self
            .watchers
            .iter()
            .any(|watcher| watcher.conn == conn && watcher.tab == tab && watcher.alive.load(Ordering::Acquire))
        {
            return;
        }
        let session = store::store(cx).read(cx).session(&tab).cloned();
        let allowed = self.device(&key).is_some_and(|device| !device.blocked) && shared(&tab, cx);
        let Some(session) = session.filter(|_| allowed) else {
            link.outbox.send(&Message::Stop { tab, reason: Stop::NotShared });
            return;
        };
        self.watchers.retain(|watcher| {
            let same = watcher.conn == conn && watcher.tab == tab;
            if same {
                watcher.alive.store(false, Ordering::Release);
            }
            !same
        });
        let alive = Arc::new(AtomicBool::new(true));
        let (outbox, id, flag) = (link.outbox.clone(), tab.clone(), alive.clone());
        let tap: crate::terminal::Tap = Box::new(move |feed| {
            if !flag.load(Ordering::Acquire) || outbox.is_closed() {
                return false;
            }
            if outbox.feed(&id, feed) {
                return true;
            }
            flag.store(false, Ordering::Release);
            outbox.send(&Message::Stop { tab: id.clone(), reason: Stop::Behind });
            false
        });
        session.read(cx).terminal.watch(tap);
        self.watchers.push(Watcher { conn, key, tab, alive, size: None });
        cx.notify();
        store::store(cx).update(cx, |_, cx| cx.notify());
    }

    fn host_input(&mut self, conn: u64, tab: &str, data: &[u8], cx: &mut Context<Self>) {
        let Some((key, _)) = self.link_of(conn) else { return };
        let Some(session) = self.controlled(conn, key, tab, cx) else { return };
        session.update(cx, |session, cx| {
            if session.exit.is_some() {
                return;
            }
            {
                let mut term = session.terminal.term.lock();
                term.scroll_display(alacritty_terminal::grid::Scroll::Bottom);
                term.selection = None;
            }
            session.terminal.write_remote(data);
            cx.notify();
        });
    }

    fn host_open(
        &mut self,
        key: Key,
        project: &str,
        launcher: &str,
        workspace: &str,
        panel: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(device) = self.device(&key) else { return };
        if device.blocked || device.access != Access::Control {
            return;
        }
        let path = PathBuf::from(project);
        let store_entity = store::store(cx);
        let store = store_entity.read(cx);
        let settings = settings::get(cx);
        let Some(workspace) = store
            .workspaces
            .iter()
            .filter(|item| !item.closed && store.windows.contains_key(&item.id))
            .filter(|item| workspace.is_empty() || item.name == workspace)
            .find(|item| item.project(&path).is_some_and(|project| project.uplink.unwrap_or(settings.uplink.share)))
            .map(|item| item.id.clone())
        else {
            return;
        };
        let launcher = match settings.agents.iter().find(|profile| profile.name == launcher) {
            _ if panel => Launcher::Terminal,
            Some(profile) => Launcher::Agent(profile.clone()),
            None => match settings.shells.iter().find(|shell| shell.name == launcher) {
                Some(shell) => Launcher::Shell(shell.clone()),
                None => return,
            },
        };
        let opened = store_entity.update(cx, |store, cx| store.open_tab(&workspace, &path, launcher, None, cx));
        if let Some(tab) = opened {
            self.push_catalogs(cx);
            if let Some(link) = self.links.get(&key) {
                link.outbox.send(&Message::Opened { tab });
            }
        }
    }

    fn receive_catalog(&mut self, key: Key, mut catalog: Catalog, cx: &mut Context<Self>) {
        catalog.tabs.retain(|tab| valid_tab(&tab.id));
        let previous = self.links.get(&key).and_then(|link| link.catalog.clone());
        if let Some(link) = self.links.get_mut(&key) {
            link.catalog = Some(catalog.clone());
        }
        if let Some(previous) = previous {
            self.notify_changes(key, &previous, &catalog, cx);
        }
        self.sync_remotes(&key, cx);
        self.update_mirrors(&key, cx);
        cx.notify();
    }

    fn notify_changes(&self, key: Key, previous: &Catalog, catalog: &Catalog, cx: &mut Context<Self>) {
        if !settings::get(cx).notifications {
            return;
        }
        let device = self.device_name(&key);
        for tab in &catalog.tabs {
            let Some(before) = previous.tabs.iter().find(|old| old.id == tab.id) else { continue };
            let Some(text) = tab.alert(before) else { continue };
            let id = mirror_id(&key, &tab.id);
            if store::store(cx).update(cx, |store, cx| store.watched(&id, cx)) {
                continue;
            }
            let project = catalog
                .projects
                .iter()
                .find(|project| project.path == tab.project)
                .map(|project| format!("{device} · {}", project.name))
                .unwrap_or(device.clone());
            crate::notify::post(&tab.title, &project, &text, &id);
        }
    }

    fn update_mirrors(&mut self, key: &Key, cx: &mut Context<Self>) {
        let link = self.links.get(key);
        let online = link.is_some();
        let catalog = link.and_then(|link| link.catalog.clone());
        let device = self.device_name(key);
        let targets: Vec<(String, String)> = self
            .mirrors
            .iter()
            .filter(|(_, target)| target.key == *key)
            .map(|(id, target)| (id.clone(), target.tab.clone()))
            .collect();
        for (id, tab) in targets {
            let Some(session) = store::store(cx).read(cx).session(&id).cloned() else { continue };
            let meta = catalog.as_ref().and_then(|catalog| catalog.tabs.iter().find(|item| item.id == tab).cloned());
            let project = meta.as_ref().and_then(|meta| {
                catalog
                    .as_ref()?
                    .projects
                    .iter()
                    .find(|project| project.path == meta.project)
                    .map(|project| project.name.clone())
            });
            let control = catalog.as_ref().is_some_and(|catalog| catalog.control);
            if meta.as_ref().is_some_and(|meta| meta.attention)
                && store::store(cx).update(cx, |store, cx| store.watched(&id, cx))
                && let Some(link) = self.links.get(key)
            {
                link.outbox.send(&Message::Seen { tab: tab.clone() });
            }
            session.update(cx, |session, cx| {
                let mut changed = meta.as_ref().is_some_and(|meta| session.apply_remote(meta));
                if let Some(remote) = &mut session.remote {
                    let next = Remote {
                        device: device.clone(),
                        online,
                        control: control && online,
                        project: project.clone().unwrap_or_else(|| remote.project.clone()),
                        ..remote.clone()
                    };
                    if *remote != next {
                        *remote = next;
                        changed = true;
                    }
                }
                if changed {
                    cx.notify();
                }
            });
        }
        store::store(cx).update(cx, |_, cx| cx.notify());
    }

    pub fn open_mirror(&mut self, key: Key, tab: &str, _workspace: &str, cx: &mut Context<Self>) {
        let Some(catalog) = self.links.get(&key).and_then(|link| link.catalog.clone()) else { return };
        let Some(meta) = catalog.tabs.iter().find(|item| item.id == tab) else { return };
        let workspace = self.open_remote(key, &meta.workspace, cx);
        let id = mirror_id(&key, tab);
        self.watch(&id, cx);
        if let Some(link) = self.links.get(&key) {
            link.outbox.send(&Message::Seen { tab: tab.to_string() });
        }
        cx.defer(move |cx| {
            crate::window::open(&workspace, cx);
            crate::actions::in_window(&workspace, cx, move |view, window, cx| view.reveal_tab(&id, window, cx));
        });
    }

    fn ensure_mirror(&mut self, key: Key, tab: &str, cx: &mut Context<Self>) {
        if !valid_tab(tab) {
            return;
        }
        let id = mirror_id(&key, tab);
        let store_entity = store::store(cx);
        if store_entity.read(cx).session(&id).is_none() {
            let catalog = self.links.get(&key).and_then(|link| link.catalog.clone()).unwrap_or_default();
            let meta = catalog
                .tabs
                .iter()
                .find(|item| item.id == tab)
                .cloned()
                .unwrap_or_else(|| RemoteTab { id: tab.to_string(), ..Default::default() });
            let project = catalog
                .projects
                .iter()
                .find(|project| project.path == meta.project)
                .map(|project| project.name.clone())
                .unwrap_or_default();
            let remote = Remote {
                key,
                device: self.device_name(&key),
                tab: tab.to_string(),
                project,
                cwd: meta.cwd.clone(),
                branch: meta.branch.clone(),
                changes: meta.changes,
                online: self.links.contains_key(&key),
                control: catalog.control,
            };
            let outboxes = self.outboxes.clone();
            let remote_tab = tab.to_string();
            let requests: Box<dyn Fn(Request) + Send> = Box::new(move |request| {
                let Some(outbox) = outboxes.lock().ok().and_then(|outboxes| outboxes.get(&key).cloned()) else {
                    return;
                };
                match request {
                    Request::Input(data) => {
                        outbox.input(&remote_tab, &data);
                    }
                    Request::Resize { cols, rows } => {
                        outbox.send(&Message::Resize {
                            tab: remote_tab.clone(),
                            cols: cols.min(u16::MAX as usize) as u16,
                            rows: rows.min(u16::MAX as usize) as u16,
                        });
                    }
                    Request::Hide => {
                        outbox.send(&Message::Hide { tab: remote_tab.clone() });
                    }
                    Request::Clear => {
                        outbox.send(&Message::Clear { tab: remote_tab.clone() });
                    }
                }
            });
            let (session, feeds) = Session::mirror(id.clone(), remote, &meta, requests, cx);
            store_entity.update(cx, |store, cx| store.add_mirror(session, cx));
            self.mirrors.insert(id.clone(), Target { key, tab: tab.to_string(), feeds, watched: false });
        }
    }

    fn watch(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(target) = self.mirrors.get_mut(id) else { return };
        target.watched = true;
        let target = &self.mirrors[id];
        let Some(link) = self.links.get(&target.key) else { return };
        if let Ok(mut routes) = link.routes.lock() {
            routes.insert(target.tab.clone(), target.feeds.clone());
        }
        link.outbox.send(&Message::Watch { tab: target.tab.clone() });
        let key = target.key;
        if let Some(session) = store::store(cx).read(cx).session(id).cloned() {
            session.update(cx, |session, cx| {
                session.terminal.revote();
                cx.notify();
            });
        }
        self.update_mirrors(&key, cx);
    }

    pub fn close_mirror(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(target) = self.mirrors.remove(id)
            && let Some(link) = self.links.get(&target.key)
        {
            if let Ok(mut routes) = link.routes.lock() {
                routes.remove(&target.tab);
            }
            link.outbox.send(&Message::Unwatch { tab: target.tab.clone() });
        }
        let id = id.to_string();
        store::store(cx).update(cx, |store, cx| store.remove_mirror(&id, cx));
    }

    pub fn forget_closed_mirrors(&mut self, cx: &mut Context<Self>) {
        let store = store::store(cx).read(cx);
        let gone: Vec<String> = self.mirrors.keys().filter(|id| store.session(id).is_none()).cloned().collect();
        for id in gone {
            self.close_mirror(&id, cx);
        }
    }

    pub fn request_open(
        &mut self,
        key: Key,
        project: &str,
        launcher: &str,
        workspace: &str,
        panel: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(link) = self.links.get(&key) else { return };
        link.outbox.send(&Message::Open {
            project: project.to_string(),
            launcher: launcher.to_string(),
            workspace: workspace.to_string(),
            panel,
        });
        self.opening = Some((key, self.split.take()));
        let _ = cx;
    }

    pub fn toggle(cx: &mut App) {
        settings::update(cx, |settings| settings.uplink.enabled = !settings.uplink.enabled);
    }
}

pub fn shared(tab: &str, cx: &App) -> bool {
    if is_mirror(tab) {
        return false;
    }
    let store = store::store(cx).read(cx);
    let Some(session) = store.session(tab) else { return false };
    let project =
        store.workspaces.iter().filter(|workspace| !workspace.closed).find_map(|workspace| workspace.project_of(tab));
    let Some(project) = project else { return false };
    session.read(cx).uplink.or(project.uplink).unwrap_or(settings::get(cx).uplink.share)
}

pub fn project_shared(path: &Path, cx: &App) -> bool {
    let store = store::store(cx).read(cx);
    let project =
        store.workspaces.iter().filter(|workspace| !workspace.closed).find_map(|workspace| workspace.project(path));
    project.is_some_and(|project| project.uplink.unwrap_or(settings::get(cx).uplink.share))
}

fn project_key(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn catalog(device: &Device, cx: &App) -> Catalog {
    if device.blocked {
        return Catalog::default();
    }
    let store = store::store(cx).read(cx);
    let default = settings::get(cx).uplink.share;
    let mut catalog = Catalog { control: device.access == Access::Control, ..Default::default() };
    catalog.workspaces = store
        .workspaces
        .iter()
        .map(|workspace| RemoteWorkspace {
            name: workspace.name.clone(),
            open: !workspace.closed,
            muted: !workspace.notifications.unwrap_or(true),
        })
        .collect();
    for workspace in store.workspaces.iter().filter(|workspace| !workspace.closed) {
        for project in workspace.all_projects() {
            let project_on = project.uplink.unwrap_or(default);
            let mut any = false;
            for id in project.tabs.iter().chain(&project.terminals) {
                let Some(entity) = store.session(id) else { continue };
                let session = entity.read(cx);
                if session.remote.is_some() || !session.uplink.unwrap_or(project_on) {
                    continue;
                }
                any = true;
                catalog.tabs.push(RemoteTab {
                    id: session.id.clone(),
                    title: session.display_title(),
                    kind: session.kind,
                    agent: session
                        .running_agent
                        .clone()
                        .or_else(|| session.agent.map(|kind| kind.command().to_string())),
                    program: session.program().map(String::from),
                    activity: session.activity,
                    attention: session.attention,
                    running: session.is_running(),
                    failed: matches!(session.exit, Some(code) if code != Some(0)),
                    color: session.color,
                    project: project_key(&project.path),
                    workspace: workspace.name.clone(),
                    cwd: session.cwd.as_deref().map(crate::paths::display),
                    branch: session.branch.clone(),
                    notice: session.notice.as_ref().map(|(text, _)| text.clone()),
                    muted: !store.notifies(id, cx),
                    changes: store.diffstats.get(&project.path).copied().filter(|stat| stat.files > 0),
                    info: session.agent_info.clone(),
                    panel: project.terminals.contains(id),
                });
            }
            if project_on || any {
                catalog.projects.push(RemoteProject {
                    path: project_key(&project.path),
                    name: if project.is_scratch() { String::new() } else { project.display_name() },
                    color: project.color,
                    workspace: workspace.name.clone(),
                    muted: !project.notifications.or(workspace.notifications).unwrap_or(true),
                    revision: store.revision(&project.path) + store.git_revision(&project.path),
                });
            }
        }
    }
    if catalog.control {
        let settings = settings::get(cx);
        catalog.launchers.extend(settings.agents.iter().filter(|profile| profile.is_launchable()).map(|profile| {
            RemoteLauncher {
                name: profile.name.clone(),
                agent: true,
                command: crate::agents::command_name(&profile.executable),
            }
        }));
        catalog.launchers.extend(settings.shells.iter().filter(|shell| shell.is_launchable()).map(|shell| {
            RemoteLauncher { name: shell.name.clone(), agent: false, command: crate::agents::command_name(&shell.path) }
        }));
    }
    catalog
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_tab_ids_are_plain() {
        assert!(valid_tab(&uuid::Uuid::new_v4().to_string()));
        assert!(!valid_tab(""));
        assert!(!valid_tab("../../etc/passwd"));
        assert!(!valid_tab("a:b"));
        assert!(!valid_tab(&"a".repeat(65)));
    }
}
