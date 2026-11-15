use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Context, Div, InteractiveElement, IntoElement, ParentElement, SharedString, StatefulInteractiveElement,
    Styled, div, px,
};
use nucli_core::icons;
use nucli_core::session::Kind;
use nucli_core::theme::Theme;
use nucli_core::uplink::protocol::Response;
use nucli_core::uplink::wire::Key;

use crate::board::{Group, group};
use crate::device::age;
use crate::home::{Home, Screen, Section};
use crate::ui;

pub const SECTIONS: [(&str, &str); 3] =
    [(icons::FOLDER, "projects"), (icons::AGENT, "agents"), (icons::HISTORY, "history")];

pub fn section_index(section: Section) -> usize {
    match section {
        Section::Projects => 0,
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
            let section = [Section::Projects, Section::Agents, Section::History][index];
            tabs = tabs.child(tab.on_click(cx.listener(move |home, _, _, cx| home.set_section(section, cx))));
        }
        let body = match self.section {
            Section::Projects => self.projects_of(key, theme, cx),
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
            .child(div().id("computer").flex_1().min_h_0().overflow_y_scroll().child(body))
            .into_any_element()
    }

    fn projects_of(&self, key: Key, theme: &Theme, cx: &mut Context<Self>) -> Div {
        let link = self.link.read(cx);
        let control = link.control(&key);
        let Some(online) = link.online.get(&key) else {
            return div().child(ui::hint("The computer is offline.", theme).py(px(24.0)));
        };
        let catalog = online.catalog.clone();
        let mut list = div().flex().flex_col();
        let mut workspace = None;
        let several =
            catalog.projects.iter().map(|project| &project.workspace).collect::<std::collections::HashSet<_>>().len()
                > 1;
        for project in &catalog.projects {
            if several && workspace != Some(&project.workspace) {
                workspace = Some(&project.workspace);
                list = list.child(ui::section(project.workspace.clone(), theme));
            }
            let tabs: Vec<_> = catalog.tabs.iter().filter(|tab| tab.project == project.path).collect();
            let agents = tabs.iter().filter(|tab| tab.kind == Kind::Agent).count();
            let waiting = tabs.iter().filter(|tab| group(tab) == Group::Waiting).count();
            let working = tabs.iter().filter(|tab| group(tab) == Group::Working).count();
            let scratch = project.name.is_empty();
            let mut parts = Vec::new();
            if agents > 0 {
                parts.push(ui::count(agents, "agent"));
            }
            if tabs.len() > agents {
                parts.push(ui::count(tabs.len() - agents, "shell"));
            }
            if parts.is_empty() {
                parts.push("nothing running".into());
            }
            let path = project.path.clone();
            let color = project.color.map_or(theme.accent, |color| color.color(theme.dark));
            list = list.child(
                ui::row(SharedString::from(format!("project:{}", project.path)), theme)
                    .child(div().flex_none().w(px(18.0)).text_color(color).child(if scratch {
                        icons::HOME
                    } else {
                        icons::FOLDER
                    }))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(div().truncate().child(if scratch {
                                "home".to_string()
                            } else {
                                project.name.clone()
                            }))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(ui::SMALL))
                                    .text_color(theme.faint)
                                    .child(parts.join(" · ")),
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
                    .when(waiting == 0 && working > 0, |this| {
                        this.child(
                            div()
                                .flex_none()
                                .text_size(px(ui::SMALL))
                                .text_color(theme.accent)
                                .child(ui::spinner(self.shown.elapsed().as_millis())),
                        )
                    })
                    .child(div().flex_none().text_color(theme.faint).child(icons::CHEVRON_RIGHT))
                    .on_click(cx.listener(move |home, _, window, cx| {
                        home.go(Screen::Project { key, project: path.clone() }, window, cx)
                    })),
            );
        }
        if catalog.projects.is_empty() {
            list = list.child(ui::hint("This computer shares no projects.", theme).py(px(16.0)));
        }
        if control {
            list = list.child(
                ui::row("add-project", theme)
                    .child(div().flex_none().w(px(18.0)).text_color(theme.accent).child(icons::ADD))
                    .child(div().text_color(theme.accent).child("add project"))
                    .on_click(cx.listener(move |home, _, window, cx| home.go(Screen::Folders(key), window, cx))),
            );
        }
        list
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
                    }),
            );
        }
        list
    }
}
