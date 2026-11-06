use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ModifiersChangedEvent, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, Window, deferred, div, px,
};

use crate::icons;
use crate::persist::Selected;
use crate::ui;
use crate::window::WorkspaceView;

pub struct Switcher {
    pub entries: Vec<Selected>,
    pub highlighted: usize,
}

pub fn open(view: &mut WorkspaceView, delta: i32, _window: &mut Window, cx: &mut Context<WorkspaceView>) {
    if let Some(switcher) = view.switcher.as_mut() {
        let count = switcher.entries.len() as i32;
        if count > 0 {
            switcher.highlighted = (switcher.highlighted as i32 + delta).rem_euclid(count) as usize;
        }
        cx.notify();
        return;
    }
    let Some(state) = view.state(cx) else { return };
    let tabs = state.ordered_tabs();
    let mut entries: Vec<Selected> =
        state.mru.iter().filter(|id| tabs.contains(id)).map(|id| Selected::Tab(id.clone())).collect();
    for id in tabs {
        if !entries.contains(&Selected::Tab(id.clone())) {
            entries.push(Selected::Tab(id));
        }
    }
    for project in
        state.projects.iter().filter(|project| project.active_file.is_some() && !project.open_files.is_empty())
    {
        entries.push(Selected::Project(project.path.clone()));
    }
    if entries.is_empty() {
        return;
    }
    let current = state.selected.clone();
    let start = entries.iter().position(|entry| Some(entry) == current.as_ref()).unwrap_or(0) as i32;
    let highlighted = (start + delta).rem_euclid(entries.len() as i32) as usize;
    view.switcher = Some(Switcher { entries, highlighted });
    cx.notify();
}

pub fn commit(view: &mut WorkspaceView, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let Some(switcher) = view.switcher.take() else { return };
    match switcher.entries.get(switcher.highlighted).cloned() {
        Some(Selected::Project(path)) => {
            view.select(Some(Selected::Project(path.clone())), window, cx);
            let workspace = view.workspace.clone();
            crate::store::store(cx).update(cx, |store, cx| {
                if let Some(project) = store.workspace_mut(&workspace).and_then(|state| state.project_mut(&path)) {
                    project.file_shown = true;
                }
                store.changed(cx);
            });
            view.focus_current(window, cx);
        }
        Some(selected) => view.select(Some(selected), window, cx),
        None => {}
    }
    cx.notify();
}

pub fn modifiers_changed(
    view: &mut WorkspaceView,
    event: &ModifiersChangedEvent,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) {
    if view.switcher.is_some() && !event.modifiers.control && !event.modifiers.platform {
        commit(view, window, cx);
    }
}

pub fn render(view: &WorkspaceView, switcher: &Switcher, cx: &mut Context<WorkspaceView>) -> AnyElement {
    let theme = ui::theme(cx);
    let store = view.store(cx);
    let state = view.state(cx);
    let mut list = div().w(px(520.0)).bg(theme.elevated).border_1().border_color(theme.accent).py(px(4.0));
    list = list
        .child(div().px(px(10.0)).pb(px(4.0)).text_color(theme.faint).child("SWITCH  [release] open  [esc] cancel"));
    for (index, entry) in switcher.entries.iter().enumerate() {
        let active = index == switcher.highlighted;
        let (glyph, color, title, subtitle, attention) = match entry {
            Selected::Tab(id) => {
                let Some(session) = store.session(id) else { continue };
                let session = session.read(cx);
                let project = state
                    .and_then(|state| state.project_of(id))
                    .map(|project| project.display_name())
                    .unwrap_or_default();
                let (glyph, color) = crate::sidebar::tab_glyph(session);
                (glyph, color, session.display_title(), project, session.attention)
            }
            Selected::Project(path) => {
                let project = state.and_then(|state| state.project(path));
                let file = project.and_then(|project| project.active_file.clone()).unwrap_or_default();
                let name = project.map(|project| project.display_name()).unwrap_or_default();
                (icons::FILE, None, file, name, false)
            }
        };
        let row_index = index;
        list = list.child(
            div()
                .id(("switcher", index))
                .flex()
                .gap(px(8.0))
                .px(px(10.0))
                .when(active, |this| this.bg(theme.accent).text_color(theme.on_accent))
                .child(
                    div()
                        .w(px(16.0))
                        .when_some(color.filter(|_| !active), |this, color| this.text_color(color.color(theme.dark)))
                        .child(glyph),
                )
                .child(div().flex_1().truncate().child(SharedString::from(title)))
                .child(div().text_color(if active { theme.on_accent } else { theme.faint }).child(subtitle))
                .when(attention, |this| this.child(div().text_color(theme.green).child(icons::DOT)))
                .on_click(cx.listener(move |view, _, window, cx| {
                    if let Some(switcher) = view.switcher.as_mut() {
                        switcher.highlighted = row_index;
                    }
                    commit(view, window, cx);
                })),
        );
    }
    deferred(
        div()
            .id("switcher-backdrop")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .justify_center()
            .items_center()
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|view, _, _, cx| {
                    view.switcher = None;
                    cx.notify();
                }),
            )
            .child(list),
    )
    .with_priority(3)
    .into_any_element()
}

use gpui::prelude::FluentBuilder;
