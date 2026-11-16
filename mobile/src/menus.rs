use std::time::Instant;

use gpui::Context;
use nucli_core::session::Kind;
use nucli_core::uplink::protocol::{
    Action, RemoteChange, RemoteCommit, RemoteConversation, RemoteTab, Request, Reset, Response, TabAction,
};
use nucli_core::uplink::wire::Key;

use crate::home::{Home, Panel, Screen};
use crate::sheet::{Item, Sheet, action, ask, confirm, copy};

pub struct Clip {
    pub key: Key,
    pub project: String,
    pub path: String,
    pub cut: bool,
}

fn name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

fn parent(path: &str) -> String {
    path.rsplit_once('/').map(|(parent, _)| parent.to_string()).unwrap_or_default()
}

fn join(folder: &str, name: &str) -> String {
    let name = name.trim_matches('/');
    if folder.is_empty() { name.to_string() } else { format!("{folder}/{name}") }
}

pub fn full(project: &str, path: &str) -> String {
    if path.is_empty() { project.to_string() } else { format!("{project}/{path}") }
}

fn short(hash: &str) -> String {
    hash.chars().take(7).collect()
}

impl Home {
    pub fn say(&mut self, text: impl Into<String>) {
        self.toast = Some((text.into(), Instant::now()));
    }

    pub fn act(&mut self, key: Key, project: &str, action: Action, cx: &mut Context<Self>) {
        self.send(key, Request::Act { project: project.to_string(), action }, cx);
    }

    pub fn tab_act(&mut self, key: Key, tab: &str, action: TabAction, cx: &mut Context<Self>) {
        self.send(key, Request::Tab { tab: tab.to_string(), action }, cx);
    }

    fn send(&mut self, key: Key, request: Request, cx: &mut Context<Self>) {
        self.send_then(key, request, |_, _| {}, cx);
    }

    pub fn send_then(
        &mut self,
        key: Key,
        request: Request,
        done: impl FnOnce(&mut Home, &mut Context<Home>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let receiver = self.link.update(cx, |link, cx| link.ask(key, request, cx));
        cx.spawn(async move |this, cx| {
            let response = receiver.await.unwrap_or(Response::Failed { reason: "No answer.".into() });
            let _ = this.update(cx, |home, cx| {
                match response {
                    Response::Failed { reason } => home.say(reason),
                    _ => done(home, cx),
                }
                home.refresh_drawer(cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn place(&self) -> Option<(Key, String)> {
        match &self.screen {
            Screen::Project { key, project }
            | Screen::File { key, project, .. }
            | Screen::Diff { key, project, .. }
            | Screen::Commit { key, project, .. } => Some((*key, project.clone())),
            _ => None,
        }
    }

    pub fn refresh_drawer(&mut self, cx: &mut Context<Self>) {
        let (Some((key, project)), Some(drawer)) = (self.place(), &self.drawer) else { return };
        let (panel, folder) = (drawer.panel, drawer.folder.clone());
        let request = match panel {
            Panel::Files => Request::Files { project, folder: folder.clone() },
            Panel::Changes => Request::Changes { project },
            Panel::Log => Request::Log { project },
        };
        self.fetch(
            key,
            request,
            move |home, response| {
                if let Some(drawer) = &mut home.drawer
                    && drawer.panel == panel
                    && drawer.folder == folder
                {
                    drawer.data = Some(response);
                }
            },
            cx,
        );
    }

    fn paste_item(&self, key: Key, project: &str, folder: &str) -> Option<Item> {
        let clip = self.clip.as_ref().filter(|clip| clip.key == key)?;
        let (source, from, cut, project, folder) =
            (clip.project.clone(), clip.path.clone(), clip.cut, project.to_string(), folder.to_string());
        Some(action(format!("paste {}", name(&from)), move |home, _, cx| {
            if cut {
                home.clip = None;
            }
            let action = Action::Paste { source: source.clone(), from: from.clone(), folder: folder.clone(), cut };
            home.act(key, &project, action, cx);
        }))
    }

    fn create_items(&self, key: Key, project: &str, folder: &str) -> Vec<Item> {
        let place = if folder.is_empty() { String::new() } else { format!(" in {folder}") };
        [("new file", false), ("new folder", true)]
            .into_iter()
            .map(|(label, is_folder)| {
                let (project, folder) = (project.to_string(), folder.to_string());
                ask(label, format!("{label}{place}"), "", move |text, home, _, cx| {
                    home.act(key, &project, Action::Create { path: join(&folder, &text), folder: is_folder }, cx)
                })
            })
            .collect()
    }

    fn location_items(&self, key: Key, project: &str, path: &str, open: bool) -> Vec<Item> {
        let mut items = Vec::new();
        if open {
            let (project, path) = (project.to_string(), path.to_string());
            items.push(action("open on computer", move |home, _, cx| {
                home.act(key, &project, Action::Open { path: path.clone() }, cx)
            }));
        }
        let (root, target) = (project.to_string(), path.to_string());
        items.push(action("reveal on computer", move |home, _, cx| {
            home.act(key, &root, Action::Reveal { path: target.clone() }, cx)
        }));
        items.push(copy("copy path", full(project, path)));
        if !path.is_empty() {
            items.push(copy("copy relative path", path));
        }
        items
    }

    pub fn folder_menu(&self, key: Key, project: &str, folder: &str) -> Sheet {
        let mut items = self.create_items(key, project, folder);
        items.extend(self.paste_item(key, project, folder));
        items.extend(self.location_items(key, project, folder, false));
        Sheet::Menu { title: if folder.is_empty() { "project".into() } else { folder.to_string() }, items }
    }

    pub fn entry_menu(&self, key: Key, project: &str, path: &str, is_folder: bool) -> Sheet {
        let folder = if is_folder { path.to_string() } else { parent(path) };
        let mut items = self.create_items(key, project, &folder);
        if is_folder {
            let absolute = full(project, path);
            items.push(action("open as project", move |home, _, cx| {
                home.send(key, Request::AddProject { path: absolute.clone() }, cx)
            }));
        }
        items.extend(self.location_items(key, project, path, true));
        for (label, cut) in [("cut", true), ("copy", false)] {
            let (project, path) = (project.to_string(), path.to_string());
            items.push(action(label, move |home, _, _| {
                home.clip = Some(Clip { key, project: project.clone(), path: path.clone(), cut });
                home.say(format!("{} {}", if cut { "cut" } else { "copied" }, name(&path)));
            }));
        }
        items.extend(self.paste_item(key, project, &folder));
        {
            let (project, path) = (project.to_string(), path.to_string());
            items.push(action("duplicate", move |home, _, cx| {
                home.act(key, &project, Action::Duplicate { path: path.clone() }, cx)
            }));
        }
        {
            let (project, from) = (project.to_string(), path.to_string());
            items.push(ask("rename", format!("rename {}", name(path)), name(path), move |to, home, _, cx| {
                home.act(key, &project, Action::Rename { from: from.clone(), to: join(&parent(&from), &to) }, cx)
            }));
        }
        {
            let (project, from) = (project.to_string(), path.to_string());
            items.push(ask("move", format!("move {}", name(path)), path, move |to, home, _, cx| {
                home.act(key, &project, Action::Rename { from: from.clone(), to }, cx)
            }));
        }
        let (root, target) = (project.to_string(), path.to_string());
        items.push(confirm("delete", format!("delete {}?", name(path)), "It is moved to the trash.", "delete", {
            move |home, _, cx| home.act(key, &root, Action::Delete { path: target.clone() }, cx)
        }));
        Sheet::Menu { title: path.to_string(), items }
    }

    pub fn change_menu(&self, key: Key, project: &str, change: &RemoteChange) -> Sheet {
        let deleted = change.status == "D";
        let mut items = Vec::new();
        if !(deleted && !change.staged) {
            let (root, path) = (project.to_string(), change.path.clone());
            items.push(action("open changes", move |home, window, cx| {
                let screen = Screen::Diff { key, project: root.clone(), path: path.clone(), commit: None };
                home.go(screen, window, cx)
            }));
        }
        let paths = vec![change.path.clone()];
        let root = project.to_string();
        if change.staged {
            items.push(action("unstage", move |home, _, cx| {
                home.act(key, &root, Action::Unstage { paths: paths.clone() }, cx)
            }));
        } else {
            let (stage_root, stage_paths) = (root.clone(), paths.clone());
            items.push(action("stage", move |home, _, cx| {
                home.act(key, &stage_root, Action::Stage { paths: stage_paths.clone() }, cx)
            }));
            items.push(confirm(
                "discard changes",
                "discard changes?",
                format!("Discard changes to {}? This cannot be undone. New files go to the trash.", change.path),
                "discard",
                move |home, _, cx| home.act(key, &root, Action::Discard { paths: paths.clone() }, cx),
            ));
        }
        items.extend(self.location_items(key, project, &change.path, !deleted));
        Sheet::Menu { title: change.path.clone(), items }
    }

    pub fn commit_file_menu(&self, key: Key, project: &str, hash: &str, change: &RemoteChange) -> Sheet {
        let mut items = Vec::new();
        {
            let (root, path, hash) = (project.to_string(), change.path.clone(), hash.to_string());
            items.push(action("open changes", move |home, window, cx| {
                let screen =
                    Screen::Diff { key, project: root.clone(), path: path.clone(), commit: Some(hash.clone()) };
                home.go(screen, window, cx)
            }));
        }
        {
            let (root, path) = (project.to_string(), change.path.clone());
            items.push(action("open current version", move |home, window, cx| {
                home.go(Screen::File { key, project: root.clone(), path: path.clone() }, window, cx)
            }));
        }
        if change.status != "D" {
            let (root, path, hash) = (project.to_string(), change.path.clone(), hash.to_string());
            items.push(confirm(
                "restore this version",
                format!("restore {} from this commit?", change.path),
                "The file in the working tree is overwritten.",
                "restore",
                move |home, _, cx| home.act(key, &root, Action::Restore { hash: hash.clone(), path: path.clone() }, cx),
            ));
        }
        items.extend(self.location_items(key, project, &change.path, false));
        Sheet::Menu { title: format!("{} · {}", change.path, short(hash)), items }
    }

    pub fn commit_menu(&self, key: Key, project: &str, commit: &RemoteCommit) -> Sheet {
        let hash = commit.hash.clone();
        let git = |label: String, action: Action| {
            let root = project.to_string();
            self::action(label, move |home, _, cx| home.act(key, &root, action.clone(), cx))
        };
        let mut items = Vec::new();
        for reference in &commit.refs {
            if let Some(remote) = reference.strip_prefix("remote: ") {
                if !remote.ends_with("/HEAD") {
                    items.push(git(format!("check out {remote}"), Action::Track { remote: remote.to_string() }));
                }
            } else if !reference.starts_with("tag: ") && !reference.starts_with("HEAD") {
                let branch = reference.to_string();
                items.push(git(format!("switch to {branch}"), Action::Switch { branch }));
            }
        }
        items.push(git("check out detached".into(), Action::Detach { hash: hash.clone() }));
        for (label, title, tag) in
            [("create branch", "new branch from this commit", false), ("create tag", "new tag on this commit", true)]
        {
            let (root, at) = (project.to_string(), hash.clone());
            items.push(ask(label, title, "", move |name, home, _, cx| {
                let action = if tag {
                    Action::Tag { name, at: at.clone() }
                } else {
                    Action::Branch { name, at: Some(at.clone()) }
                };
                home.act(key, &root, action, cx)
            }));
        }
        items.push(git("cherry-pick".into(), Action::CherryPick { hash: hash.clone() }));
        items.push(git("revert".into(), Action::Revert { hash: hash.clone() }));
        items.push(Item::Header("reset current branch to here".into()));
        items.push(git("soft (keep changes staged)".into(), Action::Reset { hash: hash.clone(), mode: Reset::Soft }));
        items.push(git("mixed (keep changes)".into(), Action::Reset { hash: hash.clone(), mode: Reset::Mixed }));
        {
            let (root, hash) = (project.to_string(), hash.clone());
            items.push(confirm(
                "hard (discard changes)",
                "reset hard?",
                "Uncommitted changes are discarded and later commits leave the branch.",
                "reset",
                move |home, _, cx| home.act(key, &root, Action::Reset { hash: hash.clone(), mode: Reset::Hard }, cx),
            ));
        }
        for reference in &commit.refs {
            let (label, title, detail, action) = if let Some(tag) = reference.strip_prefix("tag: ") {
                (
                    format!("delete tag {tag}"),
                    format!("delete tag {tag}?"),
                    "The tag is removed locally.",
                    Action::DeleteTag { name: tag.to_string() },
                )
            } else if reference.starts_with("remote: ") || reference.starts_with("HEAD") {
                continue;
            } else {
                let branch = reference.as_str();
                (
                    format!("delete branch {branch}"),
                    format!("delete branch {branch}?"),
                    "Commits only on this branch become unreachable.",
                    Action::DeleteBranch { name: branch.to_string() },
                )
            };
            let root = project.to_string();
            items.push(confirm(&label, title, detail, "delete", move |home, _, cx| {
                home.act(key, &root, action.clone(), cx)
            }));
        }
        items.push(copy("copy hash", hash.clone()));
        items.push(copy("copy short hash", short(&hash)));
        items.push(copy("copy message", commit.subject.clone()));
        Sheet::Menu { title: format!("{} {}", short(&hash), commit.subject), items }
    }

    pub fn branch_menu(&self, key: Key, project: &str, current: &str, branches: &[String]) -> Sheet {
        let mut items: Vec<Item> = branches
            .iter()
            .filter(|branch| *branch != current)
            .map(|branch| {
                let (root, branch) = (project.to_string(), branch.clone());
                action(format!("switch to {branch}"), move |home, _, cx| {
                    home.act(key, &root, Action::Switch { branch: branch.clone() }, cx)
                })
            })
            .collect();
        let root = project.to_string();
        items.push(ask("new branch", "new branch", "", move |name, home, _, cx| {
            home.act(key, &root, Action::Branch { name, at: None }, cx)
        }));
        Sheet::Menu { title: format!("on {current}"), items }
    }

    pub fn tab_menu(&self, key: Key, tab: &RemoteTab, scratch: bool) -> Sheet {
        let id = tab.id.clone();
        let mut items = Vec::new();
        {
            let id = id.clone();
            items.push(ask("rename", "rename tab", tab.title.clone(), move |title, home, _, cx| {
                home.tab_act(key, &id, TabAction::Rename { title }, cx)
            }));
        }
        {
            let id = id.clone();
            items.push(Item::Colors {
                current: tab.color,
                run: std::rc::Rc::new(move |color, home, _, cx| home.tab_act(key, &id, TabAction::Color { color }, cx)),
            });
        }
        if !scratch {
            let id = id.clone();
            items
                .push(action("move out of project", move |home, _, cx| home.tab_act(key, &id, TabAction::MoveOut, cx)));
        }
        if tab.kind == Kind::Agent && tab.running {
            items.push(confirm(
                "close tab",
                format!("close {}?", tab.title),
                "The agent is still running and will be stopped.",
                "close",
                move |home, _, cx| home.tab_act(key, &id, TabAction::Close, cx),
            ));
        } else {
            items.push(action("close tab", move |home, _, cx| home.tab_act(key, &id, TabAction::Close, cx)));
        }
        Sheet::Menu { title: tab.title.clone(), items }
    }

    pub fn project_menu(&self, key: Key, project: &str, cx: &Context<Self>) -> Sheet {
        let catalog = self.link.read(cx).online.get(&key).map(|online| online.catalog.clone()).unwrap_or_default();
        let Some(remote) = catalog.projects.iter().find(|item| item.path == project).cloned() else {
            return Sheet::Menu { title: project.to_string(), items: Vec::new() };
        };
        let mut items = Vec::new();
        for launcher in &catalog.launchers {
            let (root, name) = (project.to_string(), launcher.name.clone());
            items.push(action(format!("new {name}"), move |home, _, cx| {
                home.link.update(cx, |link, _| link.open_tab(key, &root, &name))
            }));
        }
        {
            let root = project.to_string();
            items.push(ask("rename", "rename project", remote.name.clone(), move |name, home, _, cx| {
                home.act(key, &root, Action::RenameProject { name }, cx)
            }));
        }
        {
            let root = project.to_string();
            items.push(Item::Colors {
                current: remote.color,
                run: std::rc::Rc::new(move |color, home, _, cx| {
                    home.act(key, &root, Action::ColorProject { color }, cx)
                }),
            });
        }
        items.push(Item::Header("move to workspace".into()));
        let mut workspaces: Vec<String> = catalog.projects.iter().map(|item| item.workspace.clone()).collect();
        workspaces.sort();
        workspaces.dedup();
        for workspace in workspaces.into_iter().filter(|workspace| *workspace != remote.workspace) {
            let root = project.to_string();
            items.push(action(workspace.clone(), move |home, _, cx| {
                home.act(key, &root, Action::MoveProject { workspace: workspace.clone() }, cx)
            }));
        }
        {
            let root = project.to_string();
            items.push(ask("new workspace", "new workspace", "", move |workspace, home, _, cx| {
                home.act(key, &root, Action::MoveProject { workspace }, cx)
            }));
        }
        items.extend(self.location_items(key, project, "", false));
        let root = project.to_string();
        items.push(confirm(
            "close project",
            format!("close {}?", remote.name),
            "Its tabs close and its running agents are stopped.",
            "close",
            move |home, window, cx| {
                home.act(key, &root, Action::CloseProject, cx);
                if matches!(&home.screen, Screen::Project { project, .. } if *project == root) {
                    home.back(window, cx);
                }
            },
        ));
        Sheet::Menu { title: remote.name, items }
    }

    pub fn conversation_menu(&self, key: Key, item: &RemoteConversation, control: bool) -> Sheet {
        let mut items = Vec::new();
        if control {
            let (id, path) = (item.id.clone(), item.project.clone());
            items
                .push(action("resume", move |home, window, cx| home.resume(key, id.clone(), path.clone(), window, cx)));
        }
        items.push(copy("copy title", item.title.clone()));
        items.push(copy("copy conversation id", item.id.clone()));
        items.push(copy("copy path", item.cwd.clone()));
        Sheet::Menu { title: item.title.clone(), items }
    }

    pub fn show(&mut self, sheet: Sheet, cx: &mut Context<Self>) {
        self.sheet = Some(sheet);
        cx.notify();
    }

    pub fn branches(&mut self, key: Key, project: &str, cx: &mut Context<Self>) {
        let root = project.to_string();
        self.fetch(
            key,
            Request::Branches { project: root.clone() },
            move |home, response| match response {
                Response::Branches { current, branches } => {
                    home.sheet = Some(home.branch_menu(key, &root, &current, &branches))
                }
                Response::Failed { reason } => home.say(reason),
                _ => {}
            },
            cx,
        );
    }
}
