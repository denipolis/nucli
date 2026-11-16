use std::cell::RefCell;
use std::path::PathBuf;

use futures::channel::oneshot::{self, Sender};
use objc2::rc::Retained;
use objc2::runtime::{NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_foundation::{NSArray, NSURL};
use objc2_ui_kit::{UIDocumentPickerDelegate, UIDocumentPickerViewController};
use objc2_uniform_type_identifiers::UTTypeItem;

pub struct State {
    picked: RefCell<Option<Sender<Vec<PathBuf>>>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "NucliPicker"]
    #[ivars = State]
    struct Picker;

    unsafe impl NSObjectProtocol for Picker {}

    unsafe impl UIDocumentPickerDelegate for Picker {
        #[unsafe(method(documentPicker:didPickDocumentsAtURLs:))]
        fn picked(&self, _controller: &UIDocumentPickerViewController, urls: &NSArray<NSURL>) {
            let paths = urls.iter().filter_map(|url| url.path().map(|path| PathBuf::from(path.to_string()))).collect();
            self.finish(paths);
        }

        #[unsafe(method(documentPickerWasCancelled:))]
        fn cancelled(&self, _controller: &UIDocumentPickerViewController) {
            self.finish(Vec::new());
        }
    }
);

thread_local! {
    static ACTIVE: RefCell<Option<Retained<Picker>>> = const { RefCell::new(None) };
}

impl Picker {
    fn finish(&self, paths: Vec<PathBuf>) {
        if let Some(picked) = self.ivars().picked.borrow_mut().take() {
            let _ = picked.send(paths);
        }
        ACTIVE.with(|active| active.borrow_mut().take());
    }
}

pub fn pick() -> oneshot::Receiver<Vec<PathBuf>> {
    let (sender, receiver) = oneshot::channel();
    let Some(mtm) = MainThreadMarker::new() else { return receiver };
    let Some(root) = crate::scanner::ios::root(mtm) else { return receiver };
    let types = NSArray::from_slice(&[unsafe { UTTypeItem }]);
    let controller = UIDocumentPickerViewController::initForOpeningContentTypes_asCopy(
        UIDocumentPickerViewController::alloc(mtm),
        &types,
        true,
    );
    controller.setAllowsMultipleSelection(true);
    let picker = Picker::alloc(mtm).set_ivars(State { picked: RefCell::new(Some(sender)) });
    let picker: Retained<Picker> = unsafe { msg_send![super(picker), init] };
    controller.setDelegate(Some(ProtocolObject::from_ref(&*picker)));
    root.presentViewController_animated_completion(&controller, true, None);
    ACTIVE.with(|active| *active.borrow_mut() = Some(picker));
    receiver
}
