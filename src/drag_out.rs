use std::path::Path;

use gpui::App;

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
pub fn start(path: &Path, _: &mut App) -> bool {
    use core_graphics::geometry::{CGPoint, CGRect, CGSize};
    use objc::declare::ClassDecl;
    use objc::runtime::{BOOL, Class, Object, Sel};
    use objc::{class, msg_send, sel, sel_impl};

    extern "C" fn operations(_: &Object, _: Sel, _: *mut Object, _: isize) -> usize {
        1
    }

    static SOURCE: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    let source = *SOURCE.get_or_init(|| {
        let mut decl = ClassDecl::new("NucliDragSource", class!(NSObject)).expect("drag source class");
        unsafe {
            decl.add_method(
                sel!(draggingSession:sourceOperationMaskForDraggingContext:),
                operations as extern "C" fn(&Object, Sel, *mut Object, isize) -> usize,
            );
        }
        let class: &Class = decl.register();
        let source: *mut Object = unsafe { msg_send![class, new] };
        source as usize
    });
    let Ok(text) = std::ffi::CString::new(path.to_string_lossy().as_bytes()) else { return false };
    unsafe {
        let string =
            |text: &std::ffi::CStr| -> *mut Object { msg_send![class!(NSString), stringWithUTF8String: text.as_ptr()] };
        let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
        let event: *mut Object = msg_send![app, currentEvent];
        if event.is_null() {
            return false;
        }
        let kind: usize = msg_send![event, type];
        let window: *mut Object = msg_send![event, window];
        if kind != 6 || window.is_null() {
            return false;
        }
        let view: *mut Object = msg_send![window, contentView];
        let path = string(&text);
        let url: *mut Object = msg_send![class!(NSURL), fileURLWithPath: path];
        let address: *mut Object = msg_send![url, absoluteString];
        let item: *mut Object = msg_send![class!(NSPasteboardItem), new];
        let item: *mut Object = msg_send![item, autorelease];
        let _: BOOL = msg_send![item, setString: address forType: string(c"public.file-url")];
        let _: BOOL = msg_send![item, setString: path forType: string(c"public.utf8-plain-text")];
        let dragging: *mut Object = msg_send![class!(NSDraggingItem), alloc];
        let dragging: *mut Object = msg_send![dragging, initWithPasteboardWriter: item];
        let dragging: *mut Object = msg_send![dragging, autorelease];
        let workspace: *mut Object = msg_send![class!(NSWorkspace), sharedWorkspace];
        let icon: *mut Object = msg_send![workspace, iconForFile: path];
        let location: CGPoint = msg_send![event, locationInWindow];
        let null: *mut Object = std::ptr::null_mut();
        let point: CGPoint = msg_send![view, convertPoint: location fromView: null];
        let frame = CGRect::new(&CGPoint::new(point.x - 16.0, point.y - 16.0), &CGSize::new(32.0, 32.0));
        let _: () = msg_send![dragging, setDraggingFrame: frame contents: icon];
        let items: *mut Object = msg_send![class!(NSArray), arrayWithObject: dragging];
        let session: *mut Object =
            msg_send![view, beginDraggingSessionWithItems: items event: event source: source as *mut Object];
        !session.is_null()
    }
}

#[cfg(windows)]
pub fn start(path: &Path, cx: &mut App) -> bool {
    let path = path.to_path_buf();
    cx.spawn(async move |_| {
        let _ = shell_drag(&path);
    })
    .detach();
    true
}

#[cfg(windows)]
fn shell_drag(path: &Path) -> windows::core::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::System::Com::{
        DVASPECT_CONTENT, FORMATETC, IDataObject, STGMEDIUM, STGMEDIUM_0, TYMED_HGLOBAL,
    };
    use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
    use windows::Win32::System::Ole::{CF_UNICODETEXT, DROPEFFECT_COPY, IDropSource};
    use windows::Win32::UI::Input::KeyboardAndMouse::GetActiveWindow;
    use windows::Win32::UI::Shell::{BHID_DataObject, IShellItem, SHCreateItemFromParsingName, SHDoDragDrop};
    use windows::core::PCWSTR;

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    unsafe {
        let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None)?;
        let data: IDataObject = item.BindToHandler(None, &BHID_DataObject)?;
        let global = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2)?;
        let text = GlobalLock(global) as *mut u16;
        if !text.is_null() {
            std::ptr::copy_nonoverlapping(wide.as_ptr(), text, wide.len());
            let _ = GlobalUnlock(global);
            let format = FORMATETC {
                cfFormat: CF_UNICODETEXT.0,
                ptd: std::ptr::null_mut(),
                dwAspect: DVASPECT_CONTENT.0,
                lindex: -1,
                tymed: TYMED_HGLOBAL.0 as u32,
            };
            let medium = STGMEDIUM {
                tymed: TYMED_HGLOBAL.0 as u32,
                u: STGMEDIUM_0 { hGlobal: global },
                pUnkForRelease: std::mem::ManuallyDrop::new(None),
            };
            let _ = data.SetData(&format, &medium, true);
        }
        SHDoDragDrop(Some(GetActiveWindow()), &data, None::<&IDropSource>, DROPEFFECT_COPY)?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn start(path: &Path, _: &mut App) -> bool {
    let set = |name: &str| std::env::var_os(name).is_some_and(|value| !value.is_empty());
    if set("WAYLAND_DISPLAY") || !set("DISPLAY") {
        return false;
    }
    let path = path.to_path_buf();
    std::thread::spawn(move || {
        let _ = xdnd::drag(&path);
    });
    true
}

#[cfg(target_os = "linux")]
mod xdnd {
    use std::path::Path;
    use std::time::{Duration, Instant};

    use x11rb::connection::Connection;
    use x11rb::protocol::Event;
    use x11rb::protocol::xproto::*;
    use x11rb::rust_connection::RustConnection;
    use x11rb::wrapper::ConnectionExt as _;
    use x11rb::{CURRENT_TIME, NONE};

    type Error = Box<dyn std::error::Error>;

    const NAMES: [&str; 14] = [
        "XdndAware",
        "XdndSelection",
        "XdndEnter",
        "XdndPosition",
        "XdndStatus",
        "XdndLeave",
        "XdndDrop",
        "XdndFinished",
        "XdndActionCopy",
        "text/uri-list",
        "UTF8_STRING",
        "text/plain;charset=utf-8",
        "text/plain",
        "TARGETS",
    ];

    struct Atoms {
        aware: Atom,
        selection: Atom,
        enter: Atom,
        position: Atom,
        status: Atom,
        leave: Atom,
        drop: Atom,
        finished: Atom,
        copy: Atom,
        uri_list: Atom,
        utf8: Atom,
        plain_utf8: Atom,
        plain: Atom,
        targets: Atom,
    }

    pub fn uri(path: &Path) -> String {
        let mut uri = String::from("file://");
        for byte in path.as_os_str().as_encoded_bytes() {
            if byte.is_ascii_alphanumeric() || b"/-._~".contains(byte) {
                uri.push(*byte as char);
            } else {
                uri.push_str(&format!("%{byte:02X}"));
            }
        }
        uri
    }

    fn send(conn: &RustConnection, window: Window, kind: Atom, data: [u32; 5]) -> Result<(), Error> {
        conn.send_event(false, window, EventMask::NO_EVENT, ClientMessageEvent::new(32, window, kind, data))?;
        Ok(())
    }

    fn aware_window(conn: &RustConnection, root: Window, aware: Atom) -> Result<Window, Error> {
        let mut window = root;
        loop {
            let child = conn.query_pointer(window)?.reply()?.child;
            if child == NONE {
                return Ok(NONE);
            }
            if conn.get_property(false, child, aware, AtomEnum::ANY, 0, 1)?.reply()?.type_ != NONE {
                return Ok(child);
            }
            window = child;
        }
    }

    fn answer(conn: &RustConnection, request: &SelectionRequestEvent, atoms: &Atoms, path: &Path) -> Result<(), Error> {
        let property = if request.property == NONE { request.target } else { request.property };
        let text = [atoms.utf8, atoms.plain_utf8, atoms.plain];
        let stored = if request.target == atoms.targets {
            let list = [atoms.targets, atoms.uri_list, atoms.utf8, atoms.plain_utf8, atoms.plain];
            conn.change_property32(PropMode::REPLACE, request.requestor, property, AtomEnum::ATOM, &list)?;
            property
        } else if request.target == atoms.uri_list {
            let list = format!("{}\r\n", uri(path));
            conn.change_property8(PropMode::REPLACE, request.requestor, property, atoms.uri_list, list.as_bytes())?;
            property
        } else if text.contains(&request.target) {
            let path = path.to_string_lossy();
            conn.change_property8(PropMode::REPLACE, request.requestor, property, request.target, path.as_bytes())?;
            property
        } else {
            NONE
        };
        let notify = SelectionNotifyEvent {
            response_type: SELECTION_NOTIFY_EVENT,
            sequence: 0,
            time: request.time,
            requestor: request.requestor,
            selection: request.selection,
            target: request.target,
            property: stored,
        };
        conn.send_event(false, request.requestor, EventMask::NO_EVENT, notify)?;
        conn.flush()?;
        Ok(())
    }

    pub fn drag(path: &Path) -> Result<(), Error> {
        let (conn, screen) = x11rb::connect(None)?;
        let root = conn.setup().roots[screen].root;
        let cookies =
            NAMES.iter().map(|name| conn.intern_atom(false, name.as_bytes())).collect::<Result<Vec<_>, _>>()?;
        let found =
            cookies.into_iter().map(|cookie| cookie.reply().map(|reply| reply.atom)).collect::<Result<Vec<_>, _>>()?;
        let [
            aware,
            selection,
            enter,
            position,
            status,
            leave,
            drop,
            finished,
            copy,
            uri_list,
            utf8,
            plain_utf8,
            plain,
            targets,
        ] = found[..]
        else {
            return Ok(());
        };
        let atoms = Atoms {
            aware,
            selection,
            enter,
            position,
            status,
            leave,
            drop,
            finished,
            copy,
            uri_list,
            utf8,
            plain_utf8,
            plain,
            targets,
        };
        let source = conn.generate_id()?;
        conn.create_window(0, source, root, -10, -10, 1, 1, 0, WindowClass::INPUT_ONLY, 0, &CreateWindowAux::new())?;
        conn.set_selection_owner(source, atoms.selection, CURRENT_TIME)?;
        conn.flush()?;
        let (mut target, mut accepted, mut last) = (NONE, false, None);
        let (started, mut dropped) = (Instant::now(), None::<Instant>);
        while started.elapsed() < Duration::from_secs(120) {
            while let Some(event) = conn.poll_for_event()? {
                match event {
                    Event::ClientMessage(message) if message.type_ == atoms.status => {
                        accepted = message.data.as_data32()[1] & 1 == 1;
                    }
                    Event::ClientMessage(message) if message.type_ == atoms.finished => return Ok(()),
                    Event::SelectionRequest(request) => answer(&conn, &request, &atoms, path)?,
                    _ => {}
                }
            }
            std::thread::sleep(Duration::from_millis(16));
            if let Some(time) = dropped {
                if time.elapsed() > Duration::from_secs(10) {
                    return Ok(());
                }
                continue;
            }
            let pointer = conn.query_pointer(root)?.reply()?;
            let under = aware_window(&conn, root, atoms.aware)?;
            if under != target {
                if target != NONE {
                    send(&conn, target, atoms.leave, [source, 0, 0, 0, 0])?;
                }
                (target, accepted, last) = (under, false, None);
                if target != NONE {
                    send(&conn, target, atoms.enter, [source, 5 << 24, atoms.uri_list, atoms.utf8, atoms.plain])?;
                }
            }
            if u16::from(pointer.mask) & u16::from(KeyButMask::BUTTON1) == 0 {
                if target == NONE {
                    return Ok(());
                }
                if !accepted {
                    send(&conn, target, atoms.leave, [source, 0, 0, 0, 0])?;
                    conn.flush()?;
                    return Ok(());
                }
                send(&conn, target, atoms.drop, [source, 0, CURRENT_TIME, 0, 0])?;
                dropped = Some(Instant::now());
            } else if target != NONE && last != Some((pointer.root_x, pointer.root_y)) {
                last = Some((pointer.root_x, pointer.root_y));
                let spot = ((pointer.root_x as u16 as u32) << 16) | pointer.root_y as u16 as u32;
                send(&conn, target, atoms.position, [source, 0, spot, CURRENT_TIME, atoms.copy])?;
            }
            conn.flush()?;
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn encodes_file_uris() {
            assert_eq!(super::uri(std::path::Path::new("/a b/ü.rs")), "file:///a%20b/%C3%BC.rs");
        }
    }
}

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
pub fn start(_: &Path, _: &mut App) -> bool {
    false
}
