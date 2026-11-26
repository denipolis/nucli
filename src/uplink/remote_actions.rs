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
    open(view, key, project, &launcher.name, false, cx);
}

pub fn launchers(view: &WorkspaceView, cx: &App) -> Vec<super::protocol::RemoteLauncher> {
    host(&view.workspace, cx)
        .and_then(|key| catalog(&key, cx))
        .filter(|catalog| catalog.control)
        .map(|catalog| catalog.launchers)
        .unwrap_or_default()
}

pub fn open_launcher(view: &mut WorkspaceView, project: &Path, launcher: &str, cx: &mut Context<WorkspaceView>) {
    if let Some(key) = host(&view.workspace, cx) {
        open(view, key, project, launcher, false, cx);
    }
}

pub fn resume(view: &WorkspaceView, conversation: &str, cx: &mut App) {
    if let Some(key) = host(&view.workspace, cx).filter(|_| control(&view.workspace, cx)) {
        send(view.workspace.clone(), key, Request::Resume { conversation: conversation.to_string() }, cx);
    }
}

pub fn new_terminal(view: &mut WorkspaceView, project: &Path, cx: &mut Context<WorkspaceView>) {
    let Some(key) = host(&view.workspace, cx) else { return };
    let Some(catalog) = catalog(&key, cx).filter(|catalog| catalog.control) else { return };
    let shell = catalog.launchers.iter().find(|launcher| !launcher.agent).map(|launcher| launcher.name.clone());
    open(view, key, project, &shell.unwrap_or_default(), true, cx);
}

fn open(view: &WorkspaceView, key: Key, project: &Path, launcher: &str, panel: bool, cx: &mut Context<WorkspaceView>) {
    let name = view.state(cx).map(|state| state.name.clone()).unwrap_or_default();
    let path = project.to_string_lossy().into_owned();
    if let Some(uplink) = super::uplink(cx) {
        uplink.update(cx, |uplink, cx| uplink.request_open(key, &path, launcher, &name, panel, cx));
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
                    uplink.update(cx, |uplink, cx| {
                        uplink.request_open(key, &path, &launcher_name, &workspace, false, cx)
                    });
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
                super::files::pick_upload(key, upload.clone(), String::new(), view.workspace.clone(), cx)
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
        let (upload, project) = (folder.clone(), project.clone());
        items.push(menu::action("Upload Files…", move |view, _, cx| {
            super::files::pick_upload(key, project.clone(), upload.clone(), view.workspace.clone(), cx)
        }));
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

fn separator(path: &str) -> char {
    if path.contains('\\') && !path.contains('/') { '\\' } else { '/' }
}

pub fn browse(view: &mut WorkspaceView, path: String, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let Some(key) = host(&view.workspace, cx) else { return };
    let Some(uplink) = super::uplink(cx) else { return };
    let control = control(&view.workspace, cx);
    let name = view.state(cx).map(|state| state.name.clone()).unwrap_or_default();
    let asked = uplink.update(cx, |uplink, _| uplink.ask(key, Request::Folders { path }));
    cx.spawn_in(window, async move |this, cx| {
        let Ok(Response::Folders { path, folders }) = asked.await else { return };
        let _ = this.update_in(cx, |view, window, cx| {
            let slash = separator(&path);
            let trimmed = path.trim_end_matches(slash).to_string();
            let mut items = Vec::new();
            if control {
                let (target, workspace) = (path.clone(), name.clone());
                let folder = trimmed.rsplit(slash).next().unwrap_or(&trimmed).to_string();
                items.push(
                    palette::Item::new(format!("[add {folder}]"), move |view, _, cx| {
                        let workspace_id = view.workspace.clone();
                        let request = Request::AddProject { path: target.clone(), workspace: Some(workspace.clone()) };
                        send(workspace_id, key, request, cx);
                    })
                    .glyph(icons::ADD, None),
                );
            }
            if let Some((parent, _)) = trimmed.rsplit_once(slash) {
                let parent = if parent.is_empty() || parent.ends_with(':') {
                    format!("{parent}{slash}")
                } else {
                    parent.to_string()
                };
                items.push(
                    palette::Item::new("..", move |view, window, cx| browse(view, parent.clone(), window, cx))
                        .glyph(icons::CHEVRON_UP, None),
                );
            }
            for folder in folders {
                let target = format!("{trimmed}{slash}{folder}");
                items.push(
                    palette::Item::new(folder, move |view, window, cx| browse(view, target.clone(), window, cx))
                        .glyph(icons::FOLDER, None),
                );
            }
            let palette = cx.new(|cx| Palette::new("folders", &path, items, 0, cx));
            view.show_palette(palette, window, cx);
        });
    })
    .detach();
}

pub fn rename_workspace(view: &mut WorkspaceView, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let Some(key) = host(&view.workspace, cx).filter(|_| control(&view.workspace, cx)) else { return };
    let current = view.state(cx).map(|state| state.name.clone()).unwrap_or_default();
    actions::prompt(view, "rename workspace", &current.clone(), window, cx, move |name, view, _, cx| {
        if name.is_empty() || name == current {
            return;
        }
        let Some(uplink) = super::uplink(cx) else { return };
        let request = Request::RenameWorkspace { workspace: current.clone(), name: name.clone() };
        let answered = uplink.update(cx, |uplink, _| uplink.ask(key, request));
        let id = view.workspace.clone();
        cx.spawn(async move |_, cx| match answered.await {
            Ok(Response::Failed { reason }) => cx.update(|cx| actions::alert(&id, "Not renamed", reason, cx)),
            Ok(_) => cx.update(|cx| {
                store::store(cx).update(cx, |store, cx| {
                    if let Some(remote) = store.remotes.iter_mut().find(|remote| remote.id == id) {
                        remote.name = name.clone();
                    }
                    store.changed(cx);
                });
                if let Some(uplink) = super::uplink(cx) {
                    uplink.update(cx, |uplink, cx| uplink.sync_remotes(&key, cx));
                }
            }),
            Err(_) => {}
        })
        .detach();
    });
}

pub fn workspace_menu(view: &WorkspaceView, cx: &App) -> Vec<Item> {
    let mut items = vec![menu::command("Open Project…", "open_project", cx)];
    if control(&view.workspace, cx) {
        items.push(menu::command("Rename Workspace…", "rename_workspace", cx));
    }
    let Some(key) = host(&view.workspace, cx) else { return items };
    let name = view.state(cx).map(|state| state.name.clone()).unwrap_or_default();
    let muted = catalog(&key, cx)
        .and_then(|catalog| catalog.workspaces.into_iter().find(|workspace| workspace.name == name))
        .is_some_and(|workspace| workspace.muted);
    if control(&view.workspace, cx) {
        items.push(Item::Separator);
        items.push(menu::action(actions::notifications_label(!muted), move |view, _, cx| {
            let request = Request::MuteWorkspace { workspace: name.clone(), muted: !muted };
            send(view.workspace.clone(), key, request, cx);
        }));
    }
    items
}

pub fn conversations(
    view: &WorkspaceView,
    cx: &mut Context<WorkspaceView>,
) -> Option<Vec<super::protocol::RemoteConversation>> {
    let key = host(&view.workspace, cx)?;
    let uplink = super::uplink(cx)?;
    let (stale, list) = match uplink.read(cx).histories.get(&key) {
        Some((at, list)) => (at.elapsed() > std::time::Duration::from_secs(15), list.clone()),
        None => (true, None),
    };
    if stale {
        let answered = uplink.update(cx, |uplink, _| {
            uplink.histories.insert(key, (std::time::Instant::now(), list.clone()));
            uplink.ask(key, Request::History)
        });
        cx.spawn(async move |_, cx| {
            if let Ok(Response::History { conversations }) = answered.await {
                cx.update(|cx| {
                    if let Some(uplink) = super::uplink(cx) {
                        uplink.update(cx, |uplink, _| {
                            uplink.histories.insert(key, (std::time::Instant::now(), Some(conversations)));
                        });
                    }
                    store::store(cx).update(cx, |_, cx| cx.notify());
                });
            }
        })
        .detach();
    }
    let projects: Vec<String> = view
        .state(cx)
        .map(|state| state.all_projects().map(|project| project.path.to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    list.map(|list| list.into_iter().filter(|item| projects.contains(&item.project)).collect())
}

pub fn history_rows(view: &WorkspaceView, query: &str, cx: &mut Context<WorkspaceView>) -> Vec<gpui::AnyElement> {
    use gpui::{
        InteractiveElement, IntoElement, MouseButton, ParentElement, StatefulInteractiveElement, Styled, div, px,
    };
    let theme = crate::ui::theme(cx);
    let Some(list) = conversations(view, cx) else {
        return vec![div().p(px(10.0)).text_color(theme.faint).child("loading…").into_any_element()];
    };
    let key = host(&view.workspace, cx);
    let control = control(&view.workspace, cx);
    let mut rows = Vec::new();
    for (index, item) in list.into_iter().enumerate() {
        if !crate::sidebar::matches(query, &[&item.title, &item.cwd]) {
            continue;
        }
        let age = crate::history::relative_age(std::time::UNIX_EPOCH + std::time::Duration::from_secs(item.modified));
        let (glyph, color) = icons::agent(&item.agent);
        let (resume, menu_id, title) = (item.id.clone(), item.id.clone(), item.title.clone());
        rows.push(
            div()
                .id(("remote-history", index))
                .flex()
                .flex_col()
                .px(px(8.0))
                .py(px(3.0))
                .hover(|style| style.bg(theme.hover))
                .child(
                    div()
                        .flex()
                        .gap(px(6.0))
                        .child(div().text_color(color.color(theme.dark)).child(glyph))
                        .child(div().flex_1().truncate().child(item.title.clone())),
                )
                .child(
                    div()
                        .pl(px(20.0))
                        .truncate()
                        .text_color(theme.faint)
                        .child(format!("{} · {age} · {}", item.profile, item.cwd)),
                )
                .on_click(cx.listener(move |view, _, _, cx| {
                    if let Some(key) = key.filter(|_| control) {
                        send(view.workspace.clone(), key, Request::Resume { conversation: resume.clone() }, cx);
                    }
                }))
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |view, event: &gpui::MouseDownEvent, _, cx| {
                        let mut items = Vec::new();
                        if let Some(key) = key.filter(|_| control) {
                            let id = menu_id.clone();
                            items.push(menu::action("Resume", move |view, _, cx| {
                                send(view.workspace.clone(), key, Request::Resume { conversation: id.clone() }, cx)
                            }));
                            items.push(Item::Separator);
                        }
                        items.push(menu::copy("Copy Title", title.clone()));
                        items.push(menu::copy("Copy Conversation ID", menu_id.clone()));
                        view.open_menu(event.position, items, cx);
                    }),
                )
                .into_any_element(),
        );
    }
    if rows.is_empty() {
        rows.push(div().p(px(10.0)).text_color(theme.faint).child("no conversations").into_any_element());
    }
    rows
}

pub fn branches(view: &mut WorkspaceView, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let Some(root) = view.current_project_path(cx).filter(|path| !path.as_os_str().is_empty()) else { return };
    let backend = crate::backend::Backend::of(&view.workspace, &root, cx);
    let listed = backend.ask::<(Option<String>, Vec<String>)>(crate::backend::Query::Branches { limit: 30 }, cx);
    let control = control(&view.workspace, cx);
    cx.spawn_in(window, async move |this, cx| {
        let Some((current, list)) = listed.await else { return };
        let _ = this.update_in(cx, |view, window, cx| {
            let mut items = Vec::new();
            for branch in list {
                let (target, name) = (root.clone(), branch.clone());
                items.push(
                    palette::Item::new(branch.clone(), move |view, _, cx| {
                        if control {
                            project_action(
                                &view.workspace.clone(),
                                &target,
                                Action::Switch { branch: name.clone() },
                                cx,
                            );
                        }
                    })
                    .glyph(icons::BRANCH, None)
                    .detail(if Some(&branch) == current.as_ref() { "current" } else { "" }),
                );
            }
            if control {
                let target = root.clone();
                items.push(
                    palette::Item::new("New Branch…", move |view, window, cx| {
                        let target = target.clone();
                        actions::prompt(view, "new branch", "", window, cx, move |name, view, _, cx| {
                            if !name.is_empty() {
                                project_action(&view.workspace.clone(), &target, Action::Branch { name, at: None }, cx);
                            }
                        });
                    })
                    .glyph(icons::ADD, None),
                );
            }
            let palette = cx.new(|cx| Palette::new("branch", "switch branch", items, 0, cx));
            view.show_palette(palette, window, cx);
        });
    })
    .detach();
}

pub fn project_files(key: Key, root: &Path, cx: &mut App) -> Option<super::FileList> {
    let uplink = super::uplink(cx)?;
    let revision = store::store(cx).read(cx).revision(root);
    let (stale, files) = match uplink.read(cx).files.get(root) {
        Some((at, files)) => (*at != revision, files.clone()),
        None => (true, None),
    };
    if stale {
        let (root, kept) = (root.to_path_buf(), files.clone());
        uplink.update(cx, |uplink, _| uplink.files.insert(root.clone(), (revision, kept)));
        let backend = crate::backend::Backend::Remote { key, root: root.clone() };
        let asked = backend.ask::<Vec<String>>(crate::backend::Query::Files, cx);
        cx.spawn(async move |cx| {
            let Some(list) = asked.await else { return };
            cx.update(|cx| {
                if let Some(uplink) = super::uplink(cx) {
                    uplink.update(cx, |uplink, _| {
                        if let Some(entry) = uplink.files.get_mut(&root) {
                            entry.1 = Some(std::sync::Arc::new(list.into_iter().collect()));
                        }
                    });
                }
            });
        })
        .detach();
    }
    files
}
