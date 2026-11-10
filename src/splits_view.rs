use std::path::PathBuf;

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, AppContext, Bounds, Context, CursorStyle, DragMoveEvent, Hsla, InteractiveElement, IntoElement,
    ParentElement, Pixels, Point, SharedString, StatefulInteractiveElement, Styled, Window, canvas, div, px, relative,
};

use crate::persist::Layout;
use crate::sidebar::{DragPreview, DraggedTab, TabInfo};
use crate::splits::{self, Direction, Rect, Zone};
use crate::store::{self, FilePlace, Launcher};
use crate::ui;
use crate::window::WorkspaceView;

#[derive(Clone, PartialEq, Debug)]
pub enum DropTarget {
    Pane { target: String, dragged: String, zone: Zone },
    Files { group: String, file: String, place: FilePlace },
}

pub enum Fit {
    Same,
    Adopt(PathBuf),
    Blocked,
}

pub fn fit(view: &WorkspaceView, target: &str, dragged: &str, cx: &App) -> Fit {
    let Some(state) = view.state(cx) else { return Fit::Blocked };
    let (Some(home), Some(source)) = (state.project_of(target), state.project_of(dragged)) else {
        return Fit::Blocked;
    };
    let panel = |project: &store::Project, id: &str| project.terminals.iter().any(|tab| tab == id);
    if target == dragged || panel(home, target) || panel(source, dragged) {
        Fit::Blocked
    } else if home.path == source.path {
        Fit::Same
    } else if source.is_scratch() {
        Fit::Adopt(home.path.clone())
    } else {
        Fit::Blocked
    }
}

pub fn arrange(
    view: &mut WorkspaceView,
    target: &str,
    dragged: &str,
    zone: Zone,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) {
    view.drop_target = None;
    match fit(view, target, dragged, cx) {
        Fit::Blocked => return,
        Fit::Adopt(path) => crate::actions::move_tab(view, dragged, &path, None, window, cx),
        Fit::Same => {}
    }
    let workspace = view.workspace.clone();
    let (target, dragged) = (target.to_string(), dragged.to_string());
    store::store(cx).update(cx, |store, cx| {
        if let Some(state) = store.workspace_mut(&workspace) {
            splits::arrange(&mut state.layouts, &target, &dragged, zone);
        }
        store.changed(cx);
    });
    view.select_tab(&dragged, window, cx);
}

pub fn toggle(view: &mut WorkspaceView, id: &str, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let Some(state) = view.state(cx) else { return };
    let Some(current) = state.selected_tab().map(String::from) else { return };
    let shown = splits::group(&state.layouts, &current).map(|index| splits::tabs(&state.layouts[index]));
    if shown.is_some_and(|tabs| tabs.iter().any(|tab| tab == id)) {
        detach(view, id, window, cx);
        return;
    }
    let wide = view.pane_bounds.borrow().get(&current).is_none_or(|bounds| bounds.size.width >= bounds.size.height);
    let zone = if wide { Direction::Right } else { Direction::Down };
    arrange(view, &current, id, Zone::Edge(zone), window, cx);
}

pub fn can_join(view: &WorkspaceView, id: &str, cx: &App) -> bool {
    let Some(state) = view.state(cx) else { return false };
    let Some(current) = state.selected_tab() else { return false };
    view.settings.is_none()
        && !state.file_shown()
        && splits::group(&state.layouts, current)
            .is_none_or(|index| !splits::tabs(&state.layouts[index]).iter().any(|tab| tab == id))
        && !matches!(fit(view, current, id, cx), Fit::Blocked)
}

fn header(view: &WorkspaceView, id: &str, focused: bool, cx: &mut Context<WorkspaceView>) -> AnyElement {
    let theme = ui::theme(cx);
    let Some(session) = view.store(cx).session(id) else { return div().into_any_element() };
    let info = TabInfo::new(session.read(cx), &theme, view.store(cx).tick_fast);
    let (text, faint) = if focused { (theme.on_accent, theme.on_accent) } else { (theme.muted, theme.faint) };
    let glyph = if focused {
        theme.on_accent
    } else {
        info.color.or(info.glyph.1).map(|color| color.color(theme.dark)).unwrap_or(theme.muted)
    };
    let detach_id = id.to_string();
    let dragged = DraggedTab { id: id.to_string(), title: info.title.clone() };
    div()
        .id(SharedString::from(format!("pane-header-{id}")))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(6.0))
        .pl(px(8.0))
        .pr(px(4.0))
        .border_b_1()
        .border_color(theme.border)
        .cursor(CursorStyle::OpenHand)
        .when(focused, |this| this.bg(theme.accent))
        .when(!focused, |this| this.bg(theme.surface).hover(|style| style.bg(theme.hover)))
        .text_color(text)
        .child(div().flex_none().text_color(glyph).child(info.glyph.0))
        .child(div().flex_1().min_w_0().truncate().child(info.title.clone()))
        .children(
            info.status
                .map(|(status, color)| div().flex_none().text_color(if focused { text } else { color }).child(status)),
        )
        .child(
            div()
                .id(SharedString::from(format!("pane-detach-{id}")))
                .flex_none()
                .px(px(4.0))
                .cursor(CursorStyle::PointingHand)
                .text_color(faint)
                .hover(|style| style.text_color(if focused { theme.on_accent } else { theme.text }).bg(theme.hover))
                .child(crate::icons::CLOSE)
                .tooltip(ui::tooltip("Remove from split, keeps running"))
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |view, _, window, cx| {
                    detach(view, &detach_id, window, cx);
                    cx.stop_propagation();
                })),
        )
        .on_drag(dragged, |drag: &DraggedTab, _, _, cx| cx.new(|_| DragPreview(drag.title.clone())))
        .into_any_element()
}

fn pane(view: &WorkspaceView, id: &str, focused: &str, split: bool, cx: &mut Context<WorkspaceView>) -> AnyElement {
    let Some(session) = view.store(cx).session(id).cloned() else { return div().into_any_element() };
    let target = id.to_string();
    let (drop_target, move_target) = (id.to_string(), id.to_string());
    let (bounds_map, bounds_id) = (view.pane_bounds.clone(), id.to_string());
    let chips = crate::statusbar::every_pane(cx).then(|| crate::statusbar::session_row(view, &session, cx));
    div()
        .id(SharedString::from(format!("pane-{id}")))
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .on_mouse_down(
            gpui::MouseButton::Left,
            cx.listener(move |view, _, window, cx| {
                if view.state(cx).and_then(|state| state.selected_tab()) != Some(target.as_str()) {
                    view.select_tab(&target, window, cx);
                }
            }),
        )
        .on_drag_move::<DraggedTab>(cx.listener(move |view, event: &DragMoveEvent<DraggedTab>, _, cx| {
            let inside = event.bounds.contains(&event.event.position);
            let mine = matches!(&view.drop_target, Some(DropTarget::Pane { target, .. }) if *target == move_target);
            if !inside && !mine {
                return;
            }
            let dragged = event.drag(cx).id.clone();
            let zone = zone_at(event.bounds, view.drop_area.get(), event.event.position);
            let next = (inside && dragged != move_target).then(|| DropTarget::Pane {
                target: move_target.clone(),
                dragged,
                zone,
            });
            if view.drop_target != next {
                view.drop_target = next;
                cx.notify();
            }
        }))
        .on_drop(cx.listener(move |view, drag: &DraggedTab, window, cx| {
            let zone = match &view.drop_target {
                Some(DropTarget::Pane { target, zone, .. }) if *target == drop_target => *zone,
                _ => Zone::Center,
            };
            arrange(view, &drop_target, &drag.id, zone, window, cx);
        }))
        .when(split, |this| this.child(header(view, id, id == focused, cx)))
        .child(
            div()
                .relative()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .when(id != focused, |this| this.opacity(0.55))
                .child(div().flex_1().min_h_0().child(session))
                .children(chips)
                .child(
                    canvas(
                        move |bounds, _, _| {
                            bounds_map.borrow_mut().insert(bounds_id, bounds);
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                ),
        )
        .into_any_element()
}

pub fn layout_tree(layout: &Layout, border: Hsla, leaf: &mut dyn FnMut(&str) -> AnyElement) -> AnyElement {
    match layout {
        Layout::Pane { tab } => leaf(tab),
        Layout::Row { children } | Layout::Column { children } => {
            let row = matches!(layout, Layout::Row { .. });
            let mut container = div().flex().flex_1().min_w_0().min_h_0().size_full();
            container = if row { container.flex_row() } else { container.flex_col() };
            for (index, child) in children.iter().enumerate() {
                if index > 0 {
                    container = container.child(if row {
                        div().flex_none().w(px(1.0)).h_full().bg(border)
                    } else {
                        div().flex_none().h(px(1.0)).w_full().bg(border)
                    });
                }
                container = container.child(layout_tree(child, border, leaf));
            }
            container.into_any_element()
        }
    }
}

fn rect(bounds: Bounds<Pixels>) -> Rect {
    Rect {
        x: f32::from(bounds.origin.x),
        y: f32::from(bounds.origin.y),
        w: f32::from(bounds.size.width),
        h: f32::from(bounds.size.height),
    }
}

pub fn zone_at(pane: Bounds<Pixels>, area: Bounds<Pixels>, position: Point<Pixels>) -> Zone {
    let area = if area.size.width > px(0.0) { area } else { pane };
    splits::zone(rect(pane), rect(area), f32::from(position.x), f32::from(position.y))
}

pub fn zone_hint(zone: Zone) -> &'static str {
    match zone {
        Zone::Edge(Direction::Left) => "split left",
        Zone::Edge(Direction::Right) => "split right",
        Zone::Edge(Direction::Up) => "split up",
        Zone::Edge(Direction::Down) => "split down",
        Zone::Outer(Direction::Left) => "left, full height",
        Zone::Outer(Direction::Right) => "right, full height",
        Zone::Outer(Direction::Up) => "top, full width",
        Zone::Outer(Direction::Down) => "bottom, full width",
        Zone::Center => "",
    }
}

pub struct Ghost {
    pub rect: Rect,
    pub glyph: (&'static str, Hsla),
    pub title: String,
    pub strong: bool,
    pub filled: bool,
    pub hint: Option<String>,
}

pub fn ghosts(items: Vec<Ghost>, cx: &App) -> AnyElement {
    let theme = ui::theme(cx);
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .children(items.into_iter().map(|ghost| {
            placed(ghost.rect)
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .p(px(8.0))
                .when(ghost.strong, |this| {
                    this.border_2()
                        .border_color(theme.accent)
                        .when(ghost.filled, |this| this.bg(theme.accent.opacity(0.18)))
                })
                .when(!ghost.strong, |this| {
                    this.bg(theme.background.opacity(0.6)).border_1().border_color(theme.accent.opacity(0.5))
                })
                .child(
                    div()
                        .flex()
                        .gap(px(6.0))
                        .px(px(6.0))
                        .max_w_full()
                        .when(ghost.strong, |this| this.bg(theme.accent).text_color(theme.on_accent))
                        .when(!ghost.strong, |this| this.bg(theme.elevated).text_color(theme.muted))
                        .child(
                            div()
                                .flex_none()
                                .text_color(if ghost.strong { theme.on_accent } else { ghost.glyph.1 })
                                .child(ghost.glyph.0),
                        )
                        .child(div().min_w_0().truncate().child(ghost.title)),
                )
                .children(
                    ghost
                        .hint
                        .filter(|hint| !hint.is_empty())
                        .map(|hint| div().px(px(6.0)).bg(theme.surface).text_color(theme.muted).child(hint)),
                )
        }))
        .into_any_element()
}

fn placed(rect: Rect) -> gpui::Div {
    div().absolute().left(relative(rect.x)).top(relative(rect.y)).w(relative(rect.w)).h(relative(rect.h))
}

fn hint(view: &WorkspaceView, target: &str, dragged: &str, zone: Zone, cx: &App) -> String {
    let state = view.state(cx);
    let together = state.is_some_and(|state| {
        let group = splits::group(&state.layouts, target);
        group.is_some() && group == splits::group(&state.layouts, dragged)
    });
    let action = match zone {
        Zone::Center if together => "swap",
        Zone::Center => "replace",
        zone => zone_hint(zone),
    };
    match fit(view, target, dragged, cx) {
        Fit::Adopt(path) => {
            let name = state.and_then(|state| state.project(&path)).map(|project| project.display_name());
            format!("{action} · moves to {}", name.unwrap_or_default())
        }
        _ => action.to_string(),
    }
}

fn preview(view: &WorkspaceView, shown: &Layout, cx: &App) -> Option<AnyElement> {
    let Some(DropTarget::Pane { target, dragged, zone }) = &view.drop_target else { return None };
    let shown_tabs = splits::tabs(shown);
    if !shown_tabs.contains(target) {
        return None;
    }
    let theme = ui::theme(cx);
    let unit = Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 };
    let overlay = div().absolute().top_0().left_0().size_full();
    if matches!(fit(view, target, dragged, cx), Fit::Blocked) {
        let mut rects = Vec::new();
        splits::rects(shown, unit, &mut rects);
        let rect = rects.into_iter().find(|(tab, _)| tab == target)?.1;
        return Some(
            overlay
                .child(
                    placed(rect)
                        .bg(theme.red.opacity(0.12))
                        .border_2()
                        .border_color(theme.red)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .px(px(6.0))
                                .bg(theme.red)
                                .text_color(theme.background)
                                .child("can't split tabs of different projects"),
                        ),
                )
                .into_any_element(),
        );
    }
    let layouts = view.state(cx).map(|state| state.layouts.clone()).unwrap_or_default();
    let result = splits::preview(&layouts, target, dragged, *zone);
    let mut rects = Vec::new();
    splits::rects(&result, unit, &mut rects);
    let hint = hint(view, target, dragged, *zone, cx);
    let items = rects
        .into_iter()
        .map(|(tab, rect)| {
            let info = view.store(cx).session(&tab).map(|session| TabInfo::new(session.read(cx), &theme, 0));
            let glyph = info
                .as_ref()
                .map(|info| (info.glyph.0, info.glyph.1.map(|color| color.color(theme.dark)).unwrap_or(theme.muted)))
                .unwrap_or(("", theme.muted));
            let strong = tab == *dragged;
            Ghost {
                rect,
                glyph,
                title: info.map(|info| info.title).unwrap_or_default(),
                strong,
                filled: true,
                hint: strong.then(|| hint.clone()),
            }
        })
        .collect();
    Some(ghosts(items, cx))
}

pub fn render(
    view: &WorkspaceView,
    layout: Option<&Layout>,
    focused: &str,
    cx: &mut Context<WorkspaceView>,
) -> AnyElement {
    let single = Layout::Pane { tab: focused.to_string() };
    let shown = layout.unwrap_or(&single);
    let content = match layout {
        Some(layout) => {
            let border = ui::theme(cx).border;
            layout_tree(layout, border, &mut |tab| pane(view, tab, focused, true, cx))
        }
        None => pane(view, focused, focused, false, cx),
    };
    div()
        .relative()
        .size_full()
        .flex()
        .child(content)
        .child(area_probe(view))
        .children(preview(view, shown, cx))
        .into_any_element()
}

pub fn area_probe(view: &WorkspaceView) -> impl IntoElement {
    let area = view.drop_area.clone();
    canvas(move |bounds, _, _| area.set(bounds), |_, _, _, _| {}).absolute().top_0().left_0().size_full()
}

pub fn split(view: &mut WorkspaceView, row: bool, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    if view.file_shown(cx) {
        crate::file_view::split(view, if row { Direction::Right } else { Direction::Down }, window, cx);
        return;
    }
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
        let zone = if row { Direction::Right } else { Direction::Down };
        arrange(view, &current, &id, Zone::Edge(zone), window, cx);
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

pub fn close_split(view: &mut WorkspaceView, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    if view.file_shown(cx) {
        crate::file_view::merge_group(view, window, cx);
    } else if let Some(id) = view.state(cx).and_then(|state| state.selected_tab()).map(String::from) {
        detach(view, &id, window, cx);
    }
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
    if view.file_shown(cx) {
        crate::file_view::focus_group(view, direction, window, cx);
        return;
    }
    let Some(state) = view.state(cx) else { return };
    let Some(current) = state.selected_tab() else { return };
    let Some(index) = splits::group(&state.layouts, current) else { return };
    if let Some(next) = splits::neighbor(&state.layouts[index], current, direction) {
        view.select_tab(&next, window, cx);
    }
}
