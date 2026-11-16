use std::cell::RefCell;

use dispatch2::DispatchQueue;
use futures::channel::oneshot::Sender;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_av_foundation::{
    AVCaptureConnection, AVCaptureDevice, AVCaptureDeviceInput, AVCaptureMetadataOutput,
    AVCaptureMetadataOutputObjectsDelegate, AVCaptureSession, AVCaptureVideoPreviewLayer,
    AVLayerVideoGravityResizeAspectFill, AVMediaTypeVideo, AVMetadataMachineReadableCodeObject, AVMetadataObject,
    AVMetadataObjectTypeQRCode,
};
use objc2_foundation::{NSArray, NSString};
use objc2_ui_kit::{
    NSTextAlignment, UIApplication, UIColor, UILabel, UIModalPresentationStyle, UITapGestureRecognizer,
    UIViewController, UIWindowScene,
};

const PREFIX: &str = "nucli-uplink://";

pub struct State {
    found: RefCell<Option<Sender<String>>>,
    session: RefCell<Option<Retained<AVCaptureSession>>>,
    controller: RefCell<Option<Retained<UIViewController>>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "NucliScanner"]
    #[ivars = State]
    struct Scanner;

    unsafe impl NSObjectProtocol for Scanner {}

    unsafe impl AVCaptureMetadataOutputObjectsDelegate for Scanner {
        #[unsafe(method(captureOutput:didOutputMetadataObjects:fromConnection:))]
        fn captured(
            &self,
            _output: &AVCaptureMetadataOutput,
            objects: &NSArray<AVMetadataObject>,
            _connection: &AVCaptureConnection,
        ) {
            for object in objects.iter() {
                let Some(code) = object.downcast_ref::<AVMetadataMachineReadableCodeObject>() else { continue };
                let Some(text) = (unsafe { code.stringValue() }) else { continue };
                let text = text.to_string();
                if text.starts_with(PREFIX) {
                    self.finish(Some(text));
                    return;
                }
            }
        }
    }

    impl Scanner {
        #[unsafe(method(cancel:))]
        fn cancel(&self, _sender: Option<&AnyObject>) {
            self.finish(None);
        }
    }
);

thread_local! {
    static ACTIVE: RefCell<Option<Retained<Scanner>>> = const { RefCell::new(None) };
}

impl Scanner {
    fn finish(&self, text: Option<String>) {
        if let Some(session) = self.ivars().session.borrow_mut().take() {
            unsafe { session.stopRunning() };
        }
        if let Some(controller) = self.ivars().controller.borrow_mut().take() {
            controller.dismissViewControllerAnimated_completion(true, None);
        }
        let found = self.ivars().found.borrow_mut().take();
        if let (Some(found), Some(text)) = (found, text) {
            let _ = found.send(text);
        }
        ACTIVE.with(|active| active.borrow_mut().take());
    }
}

pub fn root(mtm: MainThreadMarker) -> Option<Retained<UIViewController>> {
    let scenes = UIApplication::sharedApplication(mtm).connectedScenes();
    scenes.iter().find_map(|scene| {
        let scene = scene.downcast::<UIWindowScene>().ok()?;
        scene
            .windows()
            .iter()
            .find(|window| window.isKeyWindow())
            .or_else(|| scene.windows().firstObject())?
            .rootViewController()
    })
}

pub fn scan(found: Sender<String>) -> Result<(), String> {
    let mtm = MainThreadMarker::new().ok_or("The scanner must start on the main thread.")?;
    let unavailable = "This device has no camera. Paste the pairing link or type the code instead.";
    let media = unsafe { AVMediaTypeVideo }.ok_or(unavailable)?;
    let device = unsafe { AVCaptureDevice::defaultDeviceWithMediaType(media) }.ok_or(unavailable)?;
    let input = unsafe { AVCaptureDeviceInput::deviceInputWithDevice_error(&device) }
        .map_err(|_| "Nucli cannot use the camera. Allow it in Settings › Nucli › Camera.".to_string())?;
    let session = unsafe { AVCaptureSession::new() };
    let output = unsafe { AVCaptureMetadataOutput::new() };
    unsafe {
        if !session.canAddInput(&input) || !session.canAddOutput(&output) {
            return Err("The camera is busy.".into());
        }
        session.addInput(&input);
        session.addOutput(&output);
    }
    let root = root(mtm).ok_or("No window to show the camera in.")?;
    let scanner = Scanner::alloc(mtm).set_ivars(State {
        found: RefCell::new(Some(found)),
        session: RefCell::new(Some(session.clone())),
        controller: RefCell::new(None),
    });
    let scanner: Retained<Scanner> = unsafe { msg_send![super(scanner), init] };
    unsafe {
        output.setMetadataObjectsDelegate_queue(Some(ProtocolObject::from_ref(&*scanner)), Some(DispatchQueue::main()));
        output.setMetadataObjectTypes(Some(&NSArray::from_slice(&[AVMetadataObjectTypeQRCode])));
    }
    let controller = UIViewController::new(mtm);
    controller.setModalPresentationStyle(UIModalPresentationStyle::FullScreen);
    let view = controller.view().ok_or("No view for the camera.")?;
    let bounds = view.bounds();
    view.setBackgroundColor(Some(&UIColor::blackColor()));
    let preview = unsafe { AVCaptureVideoPreviewLayer::layerWithSession(&session) };
    if let Some(gravity) = unsafe { AVLayerVideoGravityResizeAspectFill } {
        unsafe { preview.setVideoGravity(gravity) };
    }
    preview.setFrame(bounds);
    view.layer().addSublayer(&preview);
    let label = UILabel::new(mtm);
    label.setText(Some(&NSString::from_str(
        "Point the camera at the QR code in Nucli's Connect a device.\n\nTap to cancel.",
    )));
    unsafe { label.setTextColor(Some(&UIColor::whiteColor())) };
    label.setNumberOfLines(0);
    label.setTextAlignment(NSTextAlignment::Center);
    let mut frame = bounds;
    frame.origin.x = 24.0;
    frame.size.width -= 48.0;
    frame.origin.y = bounds.size.height - 200.0;
    frame.size.height = 120.0;
    label.setFrame(frame);
    view.addSubview(&label);
    let tap = unsafe {
        UITapGestureRecognizer::initWithTarget_action(
            UITapGestureRecognizer::alloc(mtm),
            Some(&scanner),
            Some(sel!(cancel:)),
        )
    };
    view.addGestureRecognizer(&tap);
    root.presentViewController_animated_completion(&controller, true, None);
    *scanner.ivars().controller.borrow_mut() = Some(controller);
    unsafe { session.startRunning() };
    ACTIVE.with(|active| *active.borrow_mut() = Some(scanner));
    Ok(())
}
