use gpui::prelude::FluentBuilder;
use gpui::{Context, Div, Hsla, ParentElement, SharedString, Stateful, StatefulInteractiveElement, Styled, div, px};
use nucli_core::icons;
use nucli_core::session::{Activity, Kind};
use nucli_core::theme::{ItemColor, Theme};
use nucli_core::uplink::protocol::{Catalog, RemoteTab};
use nucli_core::uplink::wire::{self, Key};

use crate::home::Home;
use crate::ui;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Group {
    Waiting,
    Done,
    Working,
    Idle,
}

pub const GROUPS: [(Group, &str); 4] =
    [(Group::Waiting, "waiting"), (Group::Done, "done"), (Group::Working, "working"), (Group::Idle, "idle")];

pub fn group(tab: &RemoteTab) -> Group {
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

pub fn glyph(tab: &RemoteTab) -> (&'static str, Option<ItemColor>) {
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
    catalog
        .projects
        .iter()
        .find(|project| project.path == path)
        .map(|project| project.name.clone())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.rsplit(['/', '\\']).next().unwrap_or(path).to_string())
}

impl Home {
    pub fn tab_row(
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

    pub fn agents_of(&self, key: Key, theme: &Theme, cx: &mut Context<Self>) -> Div {
        let link = self.link.read(cx);
        let Some(online) = link.online.get(&key) else {
            return div().child(ui::hint("The computer is offline.", theme).py(px(24.0)));
        };
        let catalog = online.catalog.clone();
        let mut tabs: Vec<RemoteTab> = catalog.tabs.iter().filter(|tab| tab.kind == Kind::Agent).cloned().collect();
        tabs.sort_by(|a, b| {
            (project_name(&catalog, &a.project), &a.title).cmp(&(project_name(&catalog, &b.project), &b.title))
        });
        let mut list = div().flex().flex_col();
        if tabs.is_empty() {
            return list.child(ui::hint("No agents are running on this computer.", theme).py(px(24.0)));
        }
        for (current, title) in GROUPS {
            let rows: Vec<&RemoteTab> = tabs.iter().filter(|tab| group(tab) == current).collect();
            if rows.is_empty() {
                continue;
            }
            list = list.child(ui::section(format!("{title} {}", rows.len()), theme));
            for tab in rows {
                list = list.child(self.tab_row(key, tab, project_name(&catalog, &tab.project), theme, cx));
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
        assert_eq!(group(&tab(Activity::Waiting, true, true)), Group::Waiting);
        assert_eq!(group(&tab(Activity::Working, true, true)), Group::Working);
        assert_eq!(group(&tab(Activity::Idle, true, true)), Group::Done);
        assert_eq!(group(&tab(Activity::Waiting, false, false)), Group::Idle);
        assert_eq!(group(&tab(Activity::Idle, false, true)), Group::Idle);
    }
}
