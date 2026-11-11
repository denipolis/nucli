use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, Context, Corner, Div, Entity, InteractiveElement, IntoElement, MouseButton, ParentElement,
    StatefulInteractiveElement, Styled, Window, WindowControlArea, anchored, deferred, div, point, px,
};

use crate::commands;
use crate::icons;
use crate::session::{Activity, Kind, Session};
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
        project.and_then(|project| project.active_file().map(String::as_str)).map(|file| {
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

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Status {
    Waiting,
    Done,
    Working,
}

fn agents(view: &WorkspaceView, cx: &App) -> Vec<(Status, Entity<Session>, String)> {
    let store = view.store(cx);
    let mut agents = Vec::new();
    for project in view.state(cx).into_iter().flat_map(|state| state.all_projects()) {
        let name = if project.is_scratch() { String::new() } else { project.display_name() };
        for session in project.tabs.iter().chain(&project.terminals).filter_map(|id| store.session(id)) {
            let data = session.read(cx);
            let agent = data.kind == Kind::Agent || data.running_agent.is_some();
            let status = match data.activity {
                Activity::Waiting => Status::Waiting,
                Activity::Working => Status::Working,
                Activity::Idle if data.attention && agent => Status::Done,
                Activity::Idle => continue,
            };
            agents.push((status, session.clone(), name.clone()));
        }
    }
    agents.sort_by_key(|(status, ..)| *status);
    agents
}

fn messages(view: &WorkspaceView, cx: &App) -> Vec<String> {
    let store = view.store(cx);
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
    messages
}

fn dropdown(
    view: &WorkspaceView,
    messages: &[String],
    agents: &[(Status, Entity<Session>, String)],
    cx: &mut Context<WorkspaceView>,
) -> AnyElement {
    let theme = ui::theme(cx);
    let tick = view.store(cx).tick_fast;
    let spinner = icons::SPINNER[tick % icons::SPINNER.len()];
    let mut list = div()
        .id("status-list")
        .occlude()
        .w(px(420.0))
        .max_h(px(480.0))
        .overflow_y_scroll()
        .bg(theme.elevated)
        .border_1()
        .border_color(theme.border)
        .on_mouse_down_out(cx.listener(|view, _, _, cx| {
            view.status_open = false;
            cx.notify();
        }));
    for message in messages {
        list = list.child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .px(px(10.0))
                .py(px(4.0))
                .border_b_1()
                .border_color(theme.border)
                .child(div().flex_none().w(px(14.0)).text_color(theme.accent).child(spinner))
                .child(div().min_w_0().truncate().text_color(theme.muted).child(message.clone())),
        );
    }
    for (index, (status, session, project)) in agents.iter().enumerate() {
        let data = session.read(cx);
        let (glyph, color) =
            crate::sidebar::status_glyph(data, &theme, tick).unwrap_or((icons::CHECK.into(), theme.green));
        let (agent, agent_color) = crate::sidebar::tab_glyph(data);
        let agent_color = agent_color.map(|color| color.color(theme.dark)).unwrap_or(theme.muted);
        let notice = data.notice.as_ref().filter(|_| data.attention);
        let text = match (status, notice) {
            (Status::Working, _) if data.subagents.is_empty() => "working".to_string(),
            (Status::Working, _) => format!("working, {} subagents", data.subagents.len()),
            (_, Some((text, _))) => text.clone(),
            (Status::Waiting, None) => "waiting for input".to_string(),
            (Status::Done, None) => "finished".to_string(),
        };
        let age = notice.map(|(_, time)| crate::history::relative_age(*time));
        let id = data.id.clone();
        list = list.child(
            div()
                .id(("status-agent", index))
                .flex()
                .flex_col()
                .px(px(10.0))
                .py(px(4.0))
                .border_b_1()
                .border_color(theme.border)
                .hover(|style| style.bg(theme.hover))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(div().flex_none().w(px(14.0)).text_color(color).child(glyph))
                        .child(div().flex_none().text_color(agent_color).child(agent))
                        .child(div().flex_1().min_w_0().truncate().child(data.display_title()))
                        .children(age.map(|age| div().flex_none().text_color(theme.faint).child(age))),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(6.0))
                        .pl(px(20.0))
                        .when(!project.is_empty(), |this| {
                            this.child(div().flex_none().text_color(theme.faint).child(project.clone()))
                        })
                        .child(div().min_w_0().truncate().text_color(theme.muted).child(text)),
                )
                .on_click(cx.listener(move |view, _, window, cx| {
                    view.status_open = false;
                    view.reveal_tab(&id, window, cx);
                    cx.notify();
                })),
        );
    }
    if messages.is_empty() && agents.is_empty() {
        list = list.child(div().px(px(10.0)).py(px(4.0)).text_color(theme.faint).child("no activity"));
    }
    deferred(
        anchored()
            .anchor(Corner::TopRight)
            .offset(point(px(0.0), px(1.0)))
            .snap_to_window_with_margin(px(8.0))
            .child(list),
    )
    .with_priority(2)
    .into_any_element()
}

fn status(view: &WorkspaceView, cx: &mut Context<WorkspaceView>) -> Div {
    let theme = ui::theme(cx);
    let line = div().flex().flex_1().h_full().justify_end().min_w_0();
    let messages = messages(view, cx);
    let agents = agents(view, cx);
    if messages.is_empty() && agents.is_empty() && !view.status_open {
        return line;
    }
    let spinner = icons::SPINNER[view.store(cx).tick_fast % icons::SPINNER.len()];
    let mut chip = div()
        .id("status")
        .occlude()
        .relative()
        .flex()
        .items_center()
        .gap(px(12.0))
        .min_w_0()
        .h_full()
        .px(px(8.0))
        .when(view.status_open, |this| this.bg(theme.hover))
        .hover(|style| style.bg(theme.hover))
        .capture_any_mouse_down(cx.listener(|view, _, _, cx| {
            view.status_open = !view.status_open;
            cx.stop_propagation();
            cx.notify();
        }));
    if let Some(message) = messages.last() {
        let others = messages.len() - 1;
        chip = chip
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
            .when(others > 0, |this| this.child(div().flex_none().text_color(theme.faint).child(format!("+{others}"))));
    } else {
        let count = |status: Status, glyph: &'static str, color, label: &str| {
            let count = agents.iter().filter(|(item, ..)| *item == status).count();
            (count > 0).then(|| {
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(6.0))
                    .text_color(theme.muted)
                    .child(div().text_color(color).child(glyph))
                    .child(format!("{count} {label}"))
            })
        };
        chip = chip
            .children(count(Status::Working, spinner, theme.accent, "working"))
            .children(count(Status::Waiting, icons::QUESTION, theme.orange, "waiting"))
            .children(count(Status::Done, icons::CHECK, theme.green, "done"));
    }
    if view.status_open {
        chip = chip.child(div().absolute().right_0().bottom_0().child(dropdown(view, &messages, &agents, cx)));
    }
    line.child(chip)
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
