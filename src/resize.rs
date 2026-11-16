use gpui::{
    AnyElement, Context, CursorStyle, InteractiveElement, IntoElement, MouseButton, MouseMoveEvent, Pixels, Point,
    Styled, Window, div, px,
};

use crate::persist::Dock;
use crate::store;
use crate::window::WorkspaceView;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Drag {
    Terminal,
    Stack(Dock, usize),
    Panels(Dock, usize, usize),
}

pub struct Resizing {
    drag: Drag,
    origin: Point<Pixels>,
    start: (f32, f32),
}

fn start_sizes(view: &WorkspaceView, drag: Drag, cx: &Context<WorkspaceView>) -> (f32, f32) {
    match drag {
        Drag::Terminal => (view.state(cx).map(|state| state.window.terminal_height).unwrap_or(240.0), 0.0),
        Drag::Stack(dock, stack) => {
            (view.store(cx).ui.docks.dock(dock).get(stack).map(|stack| stack.size).unwrap_or(0.0), 0.0)
        }
        Drag::Panels(dock, stack, panel) => {
            let bounds = view.dock_bounds.borrow();
            let length = |panel: usize| {
                bounds
                    .get(&(dock, stack, panel))
                    .map(|bounds| f32::from(if dock.side() { bounds.size.height } else { bounds.size.width }))
                    .unwrap_or(0.0)
            };
            (length(panel), length(panel + 1))
        }
    }
}

pub fn start(
    drag: Drag,
) -> impl Fn(&mut WorkspaceView, &gpui::MouseDownEvent, &mut Window, &mut Context<WorkspaceView>) {
    move |view, event, _, cx| {
        let start = start_sizes(view, drag, cx);
        view.resizing = Some(Resizing { drag, origin: event.position, start });
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

pub fn handle_vertical(drag: Drag, cx: &mut Context<WorkspaceView>) -> AnyElement {
    div()
        .h(px(5.0))
        .mt(px(-3.0))
        .mb(px(-2.0))
        .w_full()
        .flex_none()
        .cursor(CursorStyle::ResizeUpDown)
        .on_mouse_down(MouseButton::Left, cx.listener(start(drag)))
        .into_any_element()
}

pub fn mouse_move(
    view: &mut WorkspaceView,
    event: &MouseMoveEvent,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) {
    let Some(resizing) = &view.resizing else { return };
    if event.pressed_button != Some(MouseButton::Left) {
        view.resizing = None;
        return;
    }
    let (drag, start) = (resizing.drag, resizing.start);
    let dx = f32::from(event.position.x - resizing.origin.x);
    let dy = f32::from(event.position.y - resizing.origin.y);
    let size = window.viewport_size();
    let (width, height) = (f32::from(size.width), f32::from(size.height));
    let workspace = view.workspace.clone();
    store::store(cx).update(cx, |store, cx| {
        match drag {
            Drag::Terminal => {
                if let Some(state) = store.workspace_mut(&workspace) {
                    state.window.terminal_height = (start.0 - dy).clamp(80.0, (height - 160.0).max(80.0));
                }
            }
            Drag::Stack(dock, stack) => {
                let delta = match dock {
                    Dock::Left => dx,
                    Dock::Right => -dx,
                    Dock::Top => dy,
                    Dock::Bottom => -dy,
                };
                let (min, max) = if dock.side() { (120.0, width * 0.6) } else { (60.0, height * 0.6) };
                if let Some(stack) = store.ui.docks.dock_mut(dock).get_mut(stack) {
                    stack.size = (start.0 + delta).clamp(min, max.max(min));
                }
            }
            Drag::Panels(dock, stack, panel) => {
                let total = start.0 + start.1;
                let delta = if dock.side() { dy } else { dx };
                let first = (start.0 + delta).clamp(60.0, (total - 60.0).max(60.0));
                if let Some(stack) = store.ui.docks.dock_mut(dock).get_mut(stack)
                    && total > 0.0
                    && panel + 1 < stack.panels.len()
                {
                    let sum = stack.panels[panel].weight + stack.panels[panel + 1].weight;
                    stack.panels[panel].weight = sum * first / total;
                    stack.panels[panel + 1].weight = sum - stack.panels[panel].weight;
                }
            }
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
