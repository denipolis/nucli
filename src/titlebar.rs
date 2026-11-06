use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, MouseButton, ParentElement, StatefulInteractiveElement,
    Styled, Window, div, px,
};

use crate::icons;
use crate::settings;
use crate::ui;
use crate::window::WorkspaceView;

pub fn render(view: &WorkspaceView, window: &Window, cx: &mut Context<WorkspaceView>) -> AnyElement {
    let theme = ui::theme(cx);
    let name = view.state(cx).map(|state| state.name.clone()).unwrap_or_default();
    let height = px((settings::get(cx).ui_font_size() * 2.4).round());
    let client_decorated = matches!(window.window_decorations(), gpui::Decorations::Client { .. });
    div()
        .id("titlebar")
        .flex()
        .flex_none()
        .items_center()
        .h(height)
        .pl(px(if cfg!(target_os = "macos") { 78.0 } else { 8.0 }))
        .pr(px(6.0))
        .bg(theme.surface)
        .border_b_1()
        .border_color(theme.border)
        .window_control_area(gpui::WindowControlArea::Drag)
        .on_mouse_down(MouseButton::Left, cx.listener(|view, _, _, _| view.dragging_titlebar = true))
        .on_mouse_up(MouseButton::Left, cx.listener(|view, _, _, _| view.dragging_titlebar = false))
        .on_mouse_move(cx.listener(|view, event: &gpui::MouseMoveEvent, window, _| {
            if view.dragging_titlebar && event.pressed_button == Some(MouseButton::Left) {
                view.dragging_titlebar = false;
                window.start_window_move();
            }
        }))
        .on_click(|event, window, _| {
            if event.click_count() == 2 {
                if cfg!(target_os = "macos") { window.titlebar_double_click() } else { window.zoom_window() }
            }
        })
        .child(
            div()
                .id("workspace-name")
                .flex_none()
                .px(px(6.0))
                .bg(theme.accent)
                .text_color(theme.on_accent)
                .child(ui::caps(&name))
                .on_click(cx.listener(|view, event: &gpui::ClickEvent, _, cx| {
                    let position = event.position();
                    let items = crate::actions::workspace_menu(view, cx);
                    view.open_menu(position, items, cx);
                })),
        )
        .child(div().flex_1())
        .when(client_decorated, |this| {
            let control = |id: &'static str, glyph: &'static str| {
                div()
                    .id(id)
                    .px(px(6.0))
                    .text_color(theme.muted)
                    .hover(|style| style.text_color(theme.accent))
                    .child(glyph)
            };
            let id = view.workspace.clone();
            this.child(control("window-minimize", icons::REMOVE).on_click(|_, window, _| window.minimize_window()))
                .child(control("window-zoom", icons::MAXIMIZE).on_click(|_, window, _| window.zoom_window()))
                .child(control("window-close", icons::CLOSE).on_click(move |_, window, cx| {
                    crate::actions::close_workspace_window(&id, window, cx);
                }))
        })
        .into_any_element()
}
