use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::prelude::FluentBuilder;
use gpui::{
    Animation, App, Bounds, Div, ElementId, FocusHandle, HitboxBehavior, InteractiveElement, IntoElement,
    LongPressEvent, MouseButton, MouseDownEvent, ParentElement, Pixels, PlatformInput, Point, ScrollEvent,
    ScrollHandle, ScrollSource, SharedString, Stateful, StatefulInteractiveElement, Styled, TouchEvent, TouchId,
    TouchPhase, Window, WindowAppearance, canvas, div, point, px,
};
use nucli_core::icons;
use nucli_core::theme::{self, Theme};

use crate::prefs::{self, Appearance};

pub const TEXT: f32 = 15.0;
pub const SMALL: f32 = 12.0;
pub const ROW: f32 = 48.0;
pub const SPINNER_MS: u128 = 110;
pub const SHIFT: f32 = 24.0;

thread_local! {
    static FOCUS: RefCell<HashMap<ElementId, FocusHandle>> = RefCell::default();
    static SCROLL: RefCell<Option<ScrollHandle>> = const { RefCell::new(None) };
    static REVEAL: Cell<bool> = const { Cell::new(false) };
    static SETTLED: Cell<Option<(Point<Pixels>, Instant)>> = const { Cell::new(None) };
    static CAUGHT: Cell<Option<Point<Pixels>>> = const { Cell::new(None) };
}

pub fn reveal() {
    REVEAL.set(true);
}

fn focusable(id: impl Into<ElementId>, theme: &Theme) -> Stateful<Div> {
    let id = id.into();
    let handle = FOCUS.with_borrow(|focus| focus.get(&id).cloned());
    let (selection, text, watched) = (theme.selection, theme.text, id.clone());
    div()
        .id(id)
        .when_some(handle.clone(), |this, handle| this.track_focus(&handle))
        .focus_visible(move |style| style.bg(selection).text_color(text))
        .child(
            canvas(move |bounds, window, cx| watch(watched, handle, bounds, window, cx), |_, _, _, _| {})
                .absolute()
                .inset_0(),
        )
}

fn watch(id: ElementId, handle: Option<FocusHandle>, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
    let Some(handle) = handle else {
        FOCUS.with_borrow_mut(|focus| focus.insert(id, cx.focus_handle().tab_stop(true)));
        window.request_animation_frame();
        return;
    };
    if !handle.is_focused(window) || !REVEAL.take() {
        return;
    }
    let Some(scroll) = SCROLL.with_borrow(Clone::clone) else { return };
    let visible = window.content_mask().bounds;
    let shift = if bounds.top() < visible.top() {
        visible.top() - bounds.top()
    } else if bounds.bottom() > visible.bottom() {
        visible.bottom() - bounds.bottom()
    } else {
        return;
    };
    let offset = scroll.offset();
    scroll.set_offset(point(offset.x, offset.y + shift));
    window.request_animation_frame();
}

pub fn hold(on: impl Fn(&(), &mut Window, &mut App) + 'static) -> impl IntoElement {
    let on = Rc::new(on);
    canvas(
        |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
        move |_, hitbox, window, _| {
            let (pressed, clicked, held) = (on.clone(), on.clone(), hitbox.clone());
            window.on_mouse_event(move |event: &LongPressEvent, phase, window, cx| {
                if phase.bubble()
                    && event.phase == TouchPhase::Started
                    && held.is_hovered_at(event.start_position, window)
                    && !window.default_prevented()
                {
                    window.prevent_default();
                    cx.stop_propagation();
                    pressed(&(), window, cx);
                }
            });
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                if phase.bubble() && event.button == MouseButton::Right && hitbox.is_hovered(window) {
                    cx.stop_propagation();
                    clicked(&(), window, cx);
                }
            });
        },
    )
    .absolute()
    .inset_0()
}

pub fn taps() -> impl IntoElement {
    canvas(
        |_, _, _| SCROLL.set(None),
        |_, _, window, _| {
            window.on_mouse_event(|event: &ScrollEvent, phase, window, cx| {
                if phase.capture() && event.source == ScrollSource::Touch {
                    caught(event, window, cx);
                }
            })
        },
    )
    .absolute()
}

fn caught(event: &ScrollEvent, window: &mut Window, cx: &mut App) {
    let Some(offset) = SCROLL.with_borrow(|scroll| scroll.as_ref().map(ScrollHandle::offset)) else {
        CAUGHT.set(None);
        return;
    };
    let now = Instant::now();
    let still = match SETTLED.get() {
        Some((anchor, since)) if (offset - anchor).magnitude() < 2.0 => now - since > Duration::from_millis(100),
        _ => {
            SETTLED.set(Some((offset, now)));
            false
        }
    };
    let resting = event.delta.pixel_delta(px(1.0)) == Point::default();
    match event.touch_phase {
        TouchPhase::Started if resting => CAUGHT.set(still.then_some(event.position)),
        TouchPhase::Ended if resting && CAUGHT.take() == Some(event.position) => {
            let position = event.position;
            window.defer(cx, move |window, cx| {
                for phase in [TouchPhase::Started, TouchPhase::Ended] {
                    let touch =
                        TouchEvent { id: TouchId(u64::MAX), phase, position, predicted_position: None, force: None };
                    window.dispatch_event(PlatformInput::Touch(touch), cx);
                }
            });
        }
        _ => CAUGHT.set(None),
    }
}

pub fn scroll(id: impl Into<ElementId>, handle: &ScrollHandle) -> Stateful<Div> {
    let current = handle.clone();
    div()
        .id(id)
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .track_scroll(handle)
        .child(canvas(move |_, _, _| SCROLL.set(Some(current)), |_, _, _, _| {}).absolute().inset_0())
}

pub fn theme(window: &Window, cx: &App) -> Theme {
    let prefs = prefs::get(cx);
    let dark = match prefs.appearance {
        Appearance::Dark => true,
        Appearance::Light => false,
        Appearance::System => matches!(window.appearance(), WindowAppearance::Dark | WindowAppearance::VibrantDark),
    };
    let theme = theme::resolve(&prefs.theme, dark, &[]);
    status_bar(theme.dark);
    theme
}

#[cfg(target_os = "ios")]
fn status_bar(dark: bool) {
    use gpui_ios::StatusBarContentStyle::{Dark, Light};
    gpui_ios::ios::set_status_bar_style(if dark { Light } else { Dark });
}

#[cfg(not(target_os = "ios"))]
fn status_bar(_: bool) {}

pub fn spinner(time: u128) -> &'static str {
    icons::SPINNER[(time / SPINNER_MS) as usize % icons::SPINNER.len()]
}

pub fn bar(theme: &Theme) -> Div {
    div().flex().flex_none().items_center().gap(px(4.0)).h(px(ROW)).px(px(6.0)).border_b_1().border_color(theme.border)
}

pub fn title(text: impl Into<SharedString>) -> Div {
    div().flex_1().min_w_0().px(px(6.0)).truncate().child(text.into())
}

pub fn button(id: impl Into<ElementId>, label: impl Into<SharedString>, theme: &Theme) -> Stateful<Div> {
    focusable(id, theme)
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .min_w(px(40.0))
        .h(px(40.0))
        .px(px(8.0))
        .text_color(theme.accent)
        .active(|style| style.bg(theme.hover))
        .child(label.into())
}

pub fn key(id: impl Into<ElementId>, label: impl Into<SharedString>, theme: &Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .min_w(px(34.0))
        .h(px(40.0))
        .px(px(6.0))
        .text_size(px(14.0))
        .text_color(theme.accent)
        .active(|style| style.bg(theme.hover))
        .child(label.into())
}

pub fn primary(id: impl Into<ElementId>, label: impl Into<SharedString>, theme: &Theme) -> Stateful<Div> {
    focusable(id, theme)
        .flex()
        .items_center()
        .justify_center()
        .h(px(ROW))
        .px(px(16.0))
        .bg(theme.accent)
        .text_color(theme.on_accent)
        .active(|style| style.bg(theme.text).text_color(theme.background))
        .child(label.into())
}

pub fn secondary(id: impl Into<ElementId>, label: impl Into<SharedString>, theme: &Theme) -> Stateful<Div> {
    focusable(id, theme)
        .flex()
        .items_center()
        .justify_center()
        .h(px(ROW))
        .px(px(16.0))
        .border_1()
        .border_color(theme.border)
        .text_color(theme.accent)
        .active(|style| style.bg(theme.hover))
        .child(label.into())
}

pub fn section(text: impl Into<SharedString>, theme: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .mt(px(-1.0))
        .px(px(16.0))
        .pt(px(21.0))
        .pb(px(6.0))
        .bg(theme.background)
        .text_size(px(SMALL))
        .text_color(theme.faint)
        .child(text.into())
        .child(div().flex_1().h(px(1.0)).bg(theme.border))
}

pub fn row(id: impl Into<ElementId>, theme: &Theme) -> Stateful<Div> {
    focusable(id, theme)
        .flex()
        .items_center()
        .gap(px(12.0))
        .min_h(px(ROW))
        .px(px(16.0))
        .py(px(8.0))
        .border_b_1()
        .border_color(theme.border)
        .active(|style| style.bg(theme.hover))
}

pub fn hint(text: impl Into<SharedString>, theme: &Theme) -> Div {
    div().px(px(16.0)).py(px(8.0)).text_size(px(SMALL)).text_color(theme.muted).child(text.into())
}

pub fn choice(on: bool, theme: &Theme) -> Div {
    div().flex_none().text_color(if on { theme.accent } else { theme.faint }).child(if on { "[x]" } else { "[ ]" })
}

pub fn tabs<const N: usize>(
    labels: [(&'static str, &'static str); N],
    active: usize,
    theme: &Theme,
) -> [Stateful<Div>; N] {
    let mut index = 0;
    labels.map(|(glyph, label)| {
        let on = index == active;
        index += 1;
        focusable(label, theme)
            .flex()
            .items_center()
            .gap(px(6.0))
            .h(px(32.0))
            .px(px(10.0))
            .text_color(if on { theme.on_accent } else { theme.muted })
            .when(on, |this| this.bg(theme.accent))
            .child(glyph)
            .child(label)
    })
}

pub fn label_value(label: &'static str, value: impl IntoElement, theme: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(12.0))
        .min_h(px(ROW))
        .px(px(16.0))
        .border_b_1()
        .border_color(theme.border)
        .child(div().flex_none().w(px(110.0)).text_color(theme.muted).child(label))
        .child(div().flex_1().min_w_0().child(value))
}

pub fn count(number: usize, noun: &str) -> String {
    if number == 1 { format!("1 {noun}") } else { format!("{number} {noun}s") }
}

pub fn motion() -> Animation {
    Animation::new(std::time::Duration::from_millis(220)).with_easing(gpui::ease_out_quint())
}
