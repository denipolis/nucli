use std::path::Path;

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
pub fn start(path: &Path) -> bool {
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

#[cfg(not(target_os = "macos"))]
pub fn start(_: &Path) -> bool {
    false
}
