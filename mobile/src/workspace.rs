use gpui::prelude::FluentBuilder;
use gpui::{
    AnimationExt, AnyElement, Context, Div, IntoElement, ParentElement, SharedString, StatefulInteractiveElement,
    Styled, Window, div, px,
};
use nucli_core::icons;
use nucli_core::session::Kind;
use nucli_core::theme::Theme;
use nucli_core::uplink::protocol::{Catalog, RemoteProject, RemoteTab, RemoteWorkspace, Request, Response};
use nucli_core::uplink::wire::Key;

use crate::board::{Group, group};
use crate::computer::{SECTIONS, section_index};
use crate::home::{Home, Screen, Section};
use crate::menus::close_project;
use crate::ui;

pub fn workspaces(catalog: &Catalog) -> Vec<RemoteWorkspace> {
    if !catalog.workspaces.is_empty() {
        return catalog.workspaces.clone();
    }
    let mut found: Vec<RemoteWorkspace> = Vec::new();
    for project in &catalog.projects {
        if !found.iter().any(|workspace| workspace.name == project.workspace) {
            found.push(RemoteWorkspace { name: project.workspace.clone(), open: true, muted: false });
        }
    }
    found
}

fn scratch(catalog: &Catalog, tab: &RemoteTab) -> bool {
    catalog.projects.iter().any(|project| project.path == tab.project && project.name.is_empty())
}

pub fn in_workspace(catalog: &Catalog, tab: &RemoteTab, name: &str) -> bool {
    tab.workspace == name
        || catalog
            .projects
            .iter()
            .any(|project| project.path == tab.project && project.workspace == name && !project.name.is_empty())
}

impl Home {
    fn summary(&self, tabs: &[&RemoteTab], empty: &str, theme: &Theme) -> (String, Option<Div>) {
        let agents = tabs.iter().filter(|tab| tab.kind == Kind::Agent).count();
        let waiting = tabs.iter().filter(|tab| group(tab) == Group::Waiting).count();
        let working = tabs.iter().filter(|tab| group(tab) == Group::Working).count();
        let mut parts = Vec::new();
        if agents > 0 {
            parts.push(ui::count(agents, "agent"));
        }
        if tabs.len() > agents {
            parts.push(ui::count(tabs.len() - agents, "shell"));
        }
        if parts.is_empty() {
            parts.push(empty.to_string());
        }
        let state = div().flex_none().text_size(px(ui::SMALL));
        let state = if waiting > 0 {
            Some(state.text_color(theme.yellow).child(format!("{waiting} waiting")))
        } else if working > 0 {
            Some(state.text_color(theme.accent).child(ui::spinner(self.shown.elapsed().as_millis())))
        } else {
            None
        };
        (parts.join(" · "), state)
    }

    pub fn workspaces_of(&self, key: Key, theme: &Theme, cx: &mut Context<Self>) -> Div {
        let link = self.link.read(cx);
        let control = link.control(&key);
        let Some(online) = link.online.get(&key) else {
            return div().child(ui::hint("The computer is offline.", theme).py(px(24.0)));
        };
        let catalog = online.catalog.clone();
        let all = workspaces(&catalog);
        let mut list = div().flex().flex_col();
        if all.is_empty() {
            return list.child(ui::hint("This computer shares no workspaces.", theme).py(px(16.0)));
        }
        for workspace in all.iter().filter(|workspace| workspace.open) {
            let projects = catalog
                .projects
                .iter()
                .filter(|project| project.workspace == workspace.name && !project.name.is_empty())
                .count();
            let tabs: Vec<&RemoteTab> =
                catalog.tabs.iter().filter(|tab| in_workspace(&catalog, tab, &workspace.name)).collect();
            let (detail, state) = self.summary(&tabs, "nothing running", theme);
            let name = workspace.name.clone();
            list = list.child(
                ui::row(SharedString::from(format!("workspace:{}", workspace.name)), theme)
                    .child(div().flex_none().w(px(18.0)).text_color(theme.accent).child(icons::WINDOW))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(div().truncate().child(workspace.name.clone()))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(ui::SMALL))
                                    .text_color(theme.faint)
                                    .child(format!("{} · {detail}", ui::count(projects, "project"))),
                            ),
                    )
                    .children(state)
                    .child(div().flex_none().text_color(theme.faint).child(icons::CHEVRON_RIGHT))
                    .on_click(cx.listener(move |home, _, window, cx| {
                        home.go(Screen::Workspace { key, workspace: name.clone() }, window, cx)
                    }))
                    .when(control, |this| this.child(self.workspace_hold(key, &workspace.name, cx))),
            );
        }
        let closed: Vec<&RemoteWorkspace> = all.iter().filter(|workspace| !workspace.open).collect();
        if !closed.is_empty() {
            list = list.child(ui::section("closed", theme));
        }
        for workspace in closed {
            let name = workspace.name.clone();
            list = list.child(
                ui::row(SharedString::from(format!("closed:{}", workspace.name)), theme)
                    .text_color(theme.muted)
                    .child(div().flex_none().w(px(18.0)).child(icons::WINDOW))
                    .child(div().flex_1().min_w_0().truncate().child(workspace.name.clone()))
                    .when(control, |this| {
                        this.child(div().flex_none().text_size(px(ui::SMALL)).text_color(theme.accent).child("open"))
                            .on_click(
                                cx.listener(move |home, _, window, cx| home.open_workspace(key, &name, window, cx)),
                            )
                            .child(self.workspace_hold(key, &workspace.name, cx))
                    }),
            );
        }
        list
    }

    fn workspace_hold(&self, key: Key, name: &str, cx: &mut Context<Self>) -> impl IntoElement {
        let name = name.to_string();
        ui::hold(cx.listener(move |home, _, _, cx| {
            let sheet = home.workspace_menu(key, &name, cx);
            home.show(sheet, cx)
        }))
    }

    pub fn open_workspace(&mut self, key: Key, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.go(Screen::Workspace { key, workspace: name.to_string() }, window, cx);
        self.notice = Some("opening…".into());
        self.fetch(
            key,
            Request::OpenWorkspace { workspace: name.to_string() },
            |home, response| {
                home.notice = match response {
                    Response::Failed { reason } => Some(reason),
                    _ => None,
                };
            },
            cx,
        );
    }

    pub fn workspace(&mut self, key: Key, name: &str, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let header = self.back_bar(name.to_string(), theme, cx);
        let mut tabs =
            div().flex().flex_none().gap(px(2.0)).px(px(10.0)).py(px(6.0)).border_b_1().border_color(theme.border);
        for (index, tab) in ui::tabs(SECTIONS, section_index(self.section), theme).into_iter().enumerate() {
            let section = [Section::Projects, Section::Agents, Section::History][index];
            tabs = tabs.child(tab.on_click(cx.listener(move |home, _, _, cx| home.set_section(section, cx))));
        }
        let body = match self.section {
            Section::Projects => self.projects_in(key, name, theme, cx),
            Section::Agents => self.agents_of(key, name, theme, cx),
            Section::History => self.history_of(key, name, theme, cx),
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(header)
            .child(tabs)
            .child(ui::scroll("workspace", &self.scroll).child(body.with_animation(
                ("section", section_index(self.section)),
                ui::motion(),
                |this, delta| this.opacity(delta),
            )))
            .into_any_element()
    }

    fn projects_in(&self, key: Key, name: &str, theme: &Theme, cx: &mut Context<Self>) -> Div {
        let link = self.link.read(cx);
        let control = link.control(&key);
        let catalog = link.online.get(&key).map(|online| online.catalog.clone());
        let mut list = div().flex().flex_col().children(self.notice_row(theme));
        match catalog {
            None => list = list.child(ui::hint("The computer is offline.", theme)),
            Some(catalog) => {
                for project in
                    catalog.projects.iter().filter(|project| project.workspace == name && !project.name.is_empty())
                {
                    list = list.child(self.project_row(key, project, &catalog, control, theme, cx));
                }
                for tab in catalog.tabs.iter().filter(|tab| tab.workspace == name && scratch(&catalog, tab)) {
                    let detail = tab.notice.clone().unwrap_or_default();
                    list = list.child(self.tab_row(key, tab, detail, theme, cx));
                }
                if control {
                    list = list.children(self.launch_rows(key, "", name, theme, cx)).child(
                        ui::row("add-project", theme)
                            .text_color(theme.accent)
                            .child(div().flex_none().w(px(18.0)).child(icons::FOLDER))
                            .child("add project")
                            .on_click(cx.listener({
                                let name = name.to_string();
                                move |home, _, window, cx| {
                                    home.go(Screen::Folders { key, workspace: name.clone() }, window, cx)
                                }
                            })),
                    );
                }
            }
        }
        list
    }

    fn project_row(
        &self,
        key: Key,
        project: &RemoteProject,
        catalog: &Catalog,
        control: bool,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tabs: Vec<&RemoteTab> = catalog.tabs.iter().filter(|tab| tab.project == project.path).collect();
        let (detail, state) = self.summary(&tabs, "nothing running", theme);
        let (path, held) = (project.path.clone(), project.path.clone());
        let color = project.color.map_or(theme.accent, |color| color.color(theme.dark));
        let id = SharedString::from(format!("project:{}", project.path));
        let row = ui::row(id.clone(), theme)
            .child(div().flex_none().w(px(18.0)).text_color(color).child(icons::FOLDER))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(div().truncate().child(project.name.clone()))
                    .child(div().truncate().text_size(px(ui::SMALL)).text_color(theme.faint).child(detail)),
            )
            .children(state)
            .child(div().flex_none().text_color(theme.faint).child(icons::CHEVRON_RIGHT))
            .on_click(cx.listener(move |home, _, window, cx| {
                home.go(Screen::Project { key, project: path.clone() }, window, cx)
            }))
            .when(control, |this| {
                this.child(ui::hold(cx.listener(move |home, _, _, cx| {
                    let sheet = home.project_menu(key, &held, cx);
                    home.show(sheet, cx)
                })))
            });
        if control {
            let close = close_project(key, &project.path, &project.name);
            self.swipeable(id, row, "close", close, theme, cx).into_any_element()
        } else {
            row.into_any_element()
        }
    }
}
