use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, Context, InteractiveElement, IntoElement, MouseButton, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::icons;
use crate::persist::PanelTab;
use crate::store;
use crate::ui;
use crate::window::WorkspaceView;

#[derive(Default)]
pub struct PanelState {
    pub files: Option<gpui::Entity<crate::files::FilesPanel>>,
    pub changes: Option<gpui::Entity<crate::changes::ChangesPanel>>,
    pub commits: Option<gpui::Entity<crate::commits::CommitsPanel>>,
}

fn panel_project(view: &WorkspaceView, cx: &App) -> Option<std::path::PathBuf> {
    view.current_project_path(cx).filter(|path| !path.as_os_str().is_empty())
}

pub fn render(view: &mut WorkspaceView, window: &mut Window, cx: &mut Context<WorkspaceView>) -> AnyElement {
    let theme = ui::theme(cx);
    let tab = view.store(cx).ui.panel_tab;
    let width = view.state(cx).map(|state| state.window.panel_width).unwrap_or(280.0);
    let side = view.store(cx).ui.panel_side;
    let header_tab = |id: &'static str, glyph: &'static str, label: &'static str, target: PanelTab| {
        let active = tab == target;
        div()
            .id(id)
            .flex()
            .gap(px(8.0))
            .px(px(4.0))
            .when(active, |this| this.bg(theme.accent).text_color(theme.on_accent))
            .when(!active, |this| this.text_color(theme.muted).hover(|style| style.text_color(theme.text)))
            .child(glyph)
            .child(label)
            .on_click(move |_, _, cx| {
                store::store(cx).update(cx, |store, cx| {
                    store.ui.panel_tab = target;
                    store.changed(cx);
                })
            })
    };
    let project = panel_project(view, cx);
    let content: AnyElement = match project {
        None => div().p(px(10.0)).text_color(theme.faint).child("no project").into_any_element(),
        Some(path) => match tab {
            PanelTab::Files => crate::files::panel(view, &path, window, cx),
            PanelTab::Changes => crate::changes::panel(view, &path, window, cx),
            PanelTab::Commits => crate::commits::panel(view, &path, window, cx),
        },
    };
    div()
        .flex()
        .flex_col()
        .flex_none()
        .w(px(width))
        .h_full()
        .bg(theme.surface)
        .when(side == crate::persist::Side::Left, |this| this.border_r_1())
        .when(side == crate::persist::Side::Right, |this| this.border_l_1())
        .border_color(theme.border)
        .child(
            div()
                .id("panel-header")
                .on_mouse_down(MouseButton::Left, cx.listener(crate::resize::start(crate::resize::Drag::PanelSide)))
                .flex()
                .flex_none()
                .items_center()
                .gap(px(6.0))
                .px(px(8.0))
                .py(px(4.0))
                .overflow_hidden()
                .border_b_1()
                .border_color(theme.border)
                .child(header_tab("panel-files", icons::FILES, "FILES", PanelTab::Files))
                .child(header_tab("panel-changes", icons::CHANGES, "CHANGES", PanelTab::Changes))
                .child(header_tab("panel-commits", icons::COMMIT, "HISTORY", PanelTab::Commits))
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|view, event: &gpui::MouseDownEvent, _, cx| {
                        let items = vec![
                            crate::menu::command("Move Panel to Other Side", "move_panel", cx),
                            crate::menu::command("Hide Panel", "toggle_panel", cx),
                        ];
                        view.open_menu(event.position, items, cx);
                    }),
                ),
        )
        .child(div().flex_1().min_h_0().child(content))
        .into_any_element()
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
    if !project.terminal_shown || project.terminals.is_empty() {
        return None;
    }
    let active = project.active_terminal.clone().or_else(|| project.terminals.first().cloned())?;
    let session = view.store(cx).session(&active)?.clone();
    let height = state.window.terminal_height;
    let maximized = state.window.terminal_maximized;
    let mut strip = div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(4.0))
        .px(px(6.0))
        .py(px(2.0))
        .border_b_1()
        .border_color(theme.border)
        .child(div().px(px(6.0)).mr(px(6.0)).bg(theme.muted).text_color(theme.on_accent).child("TERMINAL"));
    for (index, id) in project.terminals.iter().enumerate() {
        let Some(entity) = view.store(cx).session(id) else { continue };
        let data = entity.read(cx);
        let selected = *id == active;
        let tint = data.color.map(|color| color.color(theme.dark));
        let title = format!("{} {}", index + 1, data.display_title());
        let (select_id, menu_id, close_id) = (id.clone(), id.clone(), id.clone());
        let path = project.path.clone();
        strip = strip.child(
            div()
                .id(SharedString::from(format!("terminal-tab-{id}")))
                .flex()
                .gap(px(6.0))
                .px(px(6.0))
                .when(selected, |this| this.bg(theme.accent).text_color(theme.on_accent))
                .when(!selected, |this| {
                    this.text_color(tint.unwrap_or(theme.muted)).hover(|style| style.bg(theme.hover))
                })
                .when(data.attention, |this| this.text_color(theme.green))
                .child(title)
                .child(
                    div()
                        .id(SharedString::from(format!("terminal-close-{id}")))
                        .text_color(if selected { theme.on_accent } else { theme.faint })
                        .child(icons::CLOSE)
                        .on_click(cx.listener(move |view, _, window, cx| {
                            store::store(cx).update(cx, |store, cx| store.close_tab(&close_id, cx));
                            view.focus_current(window, cx);
                            cx.stop_propagation();
                        })),
                )
                .on_click(cx.listener(move |view, _, window, cx| {
                    let workspace = view.workspace.clone();
                    let (path, id) = (path.clone(), select_id.clone());
                    store::store(cx).update(cx, |store, cx| {
                        if let Some(project) =
                            store.workspace_mut(&workspace).and_then(|state| state.project_mut(&path))
                        {
                            project.active_terminal = Some(id.clone());
                        }
                        store.changed(cx);
                    });
                    if let Some(session) = view.store(cx).session(&select_id) {
                        let focus = session.read(cx).focus.clone();
                        window.focus(&focus);
                    }
                }))
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
    strip = strip
        .child(button("terminal-new", icons::ADD).on_click(
            cx.listener(move |view, _, window, cx| crate::actions::new_terminal_in(view, &new_path, window, cx)),
        ))
        .child(div().flex_1())
        .child(button("terminal-maximize", icons::MAXIMIZE).on_click(cx.listener(|view, _, _, cx| {
            let workspace = view.workspace.clone();
            store::store(cx).update(cx, |store, cx| {
                if let Some(state) = store.workspace_mut(&workspace) {
                    state.window.terminal_maximized = !state.window.terminal_maximized;
                }
                store.changed(cx);
            });
        })))
        .child(button("terminal-hide", icons::CHEVRON_DOWN).on_click(cx.listener(|view, _, window, cx| {
            set_shown(view, false, cx);
            view.focus_current(window, cx);
        })));
    let panel = div()
        .flex()
        .flex_col()
        .flex_none()
        .border_t_1()
        .border_color(theme.border)
        .when(maximized, |this| this.flex_1().min_h_0())
        .when(!maximized, |this| this.h(px(height)))
        .child(crate::resize::handle_vertical(view, cx))
        .child(strip)
        .child(div().flex_1().min_h_0().child(session));
    Some(panel.into_any_element())
}

fn panel_target(view: &WorkspaceView, cx: &App) -> std::path::PathBuf {
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
    let id = project.active_terminal.clone()?;
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
    let id = project.active_terminal.or_else(|| project.terminals.first().cloned());
    if let Some(session) = id.and_then(|id| view.store(cx).session(&id).cloned()) {
        let focus = session.read(cx).focus.clone();
        window.focus(&focus);
    }
    cx.notify();
}
