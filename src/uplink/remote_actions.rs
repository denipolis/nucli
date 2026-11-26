use std::path::Path;
use std::rc::Rc;

use gpui::{App, AppContext, Context, PromptLevel, Window};

use super::protocol::{Action, Catalog, Request, Response, TabAction};
use super::wire::{self, Key};
use crate::menu::{self, Item};
use crate::palette::{self, Palette};
use crate::session::Kind;
use crate::window::WorkspaceView;
use crate::{actions, icons, store};

pub fn host(workspace: &str, cx: &App) -> Option<Key> {
    let store = store::store(cx).read(cx);
    store.workspace(workspace)?.host.as_ref().and_then(|host| wire::decode_key(&host.key))
}

pub fn catalog(key: &Key, cx: &App) -> Option<Catalog> {
    super::uplink(cx)?.read(cx).links.get(key)?.catalog.clone()
}

fn tab_of(id: &str) -> String {
    id.rsplit(':').next().unwrap_or_default().to_string()
}

fn send(workspace: String, key: Key, request: Request, cx: &mut App) {
    let Some(uplink) = super::uplink(cx) else { return };
    let answered = uplink.update(cx, |uplink, _| uplink.ask(key, request));
    cx.spawn(async move |cx| {
        if let Ok(Response::Failed { reason }) = answered.await {
            cx.update(|cx| actions::alert(&workspace, "The other computer refused", reason, cx));
        }
    })
    .detach();
}

pub fn tab_action(workspace: &str, id: &str, action: TabAction, cx: &mut App) {
    if let Some(key) = host(workspace, cx) {
        send(workspace.to_string(), key, Request::Tab { tab: tab_of(id), action }, cx);
    }
}

pub fn project_action(workspace: &str, path: &Path, action: Action, cx: &mut App) {
    if let Some(key) = host(workspace, cx) {
        let project = path.to_string_lossy().into_owned();
        send(workspace.to_string(), key, Request::Act { project, action }, cx);
    }
}

fn control(workspace: &str, cx: &App) -> bool {
    host(workspace, cx).and_then(|key| catalog(&key, cx)).is_some_and(|catalog| catalog.control)
}

pub fn launch_first(
    view: &mut WorkspaceView,
    project: &Path,
    agent: bool,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) {
    let Some(key) = host(&view.workspace, cx) else { return };
    let Some(catalog) = catalog(&key, cx).filter(|catalog| catalog.control) else { return };
    let Some(launcher) = catalog.launchers.iter().find(|launcher| launcher.agent == agent) else {
        return launch(view, project, Some(agent), window, cx);
    };
    let name = view.state(cx).map(|state| state.name.clone()).unwrap_or_default();
    let path = project.to_string_lossy().into_owned();
    if let Some(uplink) = super::uplink(cx) {
        uplink.update(cx, |uplink, cx| uplink.request_open(key, &path, &launcher.name, &name, cx));
    }
}

pub fn launch(
    view: &mut WorkspaceView,
    project: &Path,
    agent: Option<bool>,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) {
    let Some(key) = host(&view.workspace, cx) else { return };
    let Some(catalog) = catalog(&key, cx).filter(|catalog| catalog.control) else { return };
    let name = view.state(cx).map(|state| state.name.clone()).unwrap_or_default();
    let theme = crate::ui::theme(cx);
    let items: Vec<palette::Item> = catalog
        .launchers
        .iter()
        .filter(|launcher| agent.is_none_or(|agent| launcher.agent == agent))
        .map(|launcher| {
            let (path, launcher_name, workspace) =
                (project.to_string_lossy().into_owned(), launcher.name.clone(), name.clone());
            let (glyph, color) = if launcher.agent {
                let (glyph, color) = icons::agent(&launcher.command);
                (glyph, Some(color.color(theme.dark)))
            } else {
                (icons::TERMINAL, None)
            };
            palette::Item::new(launcher.name.clone(), move |_, _, cx| {
                if let Some(uplink) = super::uplink(cx) {
                    uplink.update(cx, |uplink, cx| uplink.request_open(key, &path, &launcher_name, &workspace, cx));
                }
            })
            .glyph(glyph, color)
            .subtitle(launcher.command.clone())
        })
        .collect();
    let title = match agent {
        Some(true) => "new agent",
        Some(false) => "new shell",
        None => "new tab",
    };
    let palette = cx.new(|cx| Palette::new(title, "", items, 0, cx));
    view.show_palette(palette, window, cx);
}

pub fn close_tab(view: &mut WorkspaceView, id: &str, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let workspace = view.workspace.clone();
    let agent = view.store(cx).session(id).is_some_and(|session| {
        let session = session.read(cx);
        session.is_running() && (session.kind == Kind::Agent || session.running_agent.is_some())
    });
    let id = id.to_string();
    if !agent {
        tab_action(&workspace, &id, TabAction::Close, cx);
        return;
    }
    let title = view.store(cx).session(&id).map(|session| session.read(cx).display_title()).unwrap_or_default();
    let answer = window.prompt(
        PromptLevel::Warning,
        &format!("Close {title}?"),
        Some("The agent is still running on the other computer and will be stopped."),
        &["Close", "Cancel"],
        cx,
    );
    cx.spawn(async move |_, cx| {
        if answer.await == Ok(0) {
            cx.update(|cx| tab_action(&workspace, &id, TabAction::Close, cx));
        }
    })
    .detach();
}

pub fn close_project(view: &mut WorkspaceView, path: &Path, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let workspace = view.workspace.clone();
    let name =
        view.state(cx).and_then(|state| state.project(path)).map(|project| project.display_name()).unwrap_or_default();
    let answer = window.prompt(
        PromptLevel::Warning,
        &format!("Close {name}?"),
        Some("It closes on the other computer, with its tabs."),
        &["Close Project", "Cancel"],
        cx,
    );
    let path = path.to_path_buf();
    cx.spawn(async move |_, cx| {
        if answer.await == Ok(0) {
            cx.update(|cx| project_action(&workspace, &path, Action::CloseProject, cx));
        }
    })
    .detach();
}

pub fn rename(workspace: &str, target: &crate::window::Rename, text: &str, cx: &mut App) {
    match target {
        crate::window::Rename::Tab(id) => {
            tab_action(workspace, id, TabAction::Rename { title: text.trim().to_string() }, cx)
        }
        crate::window::Rename::Project(path) => {
            project_action(workspace, path, Action::RenameProject { name: text.trim().to_string() }, cx)
        }
    }
}

pub fn tab_menu(view: &WorkspaceView, id: &str, cx: &App) -> Vec<Item> {
    let session = view.store(cx).session(id).map(|session| session.read(cx));
    let title = session.map(|session| session.display_title()).unwrap_or_default();
    let color = session.and_then(|session| session.color);
    let mut items = Vec::new();
    if control(&view.workspace, cx) {
        let rename = id.to_string();
        items.push(menu::action("Rename…", move |view, window, cx| {
            actions::begin_rename(view, crate::window::Rename::Tab(rename.clone()), window, cx)
        }));
        let colored = id.to_string();
        items.push(Item::Colors {
            current: color,
            run: Rc::new(move |color, view, _, cx| {
                tab_action(&view.workspace.clone(), &colored, TabAction::Color { color }, cx)
            }),
        });
        items.push(Item::Separator);
        if view.state(cx).and_then(|state| state.project_of(id)).is_some_and(|project| !project.is_scratch()) {
            let moved = id.to_string();
            items.push(menu::action("Move Out of Project", move |view, _, cx| {
                tab_action(&view.workspace.clone(), &moved, TabAction::MoveOut, cx)
            }));
        }
        let closed = id.to_string();
        items.push(menu::action("Close Tab", move |view, window, cx| close_tab(view, &closed, window, cx)));
        items.push(Item::Separator);
    }
    items.push(menu::copy("Copy Title", title));
    items
}

pub fn project_menu(view: &WorkspaceView, path: &Path, cx: &App) -> Vec<Item> {
    let workspace = view.workspace.clone();
    let Some(key) = host(&workspace, cx) else { return Vec::new() };
    let Some(catalog) = catalog(&key, cx) else { return Vec::new() };
    let project = view.state(cx).and_then(|state| state.project(path)).cloned();
    let scratch = project.as_ref().is_none_or(|project| project.is_scratch());
    let remote_path = path.to_string_lossy().into_owned();
    let mut items = Vec::new();
    if catalog.control {
        let (agent, shell) = (path.to_path_buf(), path.to_path_buf());
        items.push(menu::action("New Agent…", move |view, window, cx| launch(view, &agent, Some(true), window, cx)));
        items.push(menu::action("New Shell…", move |view, window, cx| launch(view, &shell, Some(false), window, cx)));
        items.push(Item::Separator);
    }
    if !scratch {
        let download = remote_path.clone();
        items.push(menu::action("Download File…", move |view, window, cx| {
            super::files::pick_download(view, key, download.clone(), window, cx)
        }));
        if catalog.control {
            let upload = remote_path.clone();
            items.push(menu::action("Upload Files…", move |view, _, cx| {
                super::files::pick_upload(key, upload.clone(), view.workspace.clone(), cx)
            }));
            items.push(Item::Separator);
            let renamed = path.to_path_buf();
            items.push(menu::action("Rename…", move |view, window, cx| {
                actions::begin_rename(view, crate::window::Rename::Project(renamed.clone()), window, cx)
            }));
            let colored = path.to_path_buf();
            items.push(Item::Colors {
                current: project.as_ref().and_then(|project| project.color),
                run: Rc::new(move |color, view, _, cx| {
                    project_action(&view.workspace.clone(), &colored, Action::ColorProject { color }, cx)
                }),
            });
            items.push(Item::Separator);
            let closed = path.to_path_buf();
            items.push(menu::action("Close Project", move |view, window, cx| close_project(view, &closed, window, cx)));
            items.push(Item::Separator);
        }
        items.push(menu::copy("Copy Path", remote_path));
    }
    items
}

fn join(folder: &str, name: &str) -> String {
    if folder.is_empty() { name.to_string() } else { format!("{folder}/{name}") }
}

fn ask_name(
    view: &mut WorkspaceView,
    title: &str,
    value: &str,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
    done: impl Fn(String) -> Action + 'static,
    root: std::path::PathBuf,
) {
    actions::prompt(view, title, value, window, cx, move |text, view, _, cx| {
        if !text.is_empty() {
            project_action(&view.workspace.clone(), &root, done(text), cx);
        }
    });
}

pub fn file_menu(view: &WorkspaceView, root: &Path, path: &str, is_dir: bool, cx: &App) -> Vec<Item> {
    let Some(key) = host(&view.workspace, cx) else { return Vec::new() };
    let control = control(&view.workspace, cx);
    let folder = if is_dir {
        path.to_string()
    } else {
        path.rsplit_once('/').map(|(parent, _)| parent.to_string()).unwrap_or_default()
    };
    let project = root.to_string_lossy().into_owned();
    let mut items = Vec::new();
    if control {
        for (label, directory) in [("New File…", false), ("New Folder…", true)] {
            let (folder, root) = (folder.clone(), root.to_path_buf());
            items.push(menu::action(label, move |view, window, cx| {
                let folder = folder.clone();
                ask_name(
                    view,
                    label.trim_end_matches('…'),
                    "",
                    window,
                    cx,
                    move |name| Action::Create { path: join(&folder, &name), folder: directory },
                    root.clone(),
                );
            }));
        }
        items.push(Item::Separator);
    }
    if !path.is_empty() {
        if !is_dir {
            let (download, project) = (path.to_string(), project.clone());
            items.push(menu::action("Download…", move |view, _, cx| {
                super::files::save(key, project.clone(), download.clone(), view.workspace.clone(), cx)
            }));
        }
        if control {
            let (from, base) = (path.to_string(), root.to_path_buf());
            let name = path.rsplit('/').next().unwrap_or(path).to_string();
            items.push(menu::action("Rename…", move |view, window, cx| {
                let from = from.clone();
                let parent = from.rsplit_once('/').map(|(parent, _)| parent.to_string()).unwrap_or_default();
                ask_name(
                    view,
                    "Rename",
                    &name,
                    window,
                    cx,
                    move |name| Action::Rename { from: from.clone(), to: join(&parent, &name) },
                    base.clone(),
                );
            }));
            let (copied, copy_base) = (path.to_string(), root.to_path_buf());
            items.push(menu::action("Duplicate", move |view, _, cx| {
                project_action(&view.workspace.clone(), &copy_base, Action::Duplicate { path: copied.clone() }, cx)
            }));
            let (deleted, delete_base) = (path.to_string(), root.to_path_buf());
            items.push(menu::action("Delete", move |view, window, cx| {
                let answer = window.prompt(
                    PromptLevel::Warning,
                    &format!("Delete {deleted}?"),
                    Some("It goes to the trash of the other computer."),
                    &["Delete", "Cancel"],
                    cx,
                );
                let (workspace, root, deleted) = (view.workspace.clone(), delete_base.clone(), deleted.clone());
                cx.spawn(async move |_, cx| {
                    if answer.await == Ok(0) {
                        cx.update(|cx| project_action(&workspace, &root, Action::Delete { path: deleted }, cx));
                    }
                })
                .detach();
            }));
            items.push(Item::Separator);
            let (opened, revealed, open_base) = (path.to_string(), path.to_string(), root.to_path_buf());
            let reveal_root = open_base.clone();
            items.push(menu::action("Open on That Computer", move |view, _, cx| {
                project_action(&view.workspace.clone(), &open_base, Action::Open { path: opened.clone() }, cx)
            }));
            items.push(menu::action("Reveal on That Computer", move |view, _, cx| {
                project_action(&view.workspace.clone(), &reveal_root, Action::Reveal { path: revealed.clone() }, cx)
            }));
            items.push(Item::Separator);
        }
        items.push(menu::copy("Copy Path", root.join(path).to_string_lossy().into_owned()));
        items.push(menu::copy("Copy Relative Path", path.to_string()));
    }
    items
}
