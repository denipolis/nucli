use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, Context, Div, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::icons;
use crate::session::{Activity, Kind};
use crate::settings;
use crate::store::PROGRESS_DELAY;
use crate::ui;
use crate::window::WorkspaceView;

const TRAFFIC_LIGHT_CENTER: f32 = 17.0;
const LOCATION_WIDTH: f32 = 640.0;

fn location(view: &WorkspaceView, cx: &mut Context<WorkspaceView>) -> AnyElement {
    let theme = ui::theme(cx);
    let project = view.state(cx).and_then(|state| state.current_project());
    let tint = project.and_then(|project| project.color).map(|color| color.color(theme.dark));
    let name = project.filter(|project| !project.is_scratch()).map(|project| project.display_name());
    let detail = if view.settings.is_some() {
        Some("settings".to_string())
    } else if view.file_shown(cx) {
        project.and_then(|project| project.active_file.as_deref()).map(|file| {
            let path = file.split("@@").next().unwrap_or(file);
            path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
        })
    } else {
        view.selected_session(cx).map(|session| session.read(cx).display_title())
    };
    div()
        .id("location")
        .flex()
        .items_center()
        .gap(px(8.0))
        .flex_shrink()
        .min_w_0()
        .w(px(LOCATION_WIDTH))
        .overflow_hidden()
        .px(px(10.0))
        .bg(theme.background)
        .border_1()
        .border_color(theme.border)
        .hover(|style| style.border_color(theme.muted))
        .child(div().flex_none().text_color(tint.unwrap_or(theme.accent)).child(icons::FOLDER))
        .child(div().flex_none().child(name.unwrap_or_else(|| "no project".to_string())))
        .when_some(detail, |this, detail| {
            this.child(div().flex_none().text_color(theme.faint).child("›"))
                .child(div().min_w_0().truncate().text_color(theme.muted).child(detail))
        })
        .on_click(cx.listener(|view, _, window, cx| crate::actions::command_palette(view, window, cx)))
        .into_any_element()
}

fn counts(view: &WorkspaceView, cx: &App) -> (usize, usize, usize) {
    let store = view.store(cx);
    let mut counts = (0, 0, 0);
    let tabs = view.state(cx).into_iter().flat_map(|state| state.all_projects()).flat_map(|project| &project.tabs);
    for session in tabs.filter_map(|id| store.session(id)) {
        let session = session.read(cx);
        if session.activity == Activity::Working {
            counts.0 += 1;
        } else if session.activity == Activity::Waiting {
            counts.1 += 1;
        } else if session.attention && session.kind == Kind::Agent {
            counts.2 += 1;
        }
    }
    counts
}

fn status(view: &WorkspaceView, cx: &App) -> Div {
    let theme = ui::theme(cx);
    let store = view.store(cx);
    let line = div().flex().flex_1().items_center().justify_end().gap(px(12.0)).min_w_0();
    let spinner = icons::SPINNER[store.tick_fast % icons::SPINNER.len()];
    let mut messages: Vec<String> = store
        .progress
        .iter()
        .filter(|item| item.started.elapsed() >= PROGRESS_DELAY)
        .map(|item| item.text.clone())
        .collect();
    messages.extend(
        store
            .sessions
            .values()
            .map(|session| session.read(cx))
            .filter(|session| session.is_starting())
            .map(|session| format!("starting {}", session.display_title())),
    );
    if let Some(message) = messages.last() {
        let others = messages.len() - 1;
        return line
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .min_w_0()
                    .text_color(theme.muted)
                    .child(div().flex_none().text_color(theme.accent).child(spinner))
                    .child(div().min_w_0().truncate().child(message.clone())),
            )
            .when(others > 0, |this| {
                this.child(div().flex_none().text_color(theme.faint).child(format!("+{others}")))
            });
    }
    let (working, waiting, done) = counts(view, cx);
    let count = |glyph: &'static str, color, text: String| {
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(6.0))
            .text_color(theme.muted)
            .child(div().text_color(color).child(glyph))
            .child(text)
    };
    line.when(working > 0, |this| this.child(count(spinner, theme.accent, format!("{working} working"))))
        .when(waiting > 0, |this| this.child(count(icons::QUESTION, theme.orange, format!("{waiting} waiting"))))
        .when(done > 0, |this| this.child(count(icons::CHECK, theme.green, format!("{done} done"))))
}

pub fn render(view: &WorkspaceView, window: &Window, cx: &mut Context<WorkspaceView>) -> AnyElement {
    let theme = ui::theme(cx);
    let name = view.state(cx).map(|state| state.name.clone()).unwrap_or_default();
    let height = px((settings::get(cx).ui_font_size() * 2.4).round());
    let client_decorated = matches!(window.window_decorations(), gpui::Decorations::Client { .. });
    let location = location(view, cx);
    let status = status(view, cx);
    div()
        .id("titlebar")
        .flex()
        .flex_none()
        .items_center()
        .h(height)
        .pl(px(if cfg!(target_os = "macos") { 78.0 } else { 8.0 }))
        .pr(px(8.0))
        .gap(px(10.0))
        .bg(theme.surface)
        .border_b_1()
        .border_color(theme.border)
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
            div().flex().flex_1().min_w_0().child(
                div()
                    .id("workspace-name")
                    .flex_none()
                    .when(cfg!(target_os = "macos"), |this| {
                        this.relative().top(px(TRAFFIC_LIGHT_CENTER) - (height - px(1.0)) / 2.0)
                    })
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(theme.text)
                    .child(name)
                    .on_click(cx.listener(|view, event: &gpui::ClickEvent, _, cx| {
                        let position = event.position();
                        let items = crate::actions::workspace_menu(view, cx);
                        view.open_menu(position, items, cx);
                    })),
            ),
        )
        .child(location)
        .child(status)
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
