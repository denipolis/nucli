use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Context, Div, InteractiveElement, IntoElement, ParentElement, SharedString, StatefulInteractiveElement,
    Styled, div, px, relative,
};
use nucli_core::icons;
use nucli_core::session::Kind;
use nucli_core::theme::Theme;
use nucli_core::uplink::protocol::{RemoteTab, Response};
use nucli_core::uplink::wire::Key;

use crate::board::project_name;
use crate::device::age;
use crate::home::{Drawer, Home, Panel, Screen};
use crate::ui;

const PANELS: [(&str, &str); 3] = [(icons::FILES, "files"), (icons::CHANGES, "changes"), (icons::HISTORY, "history")];

fn panel_index(panel: Panel) -> usize {
    match panel {
        Panel::Files => 0,
        Panel::Changes => 1,
        Panel::Log => 2,
    }
}

pub fn status_color(status: &str, theme: &Theme) -> gpui::Hsla {
    match status {
        "M" => theme.yellow,
        "D" => theme.red,
        _ => theme.green,
    }
}

impl Home {
    pub fn project(&mut self, key: Key, project: &str, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let time = self.shown.elapsed().as_millis();
        let link = self.link.read(cx);
        let control = link.control(&key);
        let starting = link.is_opening(key) && time < 15000;
        let catalog = link.online.get(&key).map(|online| online.catalog.clone());
        let offline = catalog.is_none();
        let catalog = catalog.unwrap_or_default();
        let scratch = catalog.projects.iter().any(|item| item.path == project && item.name.is_empty());
        let name = if scratch { "home".to_string() } else { project_name(&catalog, project) };
        let mut header = self.back_bar(name, theme, cx);
        if !scratch {
            for (index, (glyph, label)) in PANELS.into_iter().enumerate() {
                let panel = [Panel::Files, Panel::Changes, Panel::Log][index];
                let path = project.to_string();
                header =
                    header.child(ui::button(label, glyph, theme).on_click(cx.listener(move |home, _, _, cx| {
                        home.open_drawer(key, path.clone(), panel, String::new(), cx)
                    })));
            }
        }
        let tabs: Vec<RemoteTab> = catalog.tabs.iter().filter(|tab| tab.project == project).cloned().collect();
        let mut list = div().flex().flex_col().child(ui::hint(project.to_string(), theme).text_color(theme.faint));
        if offline {
            list = list.child(ui::hint("The computer is offline.", theme));
        }
        if let Some(notice) = &self.notice {
            list = list.child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .px(px(16.0))
                    .py(px(8.0))
                    .text_color(theme.yellow)
                    .when(notice.ends_with('…'), |this| this.text_color(theme.accent).child(ui::spinner(time)))
                    .child(notice.clone()),
            );
        }
        for (kind, title) in [(Kind::Agent, "agents"), (Kind::Shell, "shells")] {
            let rows: Vec<&RemoteTab> =
                tabs.iter().filter(|tab| (tab.kind == Kind::Agent) == (kind == Kind::Agent)).collect();
            if rows.is_empty() {
                continue;
            }
            list = list.child(ui::section(format!("{title} {}", rows.len()), theme));
            for tab in rows {
                let detail =
                    [tab.branch.clone(), tab.notice.clone()].into_iter().flatten().collect::<Vec<_>>().join(" · ");
                list = list.child(self.tab_row(key, tab, detail, theme, cx));
            }
        }
        if tabs.is_empty() && !offline {
            list = list.child(ui::hint("Nothing is running in this project.", theme).py(px(16.0)));
        }
        let actions = div()
            .flex()
            .flex_none()
            .gap(px(8.0))
            .px(px(16.0))
            .py(px(10.0))
            .border_t_1()
            .border_color(theme.border)
            .when(starting, |this| {
                this.child(
                    div()
                        .flex()
                        .flex_1()
                        .items_center()
                        .gap(px(8.0))
                        .h(px(ui::ROW))
                        .text_color(theme.accent)
                        .child(ui::spinner(time))
                        .child("starting…"),
                )
            })
            .when(!starting, |this| {
                this.child(ui::primary("new-agent", format!("{} new agent", icons::ADD), theme).flex_1().on_click(
                    cx.listener(|home, _, _, cx| {
                        home.sheet = Some(true);
                        cx.notify();
                    }),
                ))
                .child(
                    ui::secondary("new-shell", format!("{} new shell", icons::TERMINAL), theme).flex_1().on_click(
                        cx.listener(|home, _, _, cx| {
                            home.sheet = Some(false);
                            cx.notify();
                        }),
                    ),
                )
            });
        let sheet = self.sheet.map(|agents| self.launchers(key, project, agents, theme, cx));
        let drawer = self.drawer.as_ref().map(|drawer| self.drawer_view(key, project, drawer, theme, cx));
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .relative()
            .child(header)
            .child(div().id("project").flex_1().min_h_0().overflow_y_scroll().child(list))
            .when(control && !offline, |this| this.child(actions))
            .children(sheet)
            .children(drawer)
            .into_any_element()
    }

    fn launchers(&self, key: Key, project: &str, agents: bool, theme: &Theme, cx: &mut Context<Self>) -> Div {
        let catalog = self.link.read(cx).online.get(&key).map(|online| online.catalog.clone()).unwrap_or_default();
        let mut sheet = div()
            .flex()
            .flex_col()
            .bg(theme.background)
            .border_t_1()
            .border_color(theme.border)
            .child(ui::section(if agents { "new agent" } else { "new shell" }, theme));
        for launcher in catalog.launchers.iter().filter(|launcher| launcher.agent == agents) {
            let (glyph, color) = if agents {
                let (glyph, color) = icons::agent(&launcher.command);
                (glyph, color.color(theme.dark))
            } else {
                (icons::TERMINAL, theme.muted)
            };
            let (project, name) = (project.to_string(), launcher.name.clone());
            sheet = sheet.child(
                ui::row(SharedString::from(format!("launcher:{}", launcher.name)), theme)
                    .child(div().flex_none().w(px(18.0)).text_color(color).child(glyph))
                    .child(div().flex_1().min_w_0().truncate().child(launcher.name.clone()))
                    .child(
                        div()
                            .text_size(px(ui::SMALL))
                            .text_color(theme.faint)
                            .truncate()
                            .child(launcher.command.clone()),
                    )
                    .on_click(cx.listener(move |home, _, _, cx| home.launch(key, &project, &name, cx))),
            );
        }
        sheet =
            sheet.child(ui::row("cancel-launch", theme).child(div().text_color(theme.muted).child("cancel")).on_click(
                cx.listener(|home, _, _, cx| {
                    home.sheet = None;
                    cx.notify();
                }),
            ));
        div()
            .absolute()
            .inset_0()
            .flex()
            .flex_col()
            .child(div().id("sheet-backdrop").flex_1().on_click(cx.listener(|home, _, _, cx| {
                home.sheet = None;
                cx.notify();
            })))
            .child(sheet)
    }

    fn drawer_view(&self, key: Key, project: &str, drawer: &Drawer, theme: &Theme, cx: &mut Context<Self>) -> Div {
        let mut tabs =
            div().flex().flex_none().gap(px(2.0)).px(px(10.0)).py(px(6.0)).border_b_1().border_color(theme.border);
        for (index, tab) in ui::tabs(PANELS, panel_index(drawer.panel), theme).into_iter().enumerate() {
            let panel = [Panel::Files, Panel::Changes, Panel::Log][index];
            let path = project.to_string();
            tabs = tabs.child(tab.on_click(
                cx.listener(move |home, _, _, cx| home.open_drawer(key, path.clone(), panel, String::new(), cx)),
            ));
        }
        let header = ui::bar(theme).child(tabs.border_b_0().px(px(0.0)).flex_1()).child(
            ui::button("close-drawer", icons::CLOSE, theme).on_click(cx.listener(|home, _, _, cx| {
                home.drawer = None;
                cx.notify();
            })),
        );
        let body = match &drawer.data {
            None => self.loading(theme),
            Some(Response::Failed { reason }) => div().child(ui::hint(reason.clone(), theme).py(px(24.0))),
            Some(Response::Files { entries }) => {
                let mut list = div().flex().flex_col();
                if !drawer.folder.is_empty() {
                    let parent =
                        drawer.folder.rsplit_once('/').map(|(parent, _)| parent.to_string()).unwrap_or_default();
                    let path = project.to_string();
                    list = list.child(
                        ui::row("folder-up", theme)
                            .child(div().flex_none().w(px(18.0)).text_color(theme.muted).child(icons::CHEVRON_UP))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_color(theme.muted)
                                    .child(drawer.folder.clone()),
                            )
                            .on_click(cx.listener(move |home, _, _, cx| {
                                home.open_drawer(key, path.clone(), Panel::Files, parent.clone(), cx)
                            })),
                    );
                }
                if entries.is_empty() {
                    list = list.child(ui::hint("This folder is empty.", theme));
                }
                for entry in entries {
                    let path = if drawer.folder.is_empty() {
                        entry.name.clone()
                    } else {
                        format!("{}/{}", drawer.folder, entry.name)
                    };
                    let (glyph, color) = if entry.folder {
                        (icons::FOLDER, theme.accent)
                    } else {
                        let (glyph, color) = icons::file(&entry.name);
                        (glyph, color.color(theme.dark))
                    };
                    let (folder, root) = (entry.folder, project.to_string());
                    list = list.child(
                        ui::row(SharedString::from(format!("entry:{path}")), theme)
                            .child(div().flex_none().w(px(18.0)).text_color(color).child(glyph))
                            .child(div().flex_1().min_w_0().truncate().child(entry.name.clone()))
                            .when(folder, |this| {
                                this.child(div().flex_none().text_color(theme.faint).child(icons::CHEVRON_RIGHT))
                            })
                            .on_click(cx.listener(move |home, _, window, cx| {
                                if folder {
                                    home.open_drawer(key, root.clone(), Panel::Files, path.clone(), cx)
                                } else {
                                    home.go(Screen::File { key, project: root.clone(), path: path.clone() }, window, cx)
                                }
                            })),
                    );
                }
                list
            }
            Some(Response::Changes { branch, changes }) => {
                let mut list = div().flex().flex_col().child(
                    div()
                        .flex()
                        .gap(px(8.0))
                        .px(px(16.0))
                        .py(px(10.0))
                        .border_b_1()
                        .border_color(theme.border)
                        .child(div().text_color(theme.accent).child(icons::BRANCH))
                        .child(div().truncate().child(branch.clone())),
                );
                if changes.is_empty() {
                    list = list.child(ui::hint("No changes.", theme).py(px(16.0)));
                }
                for (staged, title) in [(true, "staged"), (false, "changes")] {
                    let rows: Vec<_> = changes.iter().filter(|change| change.staged == staged).collect();
                    if rows.is_empty() {
                        continue;
                    }
                    list = list.child(ui::section(format!("{title} {}", rows.len()), theme));
                    for change in rows {
                        list = list.child(self.change_row(key, project, change, None, theme, cx));
                    }
                }
                list
            }
            Some(Response::Log { commits }) => {
                let mut list = div().flex().flex_col();
                if commits.is_empty() {
                    list = list.child(ui::hint("No commits.", theme).py(px(16.0)));
                }
                for commit in commits {
                    let short: String = commit.hash.chars().take(7).collect();
                    let (hash, root) = (commit.hash.clone(), project.to_string());
                    let refs = commit.refs.join(", ");
                    list = list.child(
                        ui::row(SharedString::from(format!("commit:{}", commit.hash)), theme)
                            .child(div().flex_none().w(px(18.0)).text_color(theme.faint).child(icons::COMMIT))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .min_w_0()
                                    .when(!refs.is_empty(), |this| {
                                        this.child(
                                            div()
                                                .truncate()
                                                .text_size(px(ui::SMALL))
                                                .text_color(theme.accent)
                                                .child(refs),
                                        )
                                    })
                                    .child(div().truncate().child(commit.subject.clone()))
                                    .child(div().truncate().text_size(px(ui::SMALL)).text_color(theme.faint).child(
                                        format!("{short} · {} · {}", commit.author, age(commit.time.max(0) as u64)),
                                    )),
                            )
                            .on_click(cx.listener(move |home, _, window, cx| {
                                home.go(Screen::Commit { key, project: root.clone(), hash: hash.clone() }, window, cx)
                            })),
                    );
                }
                list
            }
            Some(_) => div(),
        };
        div()
            .absolute()
            .inset_0()
            .flex()
            .child(div().id("drawer-backdrop").flex_1().on_click(cx.listener(|home, _, _, cx| {
                home.drawer = None;
                cx.notify();
            })))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w(relative(0.88))
                    .h_full()
                    .bg(theme.background)
                    .border_l_1()
                    .border_color(theme.border)
                    .child(header)
                    .child(div().id("drawer").flex_1().min_h_0().overflow_y_scroll().child(body)),
            )
    }

    pub fn change_row(
        &self,
        key: Key,
        project: &str,
        change: &nucli_core::uplink::protocol::RemoteChange,
        commit: Option<String>,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let (path, root) = (change.path.clone(), project.to_string());
        let name = change.path.rsplit('/').next().unwrap_or(&change.path).to_string();
        let folder = change.path.strip_suffix(&name).unwrap_or("").trim_end_matches('/').to_string();
        ui::row(SharedString::from(format!("change:{}:{}:{}", change.staged, change.path, commit.is_some())), theme)
            .child(
                div()
                    .flex_none()
                    .w(px(18.0))
                    .text_color(status_color(&change.status, theme))
                    .child(change.status.clone()),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(div().truncate().child(name))
                    .when(!folder.is_empty(), |this| {
                        this.child(div().truncate().text_size(px(ui::SMALL)).text_color(theme.faint).child(folder))
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .gap(px(6.0))
                    .text_size(px(ui::SMALL))
                    .when(change.added > 0, |this| {
                        this.child(div().text_color(theme.green).child(format!("+{}", change.added)))
                    })
                    .when(change.removed > 0, |this| {
                        this.child(div().text_color(theme.red).child(format!("-{}", change.removed)))
                    }),
            )
            .on_click(cx.listener(move |home, _, window, cx| {
                home.go(
                    Screen::Diff { key, project: root.clone(), path: path.clone(), commit: commit.clone() },
                    window,
                    cx,
                )
            }))
    }
}
