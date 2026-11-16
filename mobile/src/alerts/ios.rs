use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};

use block2::RcBlock;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyObject, Bool, NSObject, NSObjectProtocol, ProtocolObject};
use objc2::{ClassType, MainThreadMarker, class, define_class, msg_send};
use objc2_foundation::{NSData, NSError, NSString};
use objc2_ui_kit::{UIApplication, UIApplicationState};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotification, UNNotificationPresentationOptions,
    UNNotificationRequest, UNNotificationResponse, UNNotificationSound, UNUserNotificationCenter,
    UNUserNotificationCenterDelegate,
};

const MIX_WITH_OTHERS: usize = 1;

static ASKED: AtomicBool = AtomicBool::new(false);
static GRANTED: AtomicBool = AtomicBool::new(false);

thread_local! {
    static PLAYER: RefCell<Option<Retained<AnyObject>>> = const { RefCell::new(None) };
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "NucliAlerts"]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl UNUserNotificationCenterDelegate for Delegate {
        #[unsafe(method(userNotificationCenter:willPresentNotification:withCompletionHandler:))]
        fn will_present(
            &self,
            _center: &UNUserNotificationCenter,
            _notification: &UNNotification,
            done: &block2::DynBlock<dyn Fn(UNNotificationPresentationOptions)>,
        ) {
            done.call((UNNotificationPresentationOptions::Banner
                | UNNotificationPresentationOptions::List
                | UNNotificationPresentationOptions::Sound,));
        }

        #[unsafe(method(userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:))]
        fn did_receive(
            &self,
            _center: &UNUserNotificationCenter,
            response: &UNNotificationResponse,
            done: &block2::DynBlock<dyn Fn()>,
        ) {
            super::clicked(response.notification().request().identifier().to_string());
            done.call(());
        }
    }
);

pub fn listen() {
    let delegate: Retained<Delegate> = unsafe { msg_send![Delegate::class(), new] };
    UNUserNotificationCenter::currentNotificationCenter().setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    std::mem::forget(delegate);
}

pub fn active() -> bool {
    MainThreadMarker::new()
        .is_none_or(|marker| UIApplication::sharedApplication(marker).applicationState() == UIApplicationState::Active)
}

pub fn post(title: &str, place: &str, text: &str, id: &str) {
    let content = UNMutableNotificationContent::new();
    content.setTitle(&NSString::from_str(title));
    content.setSubtitle(&NSString::from_str(place));
    content.setThreadIdentifier(&NSString::from_str(place));
    content.setBody(&NSString::from_str(text));
    content.setSound(Some(&UNNotificationSound::defaultSound()));
    let request = UNNotificationRequest::requestWithIdentifier_content_trigger(&NSString::from_str(id), &content, None);
    UNUserNotificationCenter::currentNotificationCenter().addNotificationRequest_withCompletionHandler(&request, None);
}

pub fn keep(on: bool) {
    if on && !ASKED.swap(true, Ordering::Relaxed) {
        let answered =
            RcBlock::new(|granted: Bool, _: *mut NSError| GRANTED.store(granted.as_bool(), Ordering::Relaxed));
        UNUserNotificationCenter::currentNotificationCenter().requestAuthorizationWithOptions_completionHandler(
            UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
            &answered,
        );
    }
    let on = on && GRANTED.load(Ordering::Relaxed);
    PLAYER.with_borrow_mut(|player| {
        if !on {
            if let Some(player) = player.take() {
                let _: () = unsafe { msg_send![&*player, stop] };
            }
            return;
        }
        if player.as_ref().is_some_and(|player| unsafe { msg_send![&**player, isPlaying] }) {
            return;
        }
        unsafe {
            let session: Retained<AnyObject> = msg_send![class!(AVAudioSession), sharedInstance];
            let category = NSString::from_str("AVAudioSessionCategoryPlayback");
            let _: Result<(), Retained<NSError>> =
                msg_send![&*session, setCategory: &*category, withOptions: MIX_WITH_OTHERS, error: _];
            let _: Result<(), Retained<NSError>> = msg_send![&*session, setActive: true, error: _];
        }
        if player.is_none() {
            *player = silent_player();
        }
        if let Some(player) = player {
            let _: bool = unsafe { msg_send![&**player, play] };
        }
    });
}

fn silent_player() -> Option<Retained<AnyObject>> {
    let data = NSData::with_bytes(&silence());
    unsafe {
        let allocated: Allocated<AnyObject> = msg_send![class!(AVAudioPlayer), alloc];
        let player: Result<Retained<AnyObject>, Retained<NSError>> =
            msg_send![allocated, initWithData: &*data, error: _];
        let player = player.ok()?;
        let _: () = msg_send![&*player, setNumberOfLoops: -1isize];
        let _: () = msg_send![&*player, setVolume: 0.0f32];
        Some(player)
    }
}

fn silence() -> Vec<u8> {
    let (rate, size) = (8000u32, 16000u32);
    let mut wav = Vec::with_capacity(44 + size as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + size).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&size.to_le_bytes());
    wav.resize(44 + size as usize, 0);
    wav
}
