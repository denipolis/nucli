use std::time::{Duration, Instant};

use gpui::{Context, Window};

use crate::actions;
use crate::window::WorkspaceView;

const HOLD: Duration = Duration::from_millis(350);
const POLL: Duration = Duration::from_millis(10);

#[derive(Clone, Copy, PartialEq)]
pub enum Launch {
    Shell,
    Agent,
}

#[cfg(target_os = "macos")]
fn key_released() -> bool {
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventSourceKeyState(state: i32, key: u16) -> bool;
    }
    const COMBINED_SESSION: i32 = 0;
    const MODIFIERS: std::ops::RangeInclusive<u16> = 0x36..=0x3F;
    !(0..0x80u16)
        .filter(|key| !MODIFIERS.contains(key))
        .any(|key| unsafe { CGEventSourceKeyState(COMBINED_SESSION, key) })
}

#[cfg(not(target_os = "macos"))]
fn key_released() -> bool {
    false
}

pub fn press(view: &mut WorkspaceView, launch: Launch, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    if !window.modifiers().modified() {
        actions::open_launch(view, launch, window, cx);
        return;
    }
    if view.press.is_some() || view.palette.is_some() {
        return;
    }
    let started = Instant::now();
    view.press = Some((launch, started));
    cx.spawn_in(window, async move |view, cx| {
        let mut released = false;
        while !released && started.elapsed() < HOLD {
            cx.background_executor().timer(POLL).await;
            released = key_released();
        }
        let _ = view.update_in(cx, |view, window, cx| {
            let Some((launch, _)) = view.press.take_if(|press| press.1 == started) else { return };
            if released {
                actions::open_launch(view, launch, window, cx);
            } else {
                actions::choose_launch(view, launch, window, cx);
            }
        });
    })
    .detach();
}

pub fn release(view: &mut WorkspaceView, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    if let Some((launch, _)) = view.press.take() {
        actions::open_launch(view, launch, window, cx);
    }
}
