use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Context, Div, Hsla, InteractiveElement, IntoElement, ParentElement, SharedString, Stateful,
    StatefulInteractiveElement, Styled, div, px,
};
use nucli_core::icons;
use nucli_core::session::{Activity, Kind};
use nucli_core::theme::Theme;
use nucli_core::uplink::protocol::{Catalog, RemoteTab};
use nucli_core::uplink::wire::{self, Key};

use crate::home::{Board, Home, Screen};
use crate::ui;

#[derive(Clone, Copy, PartialEq)]
enum Group {
    Waiting,
    Done,
    Working,
    Idle,
}

const GROUPS: [(Group, &str); 4] =
    [(Group::Waiting, "waiting"), (Group::Done, "done"), (Group::Working, "working"), (Group::Idle, "idle")];

fn group(tab: &RemoteTab) -> Group {
    if tab.running && tab.activity == Activity::Waiting {
        Group::Waiting
    } else if tab.running && tab.activity == Activity::Working {
        Group::Working
    } else if tab.attention {
        Group::Done
    } else {
        Group::Idle
    }
}

pub fn glyph(tab: &RemoteTab) -> (&'static str, Option<nucli_core::theme::ItemColor>) {
    match (tab.kind, &tab.agent) {
        (Kind::Agent, Some(agent)) => {
            let (glyph, color) = icons::agent(agent);
            (glyph, Some(color))
        }
        (Kind::Agent, None) => (icons::AGENT, tab.color),
        _ => (icons::TERMINAL, tab.color),
    }
}

pub fn status(tab: &RemoteTab, time: u128, theme: &Theme) -> (SharedString, Hsla) {
    if !tab.running {
        (if tab.failed { "failed" } else { "exited" }.into(), if tab.failed { theme.red } else { theme.faint })
    } else {
        match group(tab) {
            Group::Waiting => ("waiting".into(), theme.yellow),
            Group::Working => (ui::spinner(time).into(), theme.accent),
            Group::Done => ("done".into(), theme.green),
            Group::Idle => ("".into(), theme.faint),
        }
    }
}

pub fn project_name(catalog: &Catalog, path: &str) -> String {
    catalog.projects.iter().find(|project| project.path == path).map(|project| project.name.clone()).unwrap_or_default()
}

impl Home {
    fn tab_row(
        &self,
        key: Key,
        tab: &RemoteTab,
        detail: String,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let time = self.shown.elapsed().as_millis();
        let (glyph, color) = glyph(tab);
        let (state, state_color) = status(tab, time, theme);
        let tab_id = tab.id.clone();
        ui::row(SharedString::from(format!("tab:{}:{}", wire::encode_key(&key), tab.id)), theme)
            .child(
                div()
                    .flex_none()
                    .w(px(18.0))
                    .text_color(color.map_or(theme.muted, |color| color.color(theme.dark)))
                    .child(glyph),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(div().truncate().child(tab.title.clone()))
                    .when(!detail.is_empty(), |this| {
                        this.child(div().truncate().text_size(px(ui::SMALL)).text_color(theme.faint).child(detail))
                    }),
            )
            .child(div().flex_none().text_size(px(ui::SMALL)).text_color(state_color).child(state))
            .on_click(cx.listener(move |home, _, _, cx| home.open(key, tab_id.clone(), cx)))
    }

    pub fn main(&mut self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let header = ui::bar(theme)
            .child(ui::title("nucli").text_color(theme.accent))
            .child(
                ui::button("add", icons::ADD, theme)
                    .on_click(cx.listener(|home, _, window, cx| home.go(Screen::Pair, window, cx))),
            )
            .child(
                ui::button("settings", icons::SETTINGS, theme)
                    .on_click(cx.listener(|home, _, window, cx| home.go(Screen::Settings, window, cx))),
            );
        let [agents, computers] = ui::tabs(
            [(icons::AGENT, "agents"), (icons::os("macos"), "computers")],
            if self.board == Board::Agents { 0 } else { 1 },
            theme,
        );
        let tabs = div()
            .flex()
            .flex_none()
            .gap(px(4.0))
            .px(px(12.0))
            .py(px(8.0))
            .border_b_1()
            .border_color(theme.border)
            .child(agents.on_click(cx.listener(|home, _, _, cx| {
                home.board = Board::Agents;
                cx.notify();
            })))
            .child(computers.on_click(cx.listener(|home, _, _, cx| {
                home.board = Board::Computers;
                cx.notify();
            })));
        let content = if self.link.read(cx).devices.is_empty() {
            self.empty(theme, cx)
        } else if self.board == Board::Agents {
            self.agents(theme, cx)
        } else {
            self.computers(theme, cx)
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(header)
            .child(tabs)
            .child(div().id("board").flex_1().min_h_0().overflow_y_scroll().child(content))
            .into_any_element()
    }

    fn empty(&self, theme: &Theme, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .px(px(16.0))
            .py(px(32.0))
            .child(div().text_color(theme.muted).child("No computers yet."))
            .child(
                div()
                    .text_size(px(ui::SMALL))
                    .text_color(theme.faint)
                    .child("Pair a computer running Nucli to see its agents and terminals here."),
            )
            .child(
                ui::primary("empty-add", "[add a computer]", theme)
                    .on_click(cx.listener(|home, _, window, cx| home.go(Screen::Pair, window, cx))),
            )
    }

    fn agents(&self, theme: &Theme, cx: &mut Context<Self>) -> Div {
        let link = self.link.read(cx);
        let mut found: Vec<(Key, RemoteTab, String)> = Vec::new();
        for (key, online) in &link.online {
            let device = link.device(key).map(|device| device.name.clone()).unwrap_or_default();
            for tab in online.catalog.tabs.iter().filter(|tab| tab.kind == Kind::Agent) {
                let project = project_name(&online.catalog, &tab.project);
                let detail = if project.is_empty() { device.clone() } else { format!("{device} · {project}") };
                found.push((*key, tab.clone(), detail));
            }
        }
        found.sort_by(|(_, a, a_detail), (_, b, b_detail)| (a_detail, &a.title).cmp(&(b_detail, &b.title)));
        let online = link.online.len();
        let mut list = div().flex().flex_col();
        if found.is_empty() {
            let text = if online == 0 { "No computer is online." } else { "No agents are running on your computers." };
            return list.child(ui::hint(text, theme).py(px(24.0)));
        }
        for (current, title) in GROUPS {
            let rows: Vec<&(Key, RemoteTab, String)> =
                found.iter().filter(|(_, tab, _)| group(tab) == current).collect();
            if rows.is_empty() {
                continue;
            }
            list = list.child(ui::section(format!("{title} {}", rows.len()), theme));
            for (key, tab, detail) in rows {
                list = list.child(self.tab_row(*key, tab, detail.clone(), theme, cx));
            }
        }
        list
    }

    fn computers(&self, theme: &Theme, cx: &mut Context<Self>) -> Div {
        let link = self.link.read(cx);
        let mut list = div().flex().flex_col();
        let devices: Vec<_> = link
            .devices
            .iter()
            .filter_map(|device| {
                let key = device.public()?;
                let catalog = link.online.get(&key).map(|online| online.catalog.clone());
                let error = link.device_error(&key).map(String::from);
                Some((key, device.clone(), catalog, error))
            })
            .collect();
        for (key, device, catalog, error) in devices {
            let (state, color) = match &catalog {
                Some(_) => ("online".to_string(), theme.green),
                None => (error.unwrap_or_else(|| "offline".into()), theme.faint),
            };
            list = list.child(
                ui::row(SharedString::from(format!("device:{}", device.key)), theme)
                    .bg(theme.surface)
                    .child(div().flex_none().w(px(18.0)).text_color(theme.accent).child(icons::os(&device.os)))
                    .child(div().flex_1().min_w_0().truncate().child(device.name.clone()))
                    .child(div().flex_none().text_size(px(ui::SMALL)).text_color(color).child(state))
                    .child(div().flex_none().text_color(theme.faint).child(icons::CHEVRON_RIGHT))
                    .on_click(cx.listener(move |home, _, window, cx| home.go(Screen::Device(key), window, cx))),
            );
            let Some(catalog) = &catalog else { continue };
            for project in catalog.projects.iter().filter(|project| !project.name.is_empty()) {
                let path = project.path.clone();
                list = list.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .pl(px(16.0))
                        .pr(px(6.0))
                        .pt(px(10.0))
                        .text_size(px(ui::SMALL))
                        .text_color(project.color.map_or(theme.muted, |color| color.color(theme.dark)))
                        .child(icons::FOLDER)
                        .child(div().flex_1().min_w_0().truncate().child(project.name.clone()))
                        .when(catalog.control, |this| {
                            this.child(
                                ui::button(
                                    SharedString::from(format!("new:{}:{}", device.key, project.path)),
                                    icons::ADD,
                                    theme,
                                )
                                .on_click(cx.listener(move |home, _, window, cx| {
                                    home.go(Screen::Launch { key, project: path.clone() }, window, cx)
                                })),
                            )
                        }),
                );
                let tabs: Vec<&RemoteTab> = catalog.tabs.iter().filter(|tab| tab.project == project.path).collect();
                if tabs.is_empty() {
                    list = list.child(ui::hint("no tabs", theme).pl(px(44.0)));
                }
                for tab in tabs {
                    list = list.child(self.tab_row(key, tab, String::new(), theme, cx).pl(px(28.0)));
                }
            }
            let loose: Vec<&RemoteTab> = catalog
                .tabs
                .iter()
                .filter(|tab| {
                    !catalog.projects.iter().any(|project| project.path == tab.project && !project.name.is_empty())
                })
                .collect();
            if !loose.is_empty() {
                list = list.child(ui::section("no project", theme));
                for tab in loose {
                    list = list.child(self.tab_row(key, tab, String::new(), theme, cx).pl(px(28.0)));
                }
            }
            if catalog.tabs.is_empty() && catalog.projects.is_empty() {
                list = list.child(ui::hint("Nothing shared yet.", theme));
            }
        }
        list
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_agents_by_what_they_need() {
        let tab = |activity, attention, running| RemoteTab { activity, attention, running, ..Default::default() };
        assert!(group(&tab(Activity::Waiting, true, true)) == Group::Waiting);
        assert!(group(&tab(Activity::Working, true, true)) == Group::Working);
        assert!(group(&tab(Activity::Idle, true, true)) == Group::Done);
        assert!(group(&tab(Activity::Waiting, false, false)) == Group::Idle);
        assert!(group(&tab(Activity::Idle, false, true)) == Group::Idle);
    }
}
