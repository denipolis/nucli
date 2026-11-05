use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement, SharedString, Styled, Window, div, px,
};

use crate::persist::Layout;
use crate::sidebar::DraggedTab;
use crate::splits::{self, Direction};
use crate::store::{self, Launcher};
use crate::ui;
use crate::window::WorkspaceView;

fn pane(view: &WorkspaceView, id: &str, focused: &str, cx: &mut Context<WorkspaceView>) -> AnyElement {
    let theme = ui::theme(cx);
    let Some(session) = view.store(cx).session(id).cloned() else { return div().into_any_element() };
    let target = id.to_string();
    let drop_target = id.to_string();
    div()
        .id(SharedString::from(format!("pane-{id}")))
        .flex_1()
        .min_w_0()
        .min_h_0()
        .relative()
        .when_else(id == focused, |this| this.opacity(1.0), |this| this.opacity(0.55))
        .on_mouse_down(
            gpui::MouseButton::Left,
            cx.listener(move |view, _, window, cx| {
                if view.state(cx).and_then(|state| state.selected_tab()) != Some(target.as_str()) {
                    view.select_tab(&target, window, cx);
                }
            }),
        )
        .drag_over::<DraggedTab>(move |style, _, _, _| style.border_2().border_color(theme.accent))
        .on_drop(cx.listener(move |view, drag: &DraggedTab, window, cx| {
            if drag.id != drop_target {
                place(view, &drop_target, &drag.id, true, window, cx);
            }
        }))
        .child(session)
        .into_any_element()
}

fn tree(view: &WorkspaceView, layout: &Layout, focused: &str, cx: &mut Context<WorkspaceView>) -> AnyElement {
    let theme = ui::theme(cx);
    match layout {
        Layout::Pane { tab } => pane(view, tab, focused, cx),
        Layout::Row { children } | Layout::Column { children } => {
            let row = matches!(layout, Layout::Row { .. });
            let mut container = div().flex().flex_1().min_w_0().min_h_0().size_full();
            container = if row { container.flex_row() } else { container.flex_col() };
            for (index, child) in children.iter().enumerate() {
                if index > 0 {
                    container = container.child(if row {
                        div().w(px(1.0)).h_full().bg(theme.border)
                    } else {
                        div().h(px(1.0)).w_full().bg(theme.border)
                    });
                }
                container = container.child(tree(view, child, focused, cx));
            }
            container.into_any_element()
        }
    }
}

pub fn render(view: &WorkspaceView, layout: &Layout, focused: &str, cx: &mut Context<WorkspaceView>) -> AnyElement {
    div().size_full().flex().child(tree(view, layout, focused, cx)).into_any_element()
}

fn place(
    view: &mut WorkspaceView,
    target: &str,
    new: &str,
    row: bool,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) {
    let workspace = view.workspace.clone();
    let (target, new) = (target.to_string(), new.to_string());
    store::store(cx).update(cx, |store, cx| {
        if let Some(state) = store.workspace_mut(&workspace) {
            splits::split(&mut state.layouts, &target, &new, row);
        }
        store.changed(cx);
    });
    view.select_tab(&new, window, cx);
}

pub fn split(view: &mut WorkspaceView, row: bool, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let Some(current) = view.state(cx).and_then(|state| state.selected_tab()).map(String::from) else { return };
    let Some(project) = view.state(cx).and_then(|state| state.project_of(&current)).map(|project| project.path.clone())
    else {
        return;
    };
    let launcher = crate::actions::last_launcher(view, &project, cx).unwrap_or(Launcher::Terminal);
    let launcher = if matches!(launcher, Launcher::Terminal) {
        match settings_shell(cx) {
            Some(shell) => Launcher::Shell(shell),
            None => return,
        }
    } else {
        launcher
    };
    let workspace = view.workspace.clone();
    let after = Some(current.clone());
    let id = store::store(cx).update(cx, |store, cx| store.open_tab(&workspace, &project, launcher, after, cx));
    if let Some(id) = id {
        place(view, &current, &id, row, window, cx);
    }
}

fn settings_shell(cx: &gpui::App) -> Option<crate::agents::ShellProfile> {
    crate::settings::get(cx).shells.iter().find(|shell| shell.is_launchable()).cloned()
}

pub fn detach(view: &mut WorkspaceView, id: &str, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let workspace = view.workspace.clone();
    let id = id.to_string();
    store::store(cx).update(cx, |store, cx| {
        if let Some(state) = store.workspace_mut(&workspace) {
            splits::remove(&mut state.layouts, &id);
        }
        store.changed(cx);
    });
    view.focus_current(window, cx);
}

pub fn unsplit(view: &mut WorkspaceView, id: &str, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let workspace = view.workspace.clone();
    let id = id.to_string();
    store::store(cx).update(cx, |store, cx| {
        if let Some(state) = store.workspace_mut(&workspace) {
            splits::unsplit(&mut state.layouts, &id);
        }
        store.changed(cx);
    });
    view.focus_current(window, cx);
}

pub fn focus(view: &mut WorkspaceView, direction: Direction, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let Some(state) = view.state(cx) else { return };
    let Some(current) = state.selected_tab() else { return };
    let Some(index) = splits::group(&state.layouts, current) else { return };
    if let Some(next) = splits::neighbor(&state.layouts[index], current, direction) {
        view.select_tab(&next, window, cx);
    }
}

use gpui::prelude::FluentBuilder;
