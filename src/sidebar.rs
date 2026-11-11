use std::path::PathBuf;

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, AppContext, Context, Hsla, InteractiveElement, IntoElement, MouseButton, ParentElement,
    SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::history::relative_age;
use crate::icons;
use crate::persist::{Selected, SidebarTab};
use crate::session::{Activity, Kind, Session};
use crate::settings;
use crate::store::{self, Project};
use crate::ui;
use crate::window::{Rename, WorkspaceView};

#[derive(Clone)]
pub struct DraggedTab {
    pub id: String,
    pub title: String,
}

#[derive(Clone)]
pub struct DraggedProject {
    pub path: PathBuf,
    pub title: String,
}

pub struct DragPreview(pub String);

impl gpui::Render for DragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = ui::theme(cx);
        div().px(px(6.0)).bg(theme.accent).text_color(theme.on_accent).font_family(settings::FONT).child(self.0.clone())
    }
}

pub struct TabInfo {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub glyph: (&'static str, Option<crate::theme::ItemColor>),
    pub color: Option<crate::theme::ItemColor>,
    pub attention: bool,
    pub running: bool,
    pub status: Option<(SharedString, Hsla)>,
    pub terminal: bool,
    pub loose: bool,
    pub subagents: Vec<crate::claude::Subagent>,
}

impl TabInfo {
    pub fn new(session: &Session, theme: &crate::theme::Theme, tick: usize) -> TabInfo {
        let title = session.display_title();
        let subtitle = if session.is_running() {
            session
                .program()
                .filter(|name| !title.contains(name))
                .filter(|name| Some(*name) != session.running_agent.as_deref() || session.kind != Kind::Agent)
                .unwrap_or_default()
                .to_string()
        } else {
            "exited".to_string()
        };
        TabInfo {
            id: session.id.clone(),
            title,
            subtitle,
            glyph: tab_glyph(session),
            color: session.color,
            attention: session.attention,
            running: session.is_running(),
            status: status_glyph(session, theme, tick),
            terminal: session.kind == Kind::Terminal,
            loose: false,
            subagents: session.subagents.clone(),
        }
    }
}

pub fn matches(query: &str, texts: &[&str]) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty() || texts.iter().any(|text| text.to_lowercase().contains(&query))
}

fn tab_texts(session: &Session) -> Vec<String> {
    let mut texts = vec![session.display_title(), session.working_dir().to_string_lossy().into_owned()];
    texts.extend(session.profile.clone());
    texts
}

pub fn first_match(view: &WorkspaceView, query: &str, cx: &App) -> Option<Selected> {
    let state = view.state(cx)?;
    let store = view.store(cx);
    for project in state.all_projects() {
        let project_hit =
            !project.is_scratch() && matches(query, &[&project.display_name(), &project.path.to_string_lossy()]);
        if project_hit {
            return Some(Selected::Project(project.path.clone()));
        }
        for id in &project.tabs {
            if let Some(session) = store.session(id)
                && matches(query, &tab_texts(session.read(cx)).iter().map(String::as_str).collect::<Vec<_>>())
            {
                return Some(Selected::Tab(id.clone()));
            }
        }
    }
    None
}

pub fn status_glyph(session: &Session, theme: &crate::theme::Theme, tick: usize) -> Option<(SharedString, Hsla)> {
    if let Some(code) = session.exit {
        if code != Some(0) {
            return Some((icons::CROSS.into(), theme.red));
        }
        return None;
    }
    if session.activity == Activity::Waiting {
        return Some((icons::QUESTION.into(), theme.orange));
    }
    if session.activity == Activity::Working {
        return Some((icons::SPINNER[tick % icons::SPINNER.len()].into(), theme.accent));
    }
    if session.attention {
        return Some((icons::CHECK.into(), theme.green));
    }
    None
}

pub fn tab_glyph(session: &Session) -> (&'static str, Option<crate::theme::ItemColor>) {
    let agent = session.running_agent.as_deref().or(session.agent.map(|kind| kind.command()));
    match (agent, session.kind) {
        (Some(name), _) => {
            let (glyph, color) = icons::agent(name);
            (glyph, Some(color))
        }
        (None, Kind::Agent) => (icons::AGENT, None),
        _ => (icons::TERMINAL, None),
    }
}

fn header(view: &WorkspaceView, cx: &mut Context<WorkspaceView>) -> impl IntoElement {
    let theme = ui::theme(cx);
    let current = view.store(cx).ui.sidebar_tab;
    let compact = view.state(cx).is_some_and(|state| state.window.sidebar_width < 260.0);
    let tab = |id: &'static str, glyph: &'static str, label: &'static str, tab: SidebarTab| {
        let active = current == tab;
        div()
            .id(id)
            .flex()
            .gap(px(8.0))
            .px(px(4.0))
            .when(active, |this| this.bg(theme.accent).text_color(theme.on_accent))
            .when(!active, |this| this.text_color(theme.muted).hover(|style| style.text_color(theme.text)))
            .child(glyph)
            .when(active || !compact, |this| this.child(label))
            .on_click(move |_, window, cx| {
                store::store(cx).update(cx, |store, cx| {
                    store.ui.sidebar_tab = tab;
                    if tab == SidebarTab::History {
                        store.refresh_history(cx);
                    }
                    store.changed(cx);
                });
                window.refresh();
            })
    };
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(6.0))
        .px(px(8.0))
        .py(px(4.0))
        .overflow_hidden()
        .border_b_1()
        .border_color(theme.border)
        .child(tab("sidebar-projects", icons::FOLDER, "Projects", SidebarTab::Projects))
        .child(tab("sidebar-agents", icons::AGENT, "Agents", SidebarTab::Agents))
        .child(tab("sidebar-history", icons::HISTORY, "History", SidebarTab::History))
}

fn rename_field(view: &WorkspaceView, matches_target: impl Fn(&Rename) -> bool) -> Option<AnyElement> {
    let (target, input) = view.rename.as_ref()?;
    matches_target(target).then(|| div().flex_1().child(input.clone()).into_any_element())
}

fn hidden_status(view: &WorkspaceView, project: &Project, cx: &App) -> Option<(SharedString, Hsla)> {
    let store = view.store(cx);
    let sessions: Vec<&Session> =
        project.tabs.iter().filter_map(|id| store.session(id)).map(|session| session.read(cx)).collect();
    let session = sessions
        .iter()
        .find(|session| session.activity == Activity::Waiting)
        .or_else(|| sessions.iter().find(|session| session.activity == Activity::Working))
        .or_else(|| sessions.iter().find(|session| session.attention))?;
    status_glyph(session, &ui::theme(cx), store.tick_fast)
}

fn project_row(
    view: &WorkspaceView,
    project: &Project,
    selected: bool,
    status: Option<(SharedString, Hsla)>,
    cx: &mut Context<WorkspaceView>,
) -> AnyElement {
    let theme = ui::theme(cx);
    let tint = project.color.map(|color| color.color(theme.dark)).unwrap_or(theme.accent);
    let path = project.path.clone();
    let stat = view.store(cx).diffstats.get(&project.path).copied();
    let name = project.display_name();
    let renaming = rename_field(view, |target| matches!(target, Rename::Project(p) if *p == path));
    let chevron = if project.collapsed { icons::CHEVRON_RIGHT } else { icons::CHEVRON_DOWN };
    let (text, faint) = if selected { (theme.on_accent, theme.on_accent) } else { (theme.text, theme.faint) };
    let id = SharedString::from(format!("project-{}", project.path.display()));
    let click_path = path.clone();
    let menu_path = path.clone();
    let toggle_path = path.clone();
    let drop_path = path.clone();
    let drop_tab_path = path.clone();
    let dragged = DraggedProject { path: path.clone(), title: name.clone() };
    let (accent, selection) = (theme.accent, theme.selection);
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(6.0))
        .pl(px(6.0))
        .pr(px(8.0))
        .when(selected, |this| this.bg(theme.accent))
        .when(!selected, |this| this.hover(|style| style.bg(theme.hover)))
        .text_color(text)
        .child(
            div()
                .id(SharedString::from(format!("chevron-{}", path.display())))
                .text_color(faint)
                .child(chevron)
                .on_click(cx.listener(move |view, _, _, cx| {
                    let id = view.workspace.clone();
                    let path = toggle_path.clone();
                    store::store(cx).update(cx, |store, cx| {
                        if let Some(project) = store.workspace_mut(&id).and_then(|state| state.project_mut(&path)) {
                            project.collapsed = !project.collapsed;
                        }
                        store.changed(cx);
                    });
                    cx.stop_propagation();
                })),
        )
        .child(div().text_color(if selected { theme.on_accent } else { tint }).child(icons::FOLDER))
        .child(renaming.unwrap_or_else(|| div().flex_1().min_w_0().truncate().child(name.clone()).into_any_element()))
        .children(status.map(|(glyph, color)| {
            div().flex_none().text_color(if selected { theme.on_accent } else { color }).child(glyph)
        }))
        .children(stat.filter(|stat| stat.added + stat.removed > 0).map(|stat| {
            div()
                .flex()
                .gap(px(4.0))
                .child(
                    div()
                        .text_color(if selected { theme.on_accent } else { theme.green })
                        .child(format!("+{}", stat.added)),
                )
                .child(
                    div()
                        .text_color(if selected { theme.on_accent } else { theme.red })
                        .child(format!("-{}", stat.removed)),
                )
        }))
        .on_click(cx.listener(move |view, event: &gpui::ClickEvent, window, cx| {
            if event.click_count() == 2 {
                crate::actions::begin_rename(view, Rename::Project(click_path.clone()), window, cx);
            } else {
                crate::actions::show_project(view, &click_path, window, cx);
            }
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |view, event: &gpui::MouseDownEvent, _, cx| {
                let items = crate::actions::project_menu(view, &menu_path, cx);
                view.open_menu(event.position, items, cx);
            }),
        )
        .on_drag(dragged, |drag: &DraggedProject, _, _, cx| cx.new(|_| DragPreview(drag.title.clone())))
        .drag_over::<DraggedProject>(move |style, _, _, _| style.border_t_1().border_color(accent))
        .on_drop(cx.listener(move |view, drag: &DraggedProject, _, cx| {
            crate::actions::move_project(view, &drag.path, &drop_path, cx);
        }))
        .drag_over::<DraggedTab>(move |style, _, _, _| style.bg(selection))
        .on_drop(cx.listener(move |view, drag: &DraggedTab, window, cx| {
            crate::actions::move_tab(view, &drag.id, &drop_tab_path, None, window, cx);
        }))
        .into_any_element()
}

#[derive(Clone, Copy)]
pub struct Link {
    pub first: bool,
    pub last: bool,
    pub active: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum SplitAction {
    Join,
    Leave,
}

const JOIN_TIP: &str = if cfg!(target_os = "macos") {
    "Split with the shown tab [⌘-click]"
} else {
    "Split with the shown tab [ctrl-click]"
};
const LEAVE_TIP: &str =
    if cfg!(target_os = "macos") { "Remove from split [⌘-click]" } else { "Remove from split [ctrl-click]" };

fn gutter(link: Option<Link>, tick: bool, loose: bool, theme: &crate::theme::Theme) -> gpui::Div {
    let width = if loose { 10.0 } else { 12.0 };
    let x = px(width - 8.0);
    let gutter = div().relative().flex_none().w(px(width));
    let Some(link) = link else { return gutter };
    let color = if link.active { theme.accent } else { theme.faint };
    let half: gpui::Length = gpui::relative(0.5).into();
    let edge: gpui::Length = px(0.0).into();
    gutter
        .child(
            div()
                .absolute()
                .left(x)
                .w(px(2.0))
                .bg(color)
                .top(if link.first && tick { half } else { edge })
                .bottom(if link.last && tick { half } else { edge }),
        )
        .when(tick, |this| {
            this.child(div().absolute().left(x).top(gpui::relative(0.5)).mt(px(-1.0)).w(px(8.0)).h(px(2.0)).bg(color))
        })
}

pub const TINT: f32 = 0.14;

pub fn stripe(color: Hsla) -> gpui::Div {
    div().absolute().left_0().top_0().bottom_0().w(px(2.0)).bg(color)
}

#[allow(clippy::too_many_arguments)]
fn tab_row(
    view: &WorkspaceView,
    project: &Project,
    info: TabInfo,
    number: usize,
    selected: bool,
    link: Option<Link>,
    cx: &mut Context<WorkspaceView>,
) -> AnyElement {
    let theme = ui::theme(cx);
    let id = info.id.clone();
    let glyph_color = info.glyph.1.or(project.color).map(|color| color.color(theme.dark));
    let tint = info.color.map(|color| color.color(theme.dark));
    let renaming = rename_field(view, |target| matches!(target, Rename::Tab(tab) if *tab == id));
    let title_color = if selected {
        theme.on_accent
    } else if info.attention {
        theme.green
    } else if info.running {
        theme.text
    } else {
        theme.faint
    };
    let action = if link.is_some() {
        Some(SplitAction::Leave)
    } else {
        crate::splits_view::can_join(view, &id, cx).then_some(SplitAction::Join)
    };
    let (click_id, menu_id, drop_id, drop_project) = (id.clone(), id.clone(), id.clone(), project.path.clone());
    let action_id = id.clone();
    let dragged = DraggedTab { id: id.clone(), title: info.title.clone() };
    let accent = theme.accent;
    let group = SharedString::from(format!("tab-row-{id}"));
    let inner = div()
        .flex()
        .flex_1()
        .min_w_0()
        .items_center()
        .gap(px(6.0))
        .pl(px(6.0))
        .pr(px(8.0))
        .relative()
        .when(selected, |this| this.bg(theme.accent))
        .when(!selected, |this| {
            this.when_some(tint, |this, tint| this.bg(tint.opacity(TINT)))
                .group_hover(group.clone(), |style| style.bg(theme.hover))
        })
        .when_some(tint, |this, tint| this.child(stripe(tint)))
        .child(
            div()
                .w(px(14.0))
                .text_color(if selected { theme.on_accent } else { theme.faint })
                .child(if (1..=9).contains(&number) { number.to_string() } else { String::new() }),
        )
        .child(
            div()
                .text_color(if selected { theme.on_accent } else { glyph_color.unwrap_or(theme.muted) })
                .child(info.glyph.0),
        )
        .child(renaming.unwrap_or_else(|| {
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .gap(px(6.0))
                .overflow_hidden()
                .child(div().min_w_0().truncate().text_color(title_color).child(info.title.clone()))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(if selected { theme.on_accent } else { theme.faint })
                        .child(info.subtitle.clone()),
                )
                .into_any_element()
        }))
        .children(
            info.status
                .clone()
                .map(|(glyph, color)| div().text_color(if selected { theme.on_accent } else { color }).child(glyph)),
        )
        .children(action.map(|action| {
            let (glyph, tip) = match action {
                SplitAction::Join => (icons::SPLIT, JOIN_TIP),
                SplitAction::Leave => (icons::UNSPLIT, LEAVE_TIP),
            };
            div()
                .id(SharedString::from(format!("tab-split-{id}")))
                .flex_none()
                .text_color(if selected { theme.on_accent } else { theme.muted })
                .invisible()
                .group_hover(group.clone(), |style| style.visible())
                .hover(|style| style.text_color(if selected { theme.on_accent } else { theme.accent }))
                .child(glyph)
                .tooltip(ui::tooltip(tip))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |view, _, window, cx| {
                    match action {
                        SplitAction::Join => crate::splits_view::toggle(view, &action_id, window, cx),
                        SplitAction::Leave => crate::splits_view::detach(view, &action_id, window, cx),
                    }
                    cx.stop_propagation();
                }))
        }));
    let mut row = div()
        .id(SharedString::from(format!("tab-{id}")))
        .group(group)
        .flex()
        .child(gutter(link, true, info.loose, &theme))
        .child(inner)
        .on_click(cx.listener(move |view, event: &gpui::ClickEvent, window, cx| {
            if event.modifiers().secondary() {
                crate::splits_view::toggle(view, &click_id, window, cx);
            } else if event.click_count() == 2 {
                crate::actions::begin_rename(view, Rename::Tab(click_id.clone()), window, cx);
            } else {
                view.select_tab(&click_id, window, cx);
            }
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |view, event: &gpui::MouseDownEvent, _, cx| {
                let items = crate::actions::tab_menu(view, &menu_id, cx);
                view.open_menu(event.position, items, cx);
            }),
        )
        .on_drag(dragged, |drag: &DraggedTab, _, _, cx| cx.new(|_| DragPreview(drag.title.clone())))
        .drag_over::<DraggedTab>(move |style, _, _, _| style.border_t_1().border_color(accent))
        .on_drop(cx.listener(move |view, drag: &DraggedTab, window, cx| {
            crate::actions::move_tab(view, &drag.id, &drop_project, Some(drop_id.clone()), window, cx);
        }));
    if info.terminal {
        row = row.opacity(0.8);
    }
    row.into_any_element()
}

fn projects(view: &WorkspaceView, query: &str, cx: &mut Context<WorkspaceView>) -> Vec<AnyElement> {
    let Some(state) = view.state(cx).cloned() else { return Vec::new() };
    let store = view.store(cx);
    let tick = store.tick_fast;
    let sessions: Vec<(String, gpui::Entity<Session>)> = state
        .all_projects()
        .flat_map(|project| project.tabs.iter())
        .filter_map(|id| store.session(id).map(|session| (id.clone(), session.clone())))
        .collect();
    let mut rows = Vec::new();
    let order = state.ordered_tabs();
    for project in state.all_projects() {
        let project_hit = matches(query, &[&project.display_name(), &project.path.to_string_lossy()]);
        let tabs: Vec<&String> = project
            .tabs
            .iter()
            .filter(|id| {
                project_hit
                    || sessions.iter().find(|(tab, _)| tab == *id).is_some_and(|(_, session)| {
                        let texts = tab_texts(session.read(cx));
                        matches(query, &texts.iter().map(String::as_str).collect::<Vec<_>>())
                    })
            })
            .collect();
        if !project_hit && tabs.is_empty() {
            continue;
        }
        let loose = project.is_scratch();
        let collapsed = !loose && project.collapsed && query.trim().is_empty();
        if !loose {
            let selected = state.selected == Some(Selected::Project(project.path.clone()));
            let status = if collapsed { hidden_status(view, project, cx) } else { None };
            rows.push(project_row(view, project, selected, status, cx));
        }
        if collapsed {
            continue;
        }
        let selected_group = state.selected_tab().and_then(|selected| crate::splits::group(&state.layouts, selected));
        let group_of = |id: Option<&&String>| id.and_then(|id| crate::splits::group(&state.layouts, id));
        for (index, id) in tabs.iter().enumerate() {
            let Some((_, session)) = sessions.iter().find(|(tab, _)| tab == *id) else { continue };
            let number = order.iter().position(|tab| tab == *id).map(|index| index + 1).unwrap_or(0);
            let group = group_of(Some(id));
            let link = group.map(|group| Link {
                first: group_of(index.checked_sub(1).and_then(|index| tabs.get(index))) != Some(group),
                last: group_of(tabs.get(index + 1)) != Some(group),
                active: selected_group == Some(group) && !state.file_shown(),
            });
            let selected = state.selected_tab() == Some(id.as_str());
            let theme = ui::theme(cx);
            let mut info = TabInfo::new(session.read(cx), &theme, tick);
            info.loose = loose;
            let subagents = info.subagents.clone();
            rows.push(tab_row(view, project, info, number, selected, link, cx));
            let through = link.filter(|link| !link.last);
            for subagent in subagents {
                let parent = (*id).clone();
                rows.push(
                    div()
                        .id(SharedString::from(format!("subagent-{}", subagent.id)))
                        .flex()
                        .child(gutter(through, false, loose, &theme))
                        .child(
                            div()
                                .flex()
                                .flex_1()
                                .min_w_0()
                                .gap(px(6.0))
                                .pl(px(if loose { 36.0 } else { 34.0 }))
                                .pr(px(8.0))
                                .text_color(theme.muted)
                                .hover(|style| style.bg(theme.hover))
                                .child("↳")
                                .child(div().flex_1().truncate().child(subagent.description.clone()))
                                .child(div().text_color(theme.faint).child(subagent.kind.clone())),
                        )
                        .on_click(cx.listener(move |view, _, window, cx| view.select_tab(&parent, window, cx)))
                        .into_any_element(),
                );
            }
        }
    }
    rows
}

type BoardGroup = Vec<(String, gpui::Entity<Session>, PathBuf)>;

fn board(view: &WorkspaceView, query: &str, cx: &mut Context<WorkspaceView>) -> Vec<AnyElement> {
    let Some(state) = view.state(cx).cloned() else { return Vec::new() };
    let theme = ui::theme(cx);
    let store = view.store(cx);
    let tick = store.tick_fast;
    let mut groups: Vec<(&str, BoardGroup)> =
        vec![("Waiting", Vec::new()), ("Done", Vec::new()), ("Working", Vec::new()), ("Idle", Vec::new())];
    for project in state.all_projects() {
        for id in &project.tabs {
            let Some(session) = store.session(id) else { continue };
            let data = session.read(cx);
            if !data.is_running() || (data.kind != Kind::Agent && data.running_agent.is_none()) {
                continue;
            }
            if !matches(query, &[&data.display_title(), &project.display_name()]) {
                continue;
            }
            let index = match (data.activity, data.attention) {
                (Activity::Waiting, _) => 0,
                (_, true) => 1,
                (Activity::Working, _) => 2,
                _ => 3,
            };
            groups[index].1.push((id.clone(), session.clone(), project.path.clone()));
        }
    }
    let mut rows = Vec::new();
    for (title, items) in groups {
        if items.is_empty() {
            continue;
        }
        rows.push(
            div()
                .px(px(8.0))
                .pt(px(6.0))
                .text_color(theme.faint)
                .child(format!("{title} {}", items.len()))
                .into_any_element(),
        );
        for (id, session, path) in items {
            let project = state.project(&path).cloned().unwrap_or_default();
            let selected = state.selected_tab() == Some(id.as_str());
            let info = TabInfo::new(session.read(cx), &theme, tick);
            rows.push(tab_row(view, &project, info, 0, selected, None, cx));
        }
    }
    if rows.is_empty() {
        rows.push(div().p(px(10.0)).text_color(theme.faint).child("no running agents").into_any_element());
    }
    rows
}

fn history(view: &WorkspaceView, query: &str, cx: &mut Context<WorkspaceView>) -> Vec<AnyElement> {
    let theme = ui::theme(cx);
    let Some(list) = view.store(cx).history.clone() else {
        return vec![div().p(px(10.0)).text_color(theme.faint).child("loading…").into_any_element()];
    };
    let mut rows = Vec::new();
    for (index, conversation) in list.into_iter().enumerate() {
        let folder = crate::paths::display(&conversation.cwd);
        if !matches(query, &[&conversation.title, &folder]) {
            continue;
        }
        let age = relative_age(conversation.modified);
        let item = conversation.clone();
        let (glyph, color) = icons::agent(conversation.kind.command());
        rows.push(
            div()
                .id(("history", index))
                .flex()
                .flex_col()
                .px(px(8.0))
                .py(px(3.0))
                .hover(|style| style.bg(theme.hover))
                .child(
                    div()
                        .flex()
                        .gap(px(6.0))
                        .child(div().text_color(color.color(theme.dark)).child(glyph))
                        .child(div().flex_1().truncate().child(conversation.title.clone())),
                )
                .child(
                    div()
                        .pl(px(20.0))
                        .truncate()
                        .text_color(theme.faint)
                        .child(format!("{} · {age} · {folder}", conversation.profile)),
                )
                .on_click(cx.listener(move |view, _, window, cx| {
                    crate::actions::resume_anywhere(view, item.clone(), window, cx)
                }))
                .into_any_element(),
        );
    }
    if rows.is_empty() {
        rows.push(div().p(px(10.0)).text_color(theme.faint).child("no conversations").into_any_element());
    }
    rows
}

fn footer(view: &WorkspaceView, cx: &mut Context<WorkspaceView>) -> impl IntoElement {
    let theme = ui::theme(cx);
    let name = view.state(cx).map(|state| state.name.clone()).unwrap_or_default();
    div()
        .flex_none()
        .border_t_1()
        .border_color(theme.border)
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .px(px(8.0))
                .py(px(2.0))
                .border_b_1()
                .border_color(theme.border)
                .child(div().text_color(theme.faint).child("/"))
                .child(view.filter.clone()),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .px(px(8.0))
                .py(px(3.0))
                .child(
                    div()
                        .id("workspace-popup")
                        .flex()
                        .gap(px(6.0))
                        .text_color(theme.muted)
                        .hover(|style| style.text_color(theme.text))
                        .child(icons::WINDOW)
                        .child(name)
                        .child(icons::CHEVRON_UP)
                        .on_click(cx.listener(|view, event: &gpui::ClickEvent, _, cx| {
                            let items = crate::actions::workspace_menu(view, cx);
                            view.open_menu(event.position(), items, cx);
                        })),
                )
                .child(
                    div()
                        .id("sidebar-add")
                        .px(px(4.0))
                        .text_color(theme.muted)
                        .hover(|style| style.text_color(theme.accent))
                        .child(icons::ADD)
                        .on_click(cx.listener(|view, event: &gpui::ClickEvent, _, cx| {
                            let items = crate::actions::add_menu(cx);
                            view.open_menu(event.position(), items, cx);
                        })),
                ),
        )
}

pub fn render(view: &mut WorkspaceView, _window: &mut Window, cx: &mut Context<WorkspaceView>) -> AnyElement {
    let theme = ui::theme(cx);
    let width = view.state(cx).map(|state| state.window.sidebar_width).unwrap_or(260.0);
    let query = view.filter.read(cx).text.clone();
    let rows = match view.store(cx).ui.sidebar_tab {
        SidebarTab::Projects => projects(view, &query, cx),
        SidebarTab::Agents => board(view, &query, cx),
        SidebarTab::History => history(view, &query, cx),
    };
    let empty_click = cx.listener(|view, _: &gpui::MouseDownEvent, window, cx| {
        view.select(None, window, cx);
    });
    let selection = theme.selection;
    let empty_menu = cx.listener(|view, event: &gpui::MouseDownEvent, _, cx| {
        let items = crate::actions::workspace_menu(view, cx);
        view.open_menu(event.position, items, cx);
    });
    div()
        .flex()
        .flex_col()
        .flex_none()
        .w(px(width))
        .h_full()
        .bg(theme.surface)
        .border_r_1()
        .border_color(theme.border)
        .child(header(view, cx))
        .child(
            div()
                .id("sidebar-list")
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .py(px(4.0))
                .child(div().flex_none().children(rows))
                .child(
                    div()
                        .id("sidebar-empty")
                        .min_h(px(80.0))
                        .flex_1()
                        .on_mouse_down(MouseButton::Left, empty_click)
                        .on_mouse_down(MouseButton::Right, empty_menu)
                        .drag_over::<DraggedTab>(move |style, _, _, _| style.bg(selection))
                        .on_drop(cx.listener(|view, drag: &DraggedTab, window, cx| {
                            crate::actions::move_tab(view, &drag.id, std::path::Path::new(""), None, window, cx);
                        })),
                ),
        )
        .child(footer(view, cx))
        .into_any_element()
}
