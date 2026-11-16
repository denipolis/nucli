use gpui::prelude::FluentBuilder;
use gpui::{
    AnimationExt, AnyElement, Context, Div, IntoElement, ParentElement, SharedString, StatefulInteractiveElement,
    Styled, div, px,
};
use nucli_core::icons;
use nucli_core::theme::Theme;
use nucli_core::uplink::protocol::Response;
use nucli_core::uplink::wire::Key;

use crate::device::age;
use crate::home::{Home, Screen, Section};
use crate::ui;

pub const SECTIONS: [(&str, &str); 3] =
    [(icons::WINDOW, "workspaces"), (icons::AGENT, "agents"), (icons::HISTORY, "history")];

pub fn section_index(section: Section) -> usize {
    match section {
        Section::Workspaces => 0,
        Section::Agents => 1,
        Section::History => 2,
    }
}

impl Home {
    pub fn back_bar(&self, title: impl Into<SharedString>, theme: &Theme, cx: &mut Context<Self>) -> Div {
        ui::bar(theme)
            .child(
                ui::button("back", icons::CHEVRON_LEFT, theme)
                    .on_click(cx.listener(|home, _, window, cx| home.back(window, cx))),
            )
            .child(ui::title(title))
    }

    pub fn loading(&self, theme: &Theme) -> Div {
        div()
            .flex()
            .gap(px(8.0))
            .px(px(16.0))
            .py(px(24.0))
            .text_color(theme.muted)
            .child(div().text_color(theme.accent).child(ui::spinner(self.shown.elapsed().as_millis())))
            .child("loading…")
    }

    pub fn computer(&mut self, key: Key, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let link = self.link.read(cx);
        let name = link.device(&key).map(|device| device.name.clone()).unwrap_or_default();
        let header = self.back_bar(name, theme, cx).child(
            ui::button("details", icons::DEVICE, theme)
                .on_click(cx.listener(move |home, _, window, cx| home.go(Screen::Device(key), window, cx))),
        );
        let mut tabs =
            div().flex().flex_none().gap(px(2.0)).px(px(10.0)).py(px(6.0)).border_b_1().border_color(theme.border);
        for (index, tab) in ui::tabs(SECTIONS, section_index(self.section), theme).into_iter().enumerate() {
            let section = [Section::Workspaces, Section::Agents, Section::History][index];
            tabs = tabs.child(tab.on_click(cx.listener(move |home, _, _, cx| home.set_section(section, cx))));
        }
        let body = match self.section {
            Section::Workspaces => self.workspaces_of(key, theme, cx),
            Section::Agents => self.agents_of(key, theme, cx),
            Section::History => self.history_of(key, theme, cx),
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(header)
            .child(tabs)
            .child(ui::scroll("computer", &self.scroll).child(body.with_animation(
                ("section", section_index(self.section)),
                ui::motion(),
                |this, delta| this.opacity(delta),
            )))
            .into_any_element()
    }

    fn history_of(&self, key: Key, theme: &Theme, cx: &mut Context<Self>) -> Div {
        let control = self.link.read(cx).control(&key);
        let catalog = self.link.read(cx).online.get(&key).map(|online| online.catalog.clone()).unwrap_or_default();
        let conversations = match &self.conversations {
            None => return self.loading(theme),
            Some(Response::Failed { reason }) => return div().child(ui::hint(reason.clone(), theme).py(px(24.0))),
            Some(Response::History { conversations }) => conversations.clone(),
            Some(_) => Vec::new(),
        };
        let mut list = div().flex().flex_col();
        if conversations.is_empty() {
            return list.child(ui::hint("No conversations in shared projects.", theme).py(px(24.0)));
        }
        if let Some(notice) = &self.notice {
            list = list.child(ui::hint(notice.clone(), theme).text_color(theme.yellow));
        }
        for item in conversations {
            let (glyph, color) = icons::agent(&item.agent);
            let project = crate::board::project_name(&catalog, &item.project);
            let detail = if item.profile.is_empty() {
                format!("{project} · {}", age(item.modified))
            } else {
                format!("{project} · {} · {}", item.profile, age(item.modified))
            };
            let (id, path) = (item.id.clone(), item.project.clone());
            let held = item.clone();
            list = list.child(
                ui::row(SharedString::from(format!("conversation:{}", item.id)), theme)
                    .child(div().flex_none().w(px(18.0)).text_color(color.color(theme.dark)).child(glyph))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(div().truncate().child(if item.title.is_empty() {
                                "untitled".to_string()
                            } else {
                                item.title.clone()
                            }))
                            .child(div().truncate().text_size(px(ui::SMALL)).text_color(theme.faint).child(detail)),
                    )
                    .when(control, |this| {
                        this.child(div().flex_none().text_size(px(ui::SMALL)).text_color(theme.accent).child("resume"))
                            .on_click(cx.listener(move |home, _, window, cx| {
                                home.resume(key, id.clone(), path.clone(), window, cx)
                            }))
                    })
                    .child(ui::hold(cx.listener(move |home, _, _, cx| {
                        let sheet = home.conversation_menu(key, &held, control);
                        home.show(sheet, cx)
                    }))),
            );
        }
        list
    }
}
