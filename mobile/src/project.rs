use gpui::prelude::FluentBuilder;
use gpui::{
    AnimationExt, AnyElement, Context, Div, InteractiveElement, IntoElement, ParentElement, SharedString, Stateful,
    StatefulInteractiveElement, Styled, div, px, relative,
};
use nucli_core::icons;
use nucli_core::theme::Theme;
use nucli_core::uplink::protocol::{Action, Response};
use nucli_core::uplink::wire::Key;

use crate::board::project_name;
use crate::device::age;
use crate::home::{Drawer, Home, Panel, Screen};
use crate::menus::delete_entry;
use crate::sheet::Sheet;
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
        let link = self.link.read(cx);
        let control = link.control(&key);
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
                header = header.child(ui::button(SharedString::from(format!("open-{label}")), glyph, theme).on_click(
                    cx.listener(move |home, _, _, cx| home.open_drawer(key, path.clone(), panel, String::new(), cx)),
                ));
            }
        }
        let mut list = div().flex().flex_col().when(!scratch && !project.is_empty(), |this| {
            this.child(ui::hint(project.to_string(), theme).text_color(theme.faint))
        });
        if offline {
            list = list.child(ui::hint("The computer is offline.", theme));
        }
        list = list.children(self.notice_row(theme));
        if control && !offline {
            list = list.children(self.launch_rows(key, project, "", theme, cx));
        }
        for tab in catalog.tabs.iter().filter(|tab| tab.project == project) {
            let detail = [tab.branch.clone(), tab.notice.clone()].into_iter().flatten().collect::<Vec<_>>().join(" · ");
            list = list.child(self.tab_row(key, tab, detail, theme, cx));
        }
        let drawer = self.drawer.as_ref().map(|drawer| self.drawer_view(key, project, drawer, theme, cx));
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .relative()
            .child(header)
            .child(ui::scroll("project", &self.scroll).child(list))
            .children(drawer)
            .into_any_element()
    }

    pub fn notice_row(&self, theme: &Theme) -> Option<Div> {
        let notice = self.notice.as_ref()?;
        let time = self.shown.elapsed().as_millis();
        Some(
            div()
                .flex()
                .gap(px(8.0))
                .px(px(16.0))
                .py(px(8.0))
                .text_color(theme.yellow)
                .when(notice.ends_with('…'), |this| this.text_color(theme.accent).child(ui::spinner(time)))
                .child(notice.clone()),
        )
    }

    pub fn launch_rows(
        &self,
        key: Key,
        project: &str,
        workspace: &str,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> [Stateful<Div>; 2] {
        [(true, icons::ADD, "new agent"), (false, icons::TERMINAL, "new shell")].map(|(agent, glyph, label)| {
            let (path, space) = (project.to_string(), workspace.to_string());
            ui::row(label, theme)
                .text_color(theme.accent)
                .child(div().flex_none().w(px(18.0)).child(glyph))
                .child(label)
                .on_click(cx.listener({
                    let (path, space) = (path.clone(), space.clone());
                    move |home, _, _, cx| home.new_tab(key, &path, &space, agent, cx)
                }))
                .child(ui::hold(cx.listener(move |home, _, _, cx| {
                    let sheet = home.launcher_menu(key, &path, &space, agent, cx);
                    home.show(sheet, cx)
                })))
        })
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
        let control = self.link.read(cx).control(&key);
        let header = ui::bar(theme).child(tabs.border_b_0().px(px(0.0)).flex_1()).child(
            ui::button("close-drawer", icons::CLOSE, theme)
                .on_click(cx.listener(|home, _, window, cx| home.back(window, cx))),
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
                            .on_click(cx.listener({
                                let path = path.clone();
                                move |home, _, _, cx| {
                                    home.open_drawer(key, path.clone(), Panel::Files, parent.clone(), cx)
                                }
                            }))
                            .when(control, |this| {
                                let folder = drawer.folder.clone();
                                this.child(ui::hold(cx.listener(move |home, _, _, cx| {
                                    let sheet = home.folder_menu(key, &path, &folder);
                                    home.show(sheet, cx)
                                })))
                            }),
                    );
                }
                if control {
                    let (root, folder) = (project.to_string(), drawer.folder.clone());
                    list = list.child(
                        ui::row("folder-menu", theme)
                            .text_color(theme.accent)
                            .child(div().flex_none().w(px(18.0)).child(icons::ADD))
                            .child("new file or folder")
                            .on_click(cx.listener(move |home, _, _, cx| {
                                let sheet = home.folder_menu(key, &root, &folder);
                                home.show(sheet, cx)
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
                    let held = (root.clone(), path.clone());
                    let id = SharedString::from(format!("entry:{path}"));
                    let delete = delete_entry(key, &root, &path);
                    let row = ui::row(id.clone(), theme)
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
                        }))
                        .when(control, |this| {
                            this.child(ui::hold(cx.listener(move |home, _, _, cx| {
                                let sheet = home.entry_menu(key, &held.0, &held.1, folder);
                                home.show(sheet, cx)
                            })))
                        });
                    list = if control {
                        list.child(self.swipeable(id, row, "delete", delete, theme, cx))
                    } else {
                        list.child(row)
                    };
                }
                list
            }
            Some(Response::Changes { branch, changes }) => {
                let root = project.to_string();
                let mut list = div().flex().flex_col().child(
                    ui::row("branch", theme)
                        .child(div().flex_none().w(px(18.0)).text_color(theme.accent).child(icons::BRANCH))
                        .child(div().flex_1().min_w_0().truncate().child(branch.clone()))
                        .when(control, |this| {
                            this.child(div().flex_none().text_color(theme.faint).child(icons::CHEVRON_RIGHT))
                                .on_click(cx.listener(move |home, _, _, cx| home.branches(key, &root, cx)))
                        }),
                );
                if control {
                    list = list.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .px(px(16.0))
                            .py(px(12.0))
                            .border_b_1()
                            .border_color(theme.border)
                            .child(self.message.clone())
                            .child(
                                ui::primary("commit", format!("{} commit", icons::CHECK), theme)
                                    .on_click(cx.listener(|home, _, _, cx| home.commit_staged(cx))),
                            ),
                    );
                }
                if changes.is_empty() {
                    list = list.child(ui::hint("No changes.", theme).py(px(16.0)));
                }
                for (staged, title) in [(true, "staged"), (false, "changes")] {
                    let rows: Vec<_> = changes.iter().filter(|change| change.staged == staged).collect();
                    if rows.is_empty() {
                        continue;
                    }
                    let paths: Vec<String> = rows.iter().map(|change| change.path.clone()).collect();
                    let mut section = ui::section(format!("{title} {}", rows.len()), theme);
                    if control {
                        let root = project.to_string();
                        let (label, all) =
                            if staged { ("unstage all", paths.clone()) } else { ("stage all", paths.clone()) };
                        section = section.child(
                            div()
                                .id(SharedString::from(format!("all-{title}")))
                                .text_color(theme.accent)
                                .child(label)
                                .on_click(cx.listener(move |home, _, _, cx| {
                                    let paths = all.clone();
                                    let action =
                                        if staged { Action::Unstage { paths } } else { Action::Stage { paths } };
                                    home.act(key, &root, action, cx)
                                })),
                        );
                        if !staged {
                            let root = project.to_string();
                            let count = paths.len();
                            section = section.child(
                                div()
                                    .id("discard-all")
                                    .text_color(theme.red)
                                    .child("discard")
                                    .on_click(cx.listener(move |home, _, _, cx| {
                                        let (root, paths) = (root.clone(), paths.clone());
                                        home.show(
                                            Sheet::Confirm {
                                                title: "discard changes?".into(),
                                                detail: format!(
                                                    "Discard changes to {count} {}? This cannot be undone. New files go to the trash.",
                                                    if count == 1 { "file" } else { "files" }
                                                ),
                                                button: "discard".into(),
                                                run: std::rc::Rc::new(move |home, _, cx| {
                                                    home.act(key, &root, Action::Discard { paths: paths.clone() }, cx)
                                                }),
                                            },
                                            cx,
                                        )
                                    })),
                            );
                        }
                    }
                    list = list.child(section);
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
                    let refs = commit
                        .refs
                        .iter()
                        .map(|reference| reference.strip_prefix("remote: ").unwrap_or(reference))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let held = (project.to_string(), commit.clone());
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
                            }))
                            .when(control, |this| {
                                this.child(ui::hold(cx.listener(move |home, _, _, cx| {
                                    let sheet = home.commit_menu(key, &held.0, &held.1);
                                    home.show(sheet, cx)
                                })))
                            }),
                    );
                }
                list
            }
            Some(_) => div(),
        };
        let dim = theme.background;
        let backdrop =
            div().id("drawer-backdrop").flex_1().on_click(cx.listener(|home, _, window, cx| home.back(window, cx)));
        let backdrop = if drawer.slide {
            backdrop
                .with_animation("drawer-dim", ui::motion(), move |this, delta| this.bg(dim.opacity(0.7 * delta)))
                .into_any_element()
        } else {
            backdrop.bg(dim.opacity(0.7)).into_any_element()
        };
        let body = if drawer.fade {
            body.with_animation(
                SharedString::from(format!("drawer-{}-{}", panel_index(drawer.panel), drawer.folder)),
                ui::motion(),
                |this, delta| this.opacity(delta),
            )
            .into_any_element()
        } else {
            body.into_any_element()
        };
        let panel = div()
            .track_focus(&self.drawer_focus)
            .flex()
            .flex_col()
            .w(relative(0.88))
            .h_full()
            .bg(theme.background)
            .border_l_1()
            .border_color(theme.border)
            .child(header)
            .child(ui::scroll("drawer", &self.drawer_scroll).child(body));
        let panel = if drawer.slide {
            panel
                .with_animation("drawer-slide", ui::motion(), |this, delta| {
                    this.relative().left(relative(0.88 * (1.0 - delta)))
                })
                .into_any_element()
        } else {
            panel.into_any_element()
        };
        div().absolute().inset_0().flex().occlude().child(backdrop).child(panel)
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
        let control = self.link.read(cx).control(&key);
        let held = (project.to_string(), change.clone(), commit.clone());
        let quick = (project.to_string(), change.path.clone(), change.staged);
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
            .when(control && held.2.is_none(), |this| {
                this.child(
                    div()
                        .id(SharedString::from(format!("quick:{}:{}", quick.2, quick.1)))
                        .flex_none()
                        .w(px(28.0))
                        .flex()
                        .justify_center()
                        .text_color(theme.accent)
                        .child(if quick.2 { "−" } else { "+" })
                        .on_click(cx.listener(move |home, _, _, cx| {
                            let paths = vec![quick.1.clone()];
                            let action = if quick.2 { Action::Unstage { paths } } else { Action::Stage { paths } };
                            cx.stop_propagation();
                            home.act(key, &quick.0, action, cx)
                        })),
                )
            })
            .on_click(cx.listener(move |home, _, window, cx| {
                home.go(
                    Screen::Diff { key, project: root.clone(), path: path.clone(), commit: commit.clone() },
                    window,
                    cx,
                )
            }))
            .when(control, |this| {
                this.child(ui::hold(cx.listener(move |home, _, _, cx| {
                    let (root, change, commit) = &held;
                    let sheet = match commit {
                        Some(hash) => home.commit_file_menu(key, root, hash, change),
                        None => home.change_menu(key, root, change),
                    };
                    home.show(sheet, cx)
                })))
            })
    }
}
