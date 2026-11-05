use gpui::{
    AnyElement, Context, CursorStyle, InteractiveElement, IntoElement, MouseButton, MouseMoveEvent, Styled, Window,
    div, px,
};

use crate::persist::Side;
use crate::store;
use crate::window::WorkspaceView;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Drag {
    Sidebar,
    Panel,
    Terminal,
}

fn start(drag: Drag) -> impl Fn(&mut WorkspaceView, &gpui::MouseDownEvent, &mut Window, &mut Context<WorkspaceView>) {
    move |view, _, _, cx| {
        view.resizing = Some(drag);
        cx.stop_propagation();
    }
}

pub fn handle_horizontal(drag: Drag, cx: &mut Context<WorkspaceView>) -> AnyElement {
    div()
        .w(px(5.0))
        .mx(px(-2.0))
        .h_full()
        .flex_none()
        .cursor(CursorStyle::ResizeLeftRight)
        .on_mouse_down(MouseButton::Left, cx.listener(start(drag)))
        .into_any_element()
}

pub fn handle_vertical(_view: &WorkspaceView, cx: &mut Context<WorkspaceView>) -> AnyElement {
    div()
        .h(px(5.0))
        .mt(px(-3.0))
        .mb(px(-2.0))
        .w_full()
        .flex_none()
        .cursor(CursorStyle::ResizeUpDown)
        .on_mouse_down(MouseButton::Left, cx.listener(start(Drag::Terminal)))
        .into_any_element()
}

pub fn mouse_move(
    view: &mut WorkspaceView,
    event: &MouseMoveEvent,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) {
    let Some(drag) = view.resizing else { return };
    if event.pressed_button != Some(MouseButton::Left) {
        view.resizing = None;
        return;
    }
    let x = f32::from(event.position.x);
    let y = f32::from(event.position.y);
    let size = window.viewport_size();
    let (width, height) = (f32::from(size.width), f32::from(size.height));
    let workspace = view.workspace.clone();
    let side = view.store(cx).ui.panel_side;
    let panel_hidden = view.store(cx).ui.panel_hidden;
    store::store(cx).update(cx, |store, cx| {
        let Some(state) = store.workspace_mut(&workspace) else { return };
        let sidebar = if state.window.sidebar_hidden { 0.0 } else { state.window.sidebar_width };
        match drag {
            Drag::Sidebar => state.window.sidebar_width = x.clamp(160.0, (width * 0.5).max(160.0)),
            Drag::Panel if !panel_hidden => {
                let value = if side == Side::Left { x - sidebar } else { width - x };
                state.window.panel_width = value.clamp(160.0, (width * 0.6).max(160.0));
            }
            Drag::Terminal => {
                let status = (crate::settings::get(cx).ui_font_size() * 1.9).round();
                state.window.terminal_height = (height - status - y).clamp(80.0, (height - 160.0).max(80.0));
            }
            _ => {}
        }
        store.schedule_save(cx);
        cx.notify();
    });
}

pub fn mouse_up(view: &mut WorkspaceView, cx: &mut Context<WorkspaceView>) {
    if view.resizing.take().is_some() {
        store::store(cx).update(cx, |store, cx| store.changed(cx));
    }
}
