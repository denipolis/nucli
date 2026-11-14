use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, Context, Hsla, InteractiveElement, IntoElement, MouseButton, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, div, px,
};

use super::devices::Device;
use super::protocol::{Catalog, RemoteProject, RemoteTab};
use super::wire::Key;
use super::{Uplink, mirror_id};
use crate::icons;
use crate::menu::{self, Item};
use crate::session::{Activity, Kind};
use crate::sidebar::{TINT, matches, stripe};
use crate::theme::Theme;
use crate::ui;
use crate::window::WorkspaceView;

pub fn status_glyph(tab: &RemoteTab, theme: &Theme, tick: usize) -> Option<(SharedString, Hsla)> {
    if !tab.running {
        return tab.failed.then(|| (icons::CROSS.into(), theme.red));
    }
    match tab.activity {
        Activity::Waiting => Some((icons::QUESTION.into(), theme.orange)),
        Activity::Working => Some((icons::SPINNER[tick % icons::SPINNER.len()].into(), theme.accent)),
        Activity::Idle => tab.attention.then(|| (icons::CHECK.into(), theme.green)),
    }
}

fn tab_glyph(tab: &RemoteTab) -> (&'static str, Option<crate::theme::ItemColor>) {
    match (&tab.agent, tab.kind) {
        (Some(agent), _) => {
            let (glyph, color) = icons::agent(agent);
            (glyph, Some(color))
        }
        (None, Kind::Agent) => (icons::AGENT, None),
        _ => (icons::TERMINAL, None),
    }
}

pub fn seen(device: &Device) -> String {
    if device.seen == 0 {
        return "never connected".into();
    }
    let time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(device.seen);
    format!("seen {}", crate::history::relative_age(time))
}

fn note(text: impl Into<SharedString>, theme: &Theme) -> AnyElement {
    div().pl(px(26.0)).pr(px(8.0)).text_color(theme.faint).truncate().child(text.into()).into_any_element()
}

fn open_settings(view: &mut WorkspaceView, window: &mut gpui::Window, cx: &mut Context<WorkspaceView>) {
    crate::settings_view::show(view, Some(crate::persist::SettingsSection::Uplink), window, cx);
}

fn device_row(device: &Device, key: Key, online: bool, cx: &mut Context<WorkspaceView>) -> AnyElement {
    let theme = ui::theme(cx);
    let (status, color) = match (device.blocked, online) {
        (true, _) => ("blocked", theme.red),
        (false, true) => ("online", theme.green),
        (false, false) => ("offline", theme.faint),
    };
    div()
        .id(SharedString::from(format!("uplink-device-{}", device.key)))
        .flex()
        .items_center()
        .gap(px(6.0))
        .pl(px(8.0))
        .pr(px(8.0))
        .mt(px(4.0))
        .hover(|style| style.bg(theme.hover))
        .child(div().text_color(if online { theme.accent } else { theme.faint }).child(icons::os(&device.os)))
        .child(div().flex_1().min_w_0().truncate().child(device.name.clone()))
        .child(div().text_color(color).child(status))
        .on_click(cx.listener(|view, _, window, cx| open_settings(view, window, cx)))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |view, event: &gpui::MouseDownEvent, _, cx| {
                let mut items = vec![menu::action("Uplink Settings…", open_settings)];
                if online {
                    items.push(menu::action("Disconnect", move |_, _, cx| {
                        if let Some(uplink) = super::uplink(cx) {
                            uplink.update(cx, |uplink, cx| uplink.disconnect(&key, cx));
                        }
                    }));
                }
                view.open_menu(event.position, items, cx);
            }),
        )
        .into_any_element()
}

fn launcher_menu(key: Key, project: &RemoteProject, catalog: &Catalog, device: &str) -> Vec<Item> {
    let mut items = vec![Item::Header(format!("new tab on {device}").into())];
    for launcher in &catalog.launchers {
        let (path, name) = (project.path.clone(), launcher.name.clone());
        let label =
            if launcher.agent { format!("  {}", launcher.name) } else { format!("  {} (shell)", launcher.name) };
        items.push(menu::action(label, move |view, _, cx| {
            let workspace = view.workspace.clone();
            if let Some(uplink) = super::uplink(cx) {
                uplink.update(cx, |uplink, cx| uplink.request_open(key, &path, &name, &workspace, cx));
            }
        }));
    }
    items
}

fn project_row(
    key: Key,
    project: &RemoteProject,
    catalog: &Catalog,
    device: &str,
    cx: &mut Context<WorkspaceView>,
) -> AnyElement {
    let theme = ui::theme(cx);
    let tint = project.color.map(|color| color.color(theme.dark)).unwrap_or(theme.accent);
    let name = if project.name.is_empty() { "~".to_string() } else { project.name.clone() };
    let group = SharedString::from(format!("uplink-project-{}-{}", &super::wire::encode_key(&key)[..12], project.path));
    let can_open = catalog.control && !catalog.launchers.is_empty();
    let menu_items = launcher_menu(key, project, catalog, device);
    let button_items = menu_items.clone();
    div()
        .id(group.clone())
        .group(group.clone())
        .flex()
        .items_center()
        .gap(px(6.0))
        .pl(px(18.0))
        .pr(px(8.0))
        .hover(|style| style.bg(theme.hover))
        .child(div().text_color(tint).child(icons::FOLDER))
        .child(div().flex_1().min_w_0().truncate().child(name))
        .when(can_open, |this| {
            this.child(
                div()
                    .id(SharedString::from(format!("{group}-add")))
                    .text_color(theme.muted)
                    .invisible()
                    .group_hover(group.clone(), |style| style.visible())
                    .hover(|style| style.text_color(theme.accent))
                    .child(icons::ADD)
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |view, event: &gpui::ClickEvent, _, cx| {
                        view.open_menu(event.position(), button_items.clone(), cx);
                        cx.stop_propagation();
                    })),
            )
        })
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |view, event: &gpui::MouseDownEvent, _, cx| {
                if can_open {
                    view.open_menu(event.position, menu_items.clone(), cx);
                }
            }),
        )
        .into_any_element()
}

pub fn tab_row(
    key: Key,
    tab: &RemoteTab,
    indent: f32,
    selected: bool,
    opened: bool,
    place: Option<String>,
    cx: &mut Context<WorkspaceView>,
) -> AnyElement {
    let theme = ui::theme(cx);
    let tick = cx.global::<crate::store::StoreHandle>().0.read(cx).tick_fast;
    let (glyph, glyph_color) = tab_glyph(tab);
    let tint = tab.color.map(|color| color.color(theme.dark));
    let title_color = if selected {
        theme.on_accent
    } else if tab.attention {
        theme.green
    } else if tab.running {
        theme.text
    } else {
        theme.faint
    };
    let subtitle = match place {
        Some(place) => place,
        None if tab.running => {
            tab.program.clone().filter(|program| !tab.title.contains(program.as_str())).unwrap_or_default()
        }
        None => "exited".into(),
    };
    let (open_id, menu_id) = (tab.id.clone(), tab.id.clone());
    let title = tab.title.clone();
    div()
        .id(SharedString::from(format!("uplink-tab-{}", mirror_id(&key, &tab.id))))
        .relative()
        .flex()
        .items_center()
        .gap(px(6.0))
        .pl(px(indent))
        .pr(px(8.0))
        .when(selected, |this| this.bg(theme.accent))
        .when(!selected, |this| {
            this.when_some(tint, |this, tint| this.bg(tint.opacity(TINT))).hover(|style| style.bg(theme.hover))
        })
        .when_some(tint, |this, tint| this.child(stripe(tint)))
        .child(
            div()
                .text_color(if selected {
                    theme.on_accent
                } else {
                    glyph_color.map(|color| color.color(theme.dark)).unwrap_or(theme.muted)
                })
                .child(glyph),
        )
        .child(
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .gap(px(6.0))
                .overflow_hidden()
                .child(div().min_w_0().truncate().text_color(title_color).child(tab.title.clone()))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(if selected { theme.on_accent } else { theme.faint })
                        .child(subtitle),
                ),
        )
        .when(opened && !selected, |this| this.child(div().text_color(theme.accent).child(icons::EYE)))
        .children(
            status_glyph(tab, &theme, tick)
                .map(|(glyph, color)| div().text_color(if selected { theme.on_accent } else { color }).child(glyph)),
        )
        .on_click(cx.listener(move |view, _, _, cx| {
            let workspace = view.workspace.clone();
            if let Some(uplink) = super::uplink(cx) {
                uplink.update(cx, |uplink, cx| uplink.open_mirror(key, &open_id, &workspace, cx));
            }
        }))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |view, event: &gpui::MouseDownEvent, _, cx| {
                let id = mirror_id(&key, &menu_id);
                let open = menu_id.clone();
                let mut items = vec![menu::action("Open", move |view, _, cx| {
                    let workspace = view.workspace.clone();
                    if let Some(uplink) = super::uplink(cx) {
                        uplink.update(cx, |uplink, cx| uplink.open_mirror(key, &open, &workspace, cx));
                    }
                })];
                if opened {
                    items.push(menu::action("Close View", move |_, _, cx| {
                        if let Some(uplink) = super::uplink(cx) {
                            uplink.update(cx, |uplink, cx| uplink.close_mirror(&id, cx));
                        }
                    }));
                }
                items.push(Item::Separator);
                items.push(menu::copy("Copy Title", title.clone()));
                view.open_menu(event.position, items, cx);
            }),
        )
        .into_any_element()
}

fn empty(text: &'static str, action: &'static str, cx: &mut Context<WorkspaceView>) -> Vec<AnyElement> {
    let theme = ui::theme(cx);
    vec![
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .p(px(10.0))
            .text_color(theme.faint)
            .child(text)
            .child(
                div()
                    .id("uplink-empty-action")
                    .text_color(theme.accent)
                    .hover(|style| style.text_color(theme.text))
                    .child(action)
                    .on_click(cx.listener(|view, _, window, cx| open_settings(view, window, cx))),
            )
            .into_any_element(),
    ]
}

pub fn rows(view: &WorkspaceView, query: &str, cx: &mut Context<WorkspaceView>) -> Vec<AnyElement> {
    let theme = ui::theme(cx);
    let Some(entity) = super::uplink(cx) else { return Vec::new() };
    if !Uplink::enabled(cx) {
        return empty("Uplink is off.", "[turn on in settings]", cx);
    }
    let uplink = entity.read(cx);
    if uplink.devices.is_empty() {
        return empty("No paired devices yet.", "[pair a device]", cx);
    }
    let selected = view.state(cx).and_then(|state| state.selected_tab()).map(String::from);
    let mut devices: Vec<(Device, Key)> =
        uplink.devices.iter().filter_map(|device| Some((device.clone(), device.public()?))).collect();
    devices.sort_by_key(|(device, key)| (!uplink.links.contains_key(key), device.name.to_lowercase()));
    let catalogs: Vec<Option<Catalog>> = devices
        .iter()
        .map(|(_, key)| uplink.links.get(key).map(|link| link.catalog.clone().unwrap_or_default()))
        .collect();
    let errors: Vec<Option<String>> = devices.iter().map(|(_, key)| uplink.dial_error(key).map(String::from)).collect();
    let opened: Vec<String> = uplink.mirrors.keys().cloned().collect();
    let mut rows = Vec::new();
    for (((device, key), catalog), error) in devices.into_iter().zip(catalogs).zip(errors) {
        let device_hit = matches(query, &[&device.name]);
        let Some(catalog) = catalog else {
            if device_hit {
                rows.push(device_row(&device, key, false, cx));
                let detail = if device.blocked {
                    "blocked: it cannot connect".to_string()
                } else {
                    error.map(|error| format!("{} · {error}", seen(&device))).unwrap_or_else(|| seen(&device))
                };
                rows.push(note(detail, &theme));
            }
            continue;
        };
        let tab_hit = |tab: &RemoteTab, project: &str| device_hit || matches(query, &[&tab.title, project]);
        let mut body = Vec::new();
        for project in &catalog.projects {
            let tabs: Vec<&RemoteTab> =
                catalog.tabs.iter().filter(|tab| tab.project == project.path && tab_hit(tab, &project.name)).collect();
            if tabs.is_empty() && !(device_hit || matches(query, &[&project.name])) {
                continue;
            }
            let loose = project.path.is_empty();
            if !loose {
                body.push(project_row(key, project, &catalog, &device.name, cx));
            }
            for tab in tabs {
                let id = mirror_id(&key, &tab.id);
                let is_selected = selected.as_deref() == Some(id.as_str());
                body.push(tab_row(
                    key,
                    tab,
                    if loose { 18.0 } else { 30.0 },
                    is_selected,
                    opened.contains(&id),
                    None,
                    cx,
                ));
            }
        }
        if body.is_empty() && !device_hit {
            continue;
        }
        rows.push(device_row(&device, key, true, cx));
        if body.is_empty() {
            body.push(note("nothing shared", &theme));
        }
        rows.extend(body);
    }
    if rows.is_empty() {
        rows.push(div().p(px(10.0)).text_color(theme.faint).child("no matches").into_any_element());
    }
    rows
}

pub fn agents(query: &str, cx: &App) -> Vec<(usize, Key, RemoteTab, String)> {
    let Some(entity) = super::uplink(cx).filter(|_| Uplink::enabled(cx)) else { return Vec::new() };
    let uplink = entity.read(cx);
    let mut agents = Vec::new();
    for (key, link) in &uplink.links {
        let Some(catalog) = &link.catalog else { continue };
        let device = uplink.device_name(key);
        for tab in &catalog.tabs {
            if !tab.running || (tab.kind != Kind::Agent && tab.agent.is_none()) {
                continue;
            }
            let project = catalog
                .projects
                .iter()
                .find(|project| project.path == tab.project)
                .map(|project| project.name.clone())
                .filter(|name| !name.is_empty());
            let place = match project {
                Some(project) => format!("{device} · {project}"),
                None => device.clone(),
            };
            if !matches(query, &[&tab.title, &place]) {
                continue;
            }
            let group = match (tab.activity, tab.attention) {
                (Activity::Waiting, _) => 0,
                (_, true) => 1,
                (Activity::Working, _) => 2,
                _ => 3,
            };
            agents.push((group, *key, tab.clone(), place));
        }
    }
    agents.sort_by(|a, b| a.3.cmp(&b.3).then_with(|| a.2.title.cmp(&b.2.title)));
    agents
}

pub fn watched(id: &str, cx: &App) -> bool {
    super::uplink(cx).is_some_and(|uplink| !uplink.read(cx).watchers_of(id).is_empty())
}

pub fn palette_items(theme: &Theme, cx: &App) -> Vec<crate::palette::Item> {
    let Some(entity) = super::uplink(cx).filter(|_| Uplink::enabled(cx)) else { return Vec::new() };
    let uplink = entity.read(cx);
    let mut items = Vec::new();
    for (key, link) in &uplink.links {
        let Some(catalog) = &link.catalog else { continue };
        let device = uplink.device_name(key);
        for tab in &catalog.tabs {
            let project = catalog
                .projects
                .iter()
                .find(|project| project.path == tab.project)
                .map(|project| project.name.clone())
                .filter(|name| !name.is_empty());
            let (glyph, color) = tab_glyph(tab);
            let (key, id) = (*key, tab.id.clone());
            items.push(
                crate::palette::Item::new(tab.title.clone(), move |view, _, cx| {
                    let workspace = view.workspace.clone();
                    if let Some(uplink) = super::uplink(cx) {
                        uplink.update(cx, |uplink, cx| uplink.open_mirror(key, &id, &workspace, cx));
                    }
                })
                .subtitle(match project {
                    Some(project) => format!("{device} · {project}"),
                    None => device.clone(),
                })
                .glyph(glyph, color.map(|color| color.color(theme.dark)))
                .detail("remote tab")
                .rank(7),
            );
        }
    }
    items
}
