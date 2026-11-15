use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{CStr, c_char, c_void};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::ptr::null_mut;
use std::sync::Arc;

use dispatch2::DispatchQueue;
use futures::channel::mpsc::UnboundedSender;
use nucli_core::uplink::net::{Nearby, NetEvent, Shared};
use nucli_core::uplink::wire::{self, Key};

type Service = *mut c_void;

const MORE_COMING: u32 = 0x1;
const ADD: u32 = 0x2;
const IPV4: u32 = 0x1;
const IPV6: u32 = 0x2;
const KIND: &CStr = c"_nucli-uplink._tcp";

type BrowseReply = extern "C" fn(Service, u32, u32, i32, *const c_char, *const c_char, *const c_char, *mut c_void);
type ResolveReply =
    extern "C" fn(Service, u32, u32, i32, *const c_char, *const c_char, u16, u16, *const u8, *mut c_void);
type AddressReply = extern "C" fn(Service, u32, u32, i32, *const c_char, *const libc::sockaddr, u32, *mut c_void);

unsafe extern "C" {
    fn DNSServiceBrowse(
        service: *mut Service,
        flags: u32,
        interface: u32,
        kind: *const c_char,
        domain: *const c_char,
        reply: BrowseReply,
        context: *mut c_void,
    ) -> i32;
    fn DNSServiceResolve(
        service: *mut Service,
        flags: u32,
        interface: u32,
        name: *const c_char,
        kind: *const c_char,
        domain: *const c_char,
        reply: ResolveReply,
        context: *mut c_void,
    ) -> i32;
    fn DNSServiceGetAddrInfo(
        service: *mut Service,
        flags: u32,
        interface: u32,
        protocol: u32,
        host: *const c_char,
        reply: AddressReply,
        context: *mut c_void,
    ) -> i32;
    fn DNSServiceSetDispatchQueue(service: Service, queue: *const c_void) -> i32;
    fn DNSServiceRefDeallocate(service: Service);
    fn TXTRecordGetValuePtr(length: u16, record: *const c_void, key: *const c_char, value: *mut u8) -> *const c_void;
}

struct Found {
    resolver: Service,
    addresser: Service,
    fullname: String,
    key: Key,
    name: String,
    pairing: bool,
    port: u16,
    addresses: Vec<SocketAddr>,
}

struct State {
    events: UnboundedSender<NetEvent>,
    own: Key,
    next: usize,
    found: HashMap<usize, Found>,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

pub struct Browser(Service);

impl Drop for Browser {
    fn drop(&mut self) {
        unsafe { DNSServiceRefDeallocate(self.0) };
        STATE.with(|state| {
            for found in state.borrow_mut().take().into_iter().flat_map(|state| state.found.into_values()) {
                release(found.resolver);
                release(found.addresser);
            }
        });
    }
}

fn on_main(service: Service) {
    unsafe { DNSServiceSetDispatchQueue(service, (DispatchQueue::main() as *const DispatchQueue).cast()) };
}

fn release(service: Service) {
    if !service.is_null() {
        unsafe { DNSServiceRefDeallocate(service) };
    }
}

fn text(pointer: *const c_char) -> String {
    if pointer.is_null() { String::new() } else { unsafe { CStr::from_ptr(pointer) }.to_string_lossy().into_owned() }
}

fn post(event: NetEvent) {
    STATE.with(|state| {
        if let Some(state) = state.borrow().as_ref() {
            let _ = state.events.unbounded_send(event);
        }
    });
}

pub fn start(shared: &Arc<Shared>) -> Option<Browser> {
    STATE.with(|state| {
        *state.borrow_mut() =
            Some(State { events: shared.events.clone(), own: shared.identity.public, next: 0, found: HashMap::new() })
    });
    let mut service: Service = null_mut();
    let error = unsafe { DNSServiceBrowse(&mut service, 0, 0, KIND.as_ptr(), std::ptr::null(), browsed, null_mut()) };
    if error != 0 {
        return None;
    }
    on_main(service);
    Some(Browser(service))
}

extern "C" fn browsed(
    _: Service,
    flags: u32,
    interface: u32,
    error: i32,
    name: *const c_char,
    kind: *const c_char,
    domain: *const c_char,
    _: *mut c_void,
) {
    if error != 0 || name.is_null() {
        return;
    }
    let fullname = format!("{}.{}{}", text(name), text(kind), text(domain));
    let gone: Vec<Found> = STATE.with(|state| {
        let mut state = state.borrow_mut();
        let Some(state) = state.as_mut() else { return Vec::new() };
        let ids: Vec<usize> =
            state.found.iter().filter(|(_, found)| found.fullname == fullname).map(|(id, _)| *id).collect();
        ids.into_iter().filter_map(|id| state.found.remove(&id)).collect()
    });
    for found in gone {
        release(found.resolver);
        release(found.addresser);
    }
    if flags & ADD == 0 {
        post(NetEvent::Gone(fullname));
        return;
    }
    let Some(id) = STATE.with(|state| {
        state.borrow_mut().as_mut().map(|state| {
            state.next += 1;
            state.next
        })
    }) else {
        return;
    };
    let mut service: Service = null_mut();
    let error =
        unsafe { DNSServiceResolve(&mut service, 0, interface, name, kind, domain, resolved, id as *mut c_void) };
    if error != 0 {
        return;
    }
    on_main(service);
    let found = Found {
        resolver: service,
        addresser: null_mut(),
        fullname,
        key: [0; 32],
        name: String::new(),
        pairing: false,
        port: 0,
        addresses: Vec::new(),
    };
    STATE.with(|state| {
        if let Some(state) = state.borrow_mut().as_mut() {
            state.found.insert(id, found);
        }
    });
}

#[allow(clippy::too_many_arguments)]
extern "C" fn resolved(
    _: Service,
    _: u32,
    interface: u32,
    error: i32,
    _: *const c_char,
    host: *const c_char,
    port: u16,
    length: u16,
    record: *const u8,
    context: *mut c_void,
) {
    let id = context as usize;
    let value = |key: &CStr| -> Option<String> {
        let mut size = 0u8;
        let pointer = unsafe { TXTRecordGetValuePtr(length, record.cast(), key.as_ptr(), &mut size) };
        if pointer.is_null() {
            return None;
        }
        let bytes = unsafe { std::slice::from_raw_parts(pointer.cast::<u8>(), size as usize) };
        Some(String::from_utf8_lossy(bytes).into_owned())
    };
    let key = value(c"id").and_then(|text| wire::decode_key(&text));
    let name: String = value(c"name").unwrap_or_default().chars().take(64).collect();
    let pairing = value(c"pairing").as_deref() == Some("1");
    let (resolve, stale) = STATE.with(|state| {
        let mut state = state.borrow_mut();
        let Some(state) = state.as_mut() else { return (false, None) };
        let own = state.own;
        let Some(key) = key.filter(|key| error == 0 && *key != own) else {
            return (false, state.found.remove(&id));
        };
        let Some(found) = state.found.get_mut(&id) else { return (false, None) };
        found.key = key;
        found.name = name;
        found.pairing = pairing;
        found.port = u16::from_be(port);
        (found.addresser.is_null(), None)
    });
    if let Some(stale) = stale {
        release(stale.resolver);
        release(stale.addresser);
        return;
    }
    if !resolve {
        if let Some(event) = nearby(id) {
            post(event);
        }
        return;
    }
    let mut service: Service = null_mut();
    let error = unsafe { DNSServiceGetAddrInfo(&mut service, 0, interface, IPV4 | IPV6, host, addressed, context) };
    if error != 0 {
        return;
    }
    on_main(service);
    STATE.with(|state| {
        if let Some(found) = state.borrow_mut().as_mut().and_then(|state| state.found.get_mut(&id)) {
            found.addresser = service;
        } else {
            release(service);
        }
    });
}

fn nearby(id: usize) -> Option<NetEvent> {
    STATE.with(|state| {
        let state = state.borrow();
        let found = state.as_ref()?.found.get(&id)?;
        if found.addresses.is_empty() {
            return None;
        }
        let mut addresses = found.addresses.clone();
        addresses.sort_by_key(|address| (address.is_ipv6(), address.ip().is_loopback()));
        addresses.truncate(4);
        Some(NetEvent::Nearby(Nearby {
            fullname: found.fullname.clone(),
            key: found.key,
            name: found.name.clone(),
            addresses,
            pairing: found.pairing,
        }))
    })
}

fn ip(address: *const libc::sockaddr) -> Option<IpAddr> {
    if address.is_null() {
        return None;
    }
    match unsafe { (*address).sa_family } as i32 {
        libc::AF_INET => {
            let v4 = unsafe { &*(address as *const libc::sockaddr_in) };
            Some(IpAddr::V4(Ipv4Addr::from(u32::from_be(v4.sin_addr.s_addr))))
        }
        libc::AF_INET6 => {
            let v6 = unsafe { &*(address as *const libc::sockaddr_in6) };
            Some(IpAddr::V6(Ipv6Addr::from(v6.sin6_addr.s6_addr)))
        }
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
extern "C" fn addressed(
    _: Service,
    flags: u32,
    _: u32,
    error: i32,
    _: *const c_char,
    address: *const libc::sockaddr,
    _: u32,
    context: *mut c_void,
) {
    let id = context as usize;
    let ip = if error == 0 { ip(address) } else { None };
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let Some(found) = state.as_mut().and_then(|state| state.found.get_mut(&id)) else { return };
        if let Some(ip) = ip.filter(|ip| !matches!(ip, IpAddr::V6(v6) if v6.segments()[0] & 0xffc0 == 0xfe80)) {
            let socket = SocketAddr::new(ip, found.port);
            if !found.addresses.contains(&socket) {
                found.addresses.push(socket);
            }
        }
    });
    let event = if flags & MORE_COMING == 0 { nearby(id) } else { None };
    if let Some(event) = event {
        post(event);
    }
}
