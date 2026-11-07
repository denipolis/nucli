use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, Context, Div, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, Window, WindowControlArea, div, point, px,
};

use crate::commands;
use crate::icons;
use crate::session::{Activity, Kind};
use crate::settings;
use crate::store::PROGRESS_DELAY;
use crate::ui;
use crate::window::WorkspaceView;

const TRAFFIC_LIGHT_CENTER: f32 = 17.0;
const LOCATION_WIDTH: f32 = 427.0;
const CAPTION_BUTTON_WIDTH: f32 = 46.0;

fn location(view: &WorkspaceView, cx: &mut Context<WorkspaceView>) -> AnyElement {
    let theme = ui::theme(cx);
    let project = view.state(cx).map(|state| state.current_project().unwrap_or(&state.scratch));
    let tint = project.and_then(|project| project.color).map(|color| color.color(theme.dark));
    let name = project.map(|project| project.display_name()).unwrap_or_default();
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
        .occlude()
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
        .child(div().flex_none().child(name))
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

fn open_menu(view: &mut WorkspaceView, index: usize, cx: &mut Context<WorkspaceView>) {
    let Some(bounds) = view.title_menu_bounds.borrow().get(index).copied() else { return };
    let items = crate::menu::from_commands(&commands::window_menu(index), cx);
    view.open_menu(point(bounds.left(), bounds.bottom()), items, cx);
    view.title_menu = Some(index);
}

fn menu_bar(view: &WorkspaceView, cx: &mut Context<WorkspaceView>) -> Div {
    let theme = ui::theme(cx);
    let bounds = view.title_menu_bounds.clone();
    let mut row = div().flex().flex_none().h_full().on_children_prepainted(move |children, _, _| {
        *bounds.borrow_mut() = children;
    });
    for (index, name) in commands::WINDOW_MENUS.iter().enumerate() {
        let open = view.title_menu == Some(index);
        row = row.child(
            div()
                .id(("title-menu", index))
                .occlude()
                .flex()
                .items_center()
                .h_full()
                .px(px(7.0))
                .when(open, |this| this.bg(theme.accent).text_color(theme.on_accent))
                .when(!open, |this| {
                    this.text_color(theme.muted).hover(|style| style.bg(theme.hover).text_color(theme.text))
                })
                .child(*name)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |view, _, _, cx| {
                        if view.title_menu == Some(index) {
                            view.title_menu = None;
                            view.menu = None;
                            cx.notify();
                        } else {
                            open_menu(view, index, cx);
                        }
                    }),
                )
                .on_hover(cx.listener(move |view, hovered: &bool, _, cx| {
                    if *hovered && view.menu.is_some() && view.title_menu.is_some_and(|open| open != index) {
                        open_menu(view, index, cx);
                    }
                })),
        );
    }
    row
}

pub fn render(view: &WorkspaceView, window: &Window, cx: &mut Context<WorkspaceView>) -> AnyElement {
    let theme = ui::theme(cx);
    let name = view.state(cx).map(|state| state.name.clone()).unwrap_or_default();
    let height = px((settings::get(cx).ui_font_size() * 2.4).round());
    let client_decorated = matches!(window.window_decorations(), gpui::Decorations::Client { .. });
    let location = location(view, cx);
    let status = status(view, cx);
    let menus = (!cfg!(target_os = "macos")).then(|| menu_bar(view, cx));
    div()
        .id("titlebar")
        .occlude()
        .flex()
        .flex_none()
        .items_center()
        .h(height)
        .pl(px(if cfg!(target_os = "macos") { 78.0 } else { 8.0 }))
        .pr(px(if cfg!(windows) { 0.0 } else { 8.0 }))
        .gap(px(10.0))
        .when(cfg!(windows), |this| this.window_control_area(WindowControlArea::Drag))
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
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .id("workspace-name")
                        .occlude()
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
                )
                .children(menus.map(|menus| div().flex().h_full().pl(px(8.0)).child(menus)))
                .when(!cfg!(target_os = "macos"), |this| this.h_full().items_center().overflow_hidden()),
        )
        .child(location)
        .child(
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .h_full()
                .items_center()
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
                    this.child(
                        control("window-minimize", icons::REMOVE).on_click(|_, window, _| window.minimize_window()),
                    )
                    .child(control("window-zoom", icons::MAXIMIZE).on_click(|_, window, _| window.zoom_window()))
                    .child(control("window-close", icons::CLOSE).on_click(move |_, window, cx| {
                        crate::actions::close_workspace_window(&id, window, cx);
                    }))
                })
                .when(cfg!(windows), |this| {
                    let control = |id: &'static str, glyph: &'static str, area: WindowControlArea| {
                        div()
                            .id(id)
                            .occlude()
                            .window_control_area(area)
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_center()
                            .w(px(CAPTION_BUTTON_WIDTH))
                            .h_full()
                            .text_color(theme.muted)
                            .hover(|style| style.bg(theme.hover).text_color(theme.text))
                            .child(glyph)
                    };
                    let zoom = if window.is_maximized() { icons::CAPTION_RESTORE } else { icons::CAPTION_MAXIMIZE };
                    this.child(control("caption-minimize", icons::CAPTION_MINIMIZE, WindowControlArea::Min))
                        .child(control("caption-zoom", zoom, WindowControlArea::Max))
                        .child(
                            control("caption-close", icons::CAPTION_CLOSE, WindowControlArea::Close)
                                .hover(|style| style.bg(theme.red).text_color(theme.on_accent)),
                        )
                }),
        )
        .into_any_element()
}
