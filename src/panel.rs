use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, AppContext, Context, InteractiveElement, IntoElement, MouseButton, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, Window, div, px,
};

use std::path::{Path, PathBuf};

use crate::icons;
use crate::persist::View;
use crate::store;
use crate::ui;
use crate::window::WorkspaceView;

#[derive(Default)]
pub struct PanelState {
    pub files: Option<gpui::Entity<crate::files::FilesPanel>>,
    pub changes: Option<gpui::Entity<crate::changes::ChangesPanel>>,
    pub commits: Option<gpui::Entity<crate::commits::CommitsPanel>>,
    terminal_tabs: gpui::ScrollHandle,
    revealed_terminal: Option<String>,
}

fn panel_project(view: &WorkspaceView, cx: &App) -> Option<PathBuf> {
    view.current_project_path(cx).filter(|path| !path.as_os_str().is_empty())
}

pub fn render(
    view: &mut WorkspaceView,
    kind: View,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) -> AnyElement {
    let theme = ui::theme(cx);
    match panel_project(view, cx) {
        None => div().p(px(10.0)).text_color(theme.faint).child("no project selected").into_any_element(),
        Some(path) => match kind {
            View::Changes => crate::changes::panel(view, &path, window, cx),
            View::Commits => crate::commits::panel(view, &path, window, cx),
            _ => crate::files::panel(view, &path, window, cx),
        },
    }
}

pub fn terminal_panel(
    view: &mut WorkspaceView,
    _window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) -> Option<AnyElement> {
    let theme = ui::theme(cx);
    let state = view.state(cx)?.clone();
    let project = match &state.selected {
        Some(_) => state.current_project()?.clone(),
        None => state.scratch.clone(),
    };
    if project.terminals.is_empty() {
        return None;
    }
    let shown = project.terminal_shown;
    let active = project.active_terminal.clone().or_else(|| project.terminals.first().cloned())?;
    let session = view.store(cx).session(&active)?.clone();
    let height = state.window.terminal_height;
    if view.panel.revealed_terminal.as_ref() != Some(&active) {
        if let Some(index) = project.terminals.iter().position(|id| *id == active) {
            view.panel.terminal_tabs.scroll_to_item(index);
        }
        view.panel.revealed_terminal = Some(active.clone());
    }
    let mut tabs = div()
        .id("terminal-tabs")
        .flex()
        .flex_shrink(1.)
        .min_w_0()
        .items_center()
        .gap(px(2.0))
        .track_scroll(&view.panel.terminal_tabs)
        .overflow_x_scroll();
    for id in &project.terminals {
        let Some(entity) = view.store(cx).session(id) else { continue };
        let data = entity.read(cx);
        let current = *id == active;
        let selected = shown && current;
        let (glyph, glyph_color) = match (crate::sidebar::tab_glyph(data), data.program()) {
            ((glyph, Some(color)), _) => (glyph, color.color(theme.dark)),
            (_, Some(_)) => (icons::PLAY, theme.green),
            ((glyph, None), None) => (glyph, theme.faint),
        };
        let text_color = if data.attention {
            theme.green
        } else if !data.is_running() {
            theme.faint
        } else if current {
            theme.text
        } else {
            theme.muted
        };
        let tint = data.color.map(|color| color.color(theme.dark));
        let title = data.terminal_title();
        let (select_id, menu_id, close_id) = (id.clone(), id.clone(), id.clone());
        let dragged = crate::sidebar::DraggedTab { id: id.clone(), title: title.clone() };
        let path = project.path.clone();
        let group = SharedString::from(format!("terminal-tab-{id}"));
        tabs = tabs.child(
            div()
                .id(SharedString::from(format!("terminal-tab-{id}")))
                .group(group.clone())
                .flex()
                .flex_none()
                .items_center()
                .gap(px(8.0))
                .px(px(6.0))
                .when(selected, |this| this.bg(theme.accent).text_color(theme.on_accent))
                .when(!selected, |this| {
                    this.text_color(tint.unwrap_or(text_color)).hover(|style| style.bg(theme.hover))
                })
                .child(div().flex_none().when(!selected, |this| this.text_color(glyph_color)).child(glyph))
                .child(title)
                .child(
                    div()
                        .id(SharedString::from(format!("terminal-close-{id}")))
                        .flex_none()
                        .text_color(if selected { theme.on_accent } else { theme.faint })
                        .when(!selected, |this| this.invisible().group_hover(group, |style| style.visible()))
                        .child(icons::CLOSE)
                        .on_click(cx.listener(move |view, _, window, cx| {
                            store::store(cx).update(cx, |store, cx| store.close_tab(&close_id, cx));
                            view.focus_current(window, cx);
                            cx.stop_propagation();
                        })),
                )
                .on_drag(dragged, |drag: &crate::sidebar::DraggedTab, _, _, cx| {
                    cx.new(|_| crate::sidebar::DragPreview(drag.title.clone()))
                })
                .on_click(cx.listener(move |view, _, window, cx| select_terminal(view, &path, &select_id, window, cx)))
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |view, event: &gpui::MouseDownEvent, _, cx| {
                        let items = crate::actions::tab_menu(view, &menu_id, cx);
                        view.open_menu(event.position, items, cx);
                    }),
                ),
        );
    }
    let button = |id: &'static str, glyph: &'static str| {
        div().id(id).px(px(6.0)).text_color(theme.muted).hover(|style| style.text_color(theme.accent)).child(glyph)
    };
    let new_path = project.path.clone();
    let strip = div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(2.0))
        .pl(px(6.0))
        .pr(px(6.0))
        .py(px(3.0))
        .child(tabs)
        .child(button("terminal-new", icons::ADD).flex_none().on_click(
            cx.listener(move |view, _, window, cx| crate::actions::new_terminal_in(view, &new_path, window, cx)),
        ))
        .child(div().flex_1())
        .child(
            button("terminal-toggle", if shown { icons::CHEVRON_DOWN } else { icons::CHEVRON_UP })
                .flex_none()
                .on_click(cx.listener(move |view, _, window, cx| toggle_terminal_panel(view, !shown, window, cx))),
        );
    let panel = div().flex().flex_col().flex_none().bg(theme.background).border_t_1().border_color(theme.border);
    let panel = if shown {
        panel
            .h(px(height))
            .child(crate::resize::handle_vertical(crate::resize::Drag::Terminal, cx))
            .child(strip)
            .child(div().flex_1().min_h_0().child(session))
    } else {
        panel.child(strip)
    };
    Some(panel.into_any_element())
}

fn toggle_terminal_panel(view: &mut WorkspaceView, shown: bool, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    set_shown(view, shown, cx);
    if !shown {
        view.focus_current(window, cx);
        return;
    }
    focus_active_terminal(view, window, cx);
    cx.notify();
}

fn select_terminal(
    view: &mut WorkspaceView,
    path: &Path,
    id: &str,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) {
    let workspace = view.workspace.clone();
    store::store(cx).update(cx, |store, cx| {
        if let Some(project) = store.workspace_mut(&workspace).and_then(|state| state.project_mut(path)) {
            project.active_terminal = Some(id.to_string());
            project.terminal_shown = true;
        }
        store.changed(cx);
    });
    if let Some(session) = view.store(cx).session(id) {
        let focus = session.read(cx).focus.clone();
        window.focus(&focus, cx);
    }
}

fn focus_active_terminal(view: &WorkspaceView, window: &mut Window, cx: &mut App) -> bool {
    let path = panel_target(view, cx);
    let active = view
        .state(cx)
        .and_then(|state| state.project(&path))
        .and_then(|project| project.active_terminal.clone().or_else(|| project.terminals.first().cloned()));
    let Some(session) = active.and_then(|id| view.store(cx).session(&id).cloned()) else { return false };
    let focus = session.read(cx).focus.clone();
    window.focus(&focus, cx);
    true
}

fn panel_terminals(view: &WorkspaceView, cx: &App) -> Vec<String> {
    let path = panel_target(view, cx);
    view.state(cx).and_then(|state| state.project(&path)).map(|project| project.terminals.clone()).unwrap_or_default()
}

pub fn close_focused(view: &mut WorkspaceView, window: &mut Window, cx: &mut Context<WorkspaceView>) -> bool {
    let Some(id) = focused_terminal(view, window, cx) else { return false };
    store::store(cx).update(cx, |store, cx| store.close_tab(&id, cx));
    if panel_terminals(view, cx).is_empty() || !focus_active_terminal(view, window, cx) {
        view.focus_current(window, cx);
    }
    true
}

pub fn cycle_focused(
    view: &mut WorkspaceView,
    delta: i32,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) -> bool {
    let Some(current) = focused_terminal(view, window, cx) else { return false };
    let terminals = panel_terminals(view, cx);
    let index = terminals.iter().position(|id| *id == current).unwrap_or(0) as i32;
    let next = (index + delta).rem_euclid(terminals.len() as i32) as usize;
    select_terminal(view, &panel_target(view, cx), &terminals[next], window, cx);
    true
}

pub fn select_focused_number(
    view: &mut WorkspaceView,
    number: usize,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) -> bool {
    if focused_terminal(view, window, cx).is_none() {
        return false;
    }
    if let Some(id) = panel_terminals(view, cx).get(number - 1) {
        select_terminal(view, &panel_target(view, cx), id, window, cx);
    }
    true
}

fn panel_target(view: &WorkspaceView, cx: &App) -> PathBuf {
    view.current_project_path(cx).unwrap_or_default()
}

fn set_shown(view: &WorkspaceView, shown: bool, cx: &mut App) {
    let workspace = view.workspace.clone();
    let path = panel_target(view, cx);
    store::store(cx).update(cx, |store, cx| {
        if let Some(project) = store.workspace_mut(&workspace).and_then(|state| state.project_mut(&path)) {
            project.terminal_shown = shown;
        }
        store.changed(cx);
    });
}

pub fn focused_terminal(view: &WorkspaceView, window: &Window, cx: &App) -> Option<String> {
    let state = view.state(cx)?;
    let project = match &state.selected {
        Some(_) => state.current_project()?,
        None => &state.scratch,
    };
    if !project.terminal_shown {
        return None;
    }
    let id = project.active_terminal.clone().or_else(|| project.terminals.first().cloned())?;
    let session = view.store(cx).session(&id)?;
    session.read(cx).focus.contains_focused(window, cx).then_some(id)
}

pub fn toggle_terminal(view: &mut WorkspaceView, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let path = panel_target(view, cx);
    let project = view.state(cx).and_then(|state| state.project(&path)).cloned();
    let Some(project) = project else { return };
    if focused_terminal(view, window, cx).is_some() {
        set_shown(view, false, cx);
        view.focus_current(window, cx);
        return;
    }
    if project.terminals.is_empty() {
        crate::actions::new_terminal_in(view, &path, window, cx);
        return;
    }
    set_shown(view, true, cx);
    focus_active_terminal(view, window, cx);
    cx.notify();
}
