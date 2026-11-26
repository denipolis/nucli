use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, Context, Hsla, InteractiveElement, IntoElement, MouseButton, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, div, px,
};

use super::devices::Device;
use super::protocol::RemoteTab;
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
                let open = menu_id.clone();
                let mut items = vec![menu::action("Open", move |view, _, cx| {
                    let workspace = view.workspace.clone();
                    if let Some(uplink) = super::uplink(cx) {
                        uplink.update(cx, |uplink, cx| uplink.open_mirror(key, &open, &workspace, cx));
                    }
                })];
                items.push(Item::Separator);
                items.push(menu::copy("Copy Title", title.clone()));
                view.open_menu(event.position, items, cx);
            }),
        )
        .into_any_element()
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
    items.extend(workspace_items(cx));
    items
}

pub fn workspace_items(cx: &App) -> Vec<crate::palette::Item> {
    let Some(entity) = super::uplink(cx).filter(|_| Uplink::enabled(cx)) else { return Vec::new() };
    let uplink = entity.read(cx);
    let mut items = Vec::new();
    for (key, link) in &uplink.links {
        let Some(catalog) = &link.catalog else { continue };
        let device = uplink.device_name(key);
        for workspace in &catalog.workspaces {
            let tabs = catalog.tabs.iter().filter(|tab| tab.workspace == workspace.name).count();
            let (key, name) = (*key, workspace.name.clone());
            items.push(
                crate::palette::Item::new(format!("{device} › {}", workspace.name), move |_, _, cx| {
                    open_workspace(key, name.clone(), cx)
                })
                .subtitle(format!(
                    "{tabs} shared {}{}",
                    if tabs == 1 { "tab" } else { "tabs" },
                    if workspace.open { "" } else { ", closed there" }
                ))
                .glyph(icons::UPLINK, None)
                .detail("remote workspace")
                .rank(6),
            );
        }
    }
    items
}

pub fn workspace_menu_items(view: &WorkspaceView, cx: &App) -> Vec<Item> {
    let Some(entity) = super::uplink(cx).filter(|_| Uplink::enabled(cx)) else { return Vec::new() };
    let uplink = entity.read(cx);
    let current = view.state(cx).and_then(|state| Some((state.host.as_ref()?.key.clone(), state.name.clone())));
    let mut devices: Vec<_> = uplink
        .links
        .iter()
        .filter_map(|(key, link)| Some((uplink.device_name(key), *key, link.catalog.as_ref()?)))
        .filter(|(_, _, catalog)| !catalog.workspaces.is_empty())
        .collect();
    devices.sort_by(|a, b| a.0.cmp(&b.0));
    let mut items = Vec::new();
    for (device, key, catalog) in devices {
        let encoded = super::wire::encode_key(&key);
        items.push(Item::Header(device.into()));
        for workspace in &catalog.workspaces {
            let here = current.as_ref().is_some_and(|(host, name)| *host == encoded && *name == workspace.name);
            let label = if here {
                format!("● {}", workspace.name)
            } else if workspace.open {
                format!("  {}", workspace.name)
            } else {
                format!("  {} (closed)", workspace.name)
            };
            let name = workspace.name.clone();
            items.push(menu::action(label, move |_, _, cx| open_workspace(key, name.clone(), cx)));
        }
    }
    items
}

fn open_workspace(key: Key, name: String, cx: &mut App) {
    cx.defer(move |cx| {
        let Some(uplink) = super::uplink(cx) else { return };
        let id = uplink.update(cx, |uplink, cx| uplink.open_remote(key, &name, cx));
        crate::window::open(&id, cx);
    });
}
