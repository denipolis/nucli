use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Context, IntoElement, ParentElement, SharedString, StatefulInteractiveElement, Styled, div, px,
};
use nucli_core::icons;
use nucli_core::session::Kind;
use nucli_core::theme::Theme;

use crate::board::{Group, group};
use crate::device::age;
use crate::home::{Home, Screen};
use crate::ui;

impl Home {
    pub fn computers(&mut self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let link = self.link.read(cx);
        let rows: Vec<_> = link
            .devices
            .iter()
            .filter_map(|device| {
                let key = device.public()?;
                let summary = match link.online.get(&key) {
                    Some(online) => {
                        let agents = online.catalog.tabs.iter().filter(|tab| tab.kind == Kind::Agent);
                        let count = |wanted: Group| agents.clone().filter(|tab| group(tab) == wanted).count();
                        let (waiting, working) = (count(Group::Waiting), count(Group::Working));
                        let total = agents.clone().count();
                        let projects =
                            online.catalog.projects.iter().filter(|project| !project.name.is_empty()).count();
                        let mut parts = vec![ui::count(projects, "project")];
                        parts.push(ui::count(total, "agent"));
                        if working > 0 {
                            parts.push(format!("{working} working"));
                        }
                        (parts.join(" · "), waiting, true)
                    }
                    None => (
                        format!("{} · seen {}", link.device_error(&key).unwrap_or("offline"), age(device.seen)),
                        0,
                        false,
                    ),
                };
                Some((key, device.name.clone(), device.os.clone(), summary))
            })
            .collect();
        let header = ui::bar(theme).child(ui::title("computers")).child(
            ui::button("settings", icons::SETTINGS, theme)
                .on_click(cx.listener(|home, _, window, cx| home.go(Screen::Settings, window, cx))),
        );
        let mut list = div().flex().flex_col();
        if rows.is_empty() {
            list = list.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .px(px(16.0))
                    .py(px(24.0))
                    .child(div().text_color(theme.muted).child("No computers yet."))
                    .child(
                        div()
                            .text_size(px(ui::SMALL))
                            .text_color(theme.faint)
                            .child("Pair a computer running Nucli to see its projects, agents and history here."),
                    ),
            );
        }
        for (key, name, os, (summary, waiting, online)) in rows {
            list = list.child(
                ui::row(SharedString::from(format!("computer:{name}:{}", key[0])), theme)
                    .child(
                        div()
                            .flex_none()
                            .w(px(20.0))
                            .text_color(if online { theme.accent } else { theme.faint })
                            .child(icons::os(&os)),
                    )
                    .child(
                        div().flex().flex_col().flex_1().min_w_0().child(div().truncate().child(name)).child(
                            div()
                                .truncate()
                                .text_size(px(ui::SMALL))
                                .text_color(if online { theme.muted } else { theme.faint })
                                .child(summary),
                        ),
                    )
                    .when(waiting > 0, |this| {
                        this.child(
                            div()
                                .flex_none()
                                .text_size(px(ui::SMALL))
                                .text_color(theme.yellow)
                                .child(format!("{waiting} waiting")),
                        )
                    })
                    .when(online && waiting == 0, |this| {
                        this.child(div().flex_none().text_size(px(ui::SMALL)).text_color(theme.green).child("online"))
                    })
                    .child(div().flex_none().text_color(theme.faint).child(icons::CHEVRON_RIGHT))
                    .on_click(cx.listener(move |home, _, window, cx| home.go(Screen::Computer(key), window, cx))),
            );
        }
        list = list.child(
            ui::row("add-computer", theme)
                .child(div().flex_none().w(px(20.0)).text_color(theme.accent).child(icons::ADD))
                .child(div().text_color(theme.accent).child("add computer"))
                .on_click(cx.listener(|home, _, window, cx| home.go(Screen::Pair, window, cx))),
        );
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(header)
            .child(ui::scroll("computers", &self.scroll).child(list))
            .into_any_element()
    }
}
