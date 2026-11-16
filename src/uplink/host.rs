use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

use gpui::{App, Context};

use super::act;
use super::devices::Access;
use super::net::Outbox;
use super::protocol::{
    Action, LineKind, Message, RemoteChange, RemoteCommit, RemoteConversation, RemoteEntry, RemoteLine, Request,
    Response,
};
use super::wire::Key;
use super::{Uplink, project_key};
use crate::git::{self, GitRef};
use crate::{actions, history, settings, store};

const FILE_LIMIT: usize = 1048576;
const HISTORY_LIMIT: usize = 150;
const LOG_LIMIT: usize = 300;
const BRANCH_LIMIT: usize = 100;

fn failed(reason: impl Into<String>) -> Response {
    Response::Failed { reason: reason.into() }
}

fn shared_projects(cx: &App) -> Vec<(PathBuf, String)> {
    let store = store::store(cx).read(cx);
    let default = settings::get(cx).uplink.share;
    store
        .workspaces
        .iter()
        .filter(|workspace| !workspace.closed)
        .flat_map(|workspace| workspace.all_projects())
        .filter(|project| !project.is_scratch() && project.uplink.unwrap_or(default))
        .map(|project| (project.path.clone(), workspace_of(&project.path, cx)))
        .collect()
}

pub(super) fn workspace_of(path: &Path, cx: &App) -> String {
    let store = store::store(cx).read(cx);
    store
        .workspaces
        .iter()
        .filter(|workspace| !workspace.closed && store.windows.contains_key(&workspace.id))
        .find(|workspace| workspace.project(path).is_some())
        .or_else(|| {
            store.workspaces.iter().find(|workspace| !workspace.closed && store.windows.contains_key(&workspace.id))
        })
        .map(|workspace| workspace.id.clone())
        .unwrap_or_default()
}

pub(super) fn inside(root: &Path, relative: &str) -> Option<PathBuf> {
    let relative = Path::new(relative);
    if relative.components().any(|part| !matches!(part, Component::Normal(_))) {
        return None;
    }
    let path = root.join(relative);
    let resolved = path.canonicalize().ok()?;
    resolved.starts_with(root.canonicalize().ok()?).then_some(path)
}

fn conversation(item: history::Conversation, shared: &[(PathBuf, String)]) -> Option<RemoteConversation> {
    let (root, _) =
        shared.iter().filter(|(root, _)| item.cwd.starts_with(root)).max_by_key(|(root, _)| root.as_os_str().len())?;
    Some(RemoteConversation {
        id: item.id,
        title: item.title,
        modified: item.modified.duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs()),
        agent: item.kind.command().to_string(),
        profile: item.profile,
        cwd: item.cwd.to_string_lossy().into_owned(),
        project: project_key(root),
    })
}

fn change(change: git::Change) -> RemoteChange {
    RemoteChange {
        path: change.path,
        status: change.status.to_string(),
        staged: change.staged,
        added: change.added,
        removed: change.removed,
    }
}

fn reference(reference: GitRef) -> String {
    match reference {
        GitRef::Head => "HEAD".into(),
        GitRef::Current(name) => format!("HEAD -> {name}"),
        GitRef::Branch(name) => name,
        GitRef::Remote(name) => format!("remote: {name}"),
        GitRef::Tag(name) => format!("tag: {name}"),
    }
}

fn read_file(path: &Path) -> Response {
    let Ok(bytes) = std::fs::read(path) else { return failed("The file cannot be read.") };
    let binary = bytes.iter().take(8192).any(|byte| *byte == 0);
    let truncated = bytes.len() > FILE_LIMIT;
    let text = if binary {
        String::new()
    } else {
        String::from_utf8_lossy(&bytes[..bytes.len().min(FILE_LIMIT)]).into_owned()
    };
    Response::File { text, truncated, binary }
}

fn list_folder(path: &Path, folders_only: bool) -> Vec<RemoteEntry> {
    let Ok(read) = std::fs::read_dir(path) else { return Vec::new() };
    let mut entries: Vec<RemoteEntry> = read
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let folder = entry.path().is_dir();
            let hidden = name == ".git" || name == ".DS_Store" || (folders_only && name.starts_with('.'));
            (!hidden && (folder || !folders_only)).then_some(RemoteEntry { name, folder })
        })
        .collect();
    entries.sort_by(|a, b| b.folder.cmp(&a.folder).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    entries
}

fn answer(request: Request, root: PathBuf) -> Response {
    match request {
        Request::Files { folder, .. } => match inside(&root, &folder) {
            Some(path) => Response::Files { entries: list_folder(&path, false) },
            None => failed("This folder is outside the project."),
        },
        Request::File { path, .. } => match inside(&root, &path) {
            Some(path) => read_file(&path),
            None => failed("This file is outside the project."),
        },
        Request::Changes { .. } => match git::status(&root) {
            Some(status) => {
                Response::Changes { branch: status.branch, changes: status.changes.into_iter().map(change).collect() }
            }
            None => failed("This project is not a git repository."),
        },
        Request::Diff { path, commit, .. } => match git::file_diff(&root, &path, commit.as_deref()) {
            Some(lines) => Response::Diff {
                lines: lines
                    .into_iter()
                    .map(|line| RemoteLine {
                        kind: match line.kind {
                            git::LineKind::Context => LineKind::Context,
                            git::LineKind::Added => LineKind::Added,
                            git::LineKind::Removed => LineKind::Removed,
                        },
                        text: line.text,
                        old: line.old,
                        new: line.new,
                    })
                    .collect(),
            },
            None => failed("No changes to show."),
        },
        Request::Log { .. } => match git::log(&root, LOG_LIMIT) {
            Some(commits) => Response::Log {
                commits: commits
                    .into_iter()
                    .map(|commit| RemoteCommit {
                        hash: commit.hash,
                        parents: commit.parents,
                        refs: commit.refs.into_iter().map(reference).collect(),
                        author: commit.author,
                        time: commit.time,
                        subject: commit.subject,
                    })
                    .collect(),
            },
            None => failed("This project has no git history."),
        },
        Request::Commit { hash, .. } => match git::commit_details(&root, &hash) {
            Some(details) => Response::Commit {
                date: details.date,
                message: details.message,
                stats: details.stats,
                changes: git::commit_changes(&root, &hash).unwrap_or_default().into_iter().map(change).collect(),
            },
            None => failed("The commit was not found."),
        },
        Request::Branches { .. } => Response::Branches {
            current: git::status(&root).map(|status| status.branch).unwrap_or_default(),
            branches: git::branches(&root, BRANCH_LIMIT),
        },
        _ => failed("Unsupported request."),
    }
}

fn project_of(request: &Request) -> Option<&str> {
    match request {
        Request::Files { project, .. }
        | Request::File { project, .. }
        | Request::Changes { project }
        | Request::Diff { project, .. }
        | Request::Log { project }
        | Request::Commit { project, .. }
        | Request::Branches { project }
        | Request::Act { project, .. } => Some(project),
        _ => None,
    }
}

impl Uplink {
    pub(super) fn host_request(&mut self, key: Key, id: u64, request: Request, cx: &mut Context<Self>) {
        let Some(outbox) = self.links.get(&key).map(|link| link.outbox.clone()) else { return };
        let Some(device) = self.device(&key).filter(|device| !device.blocked) else { return };
        let control = device.access == Access::Control;
        let reply = move |outbox: &Outbox, response: Response| {
            outbox.send(&Message::Response { id, response });
        };
        let shared = shared_projects(cx);
        let root_of =
            |project: &str| shared.iter().find(|(root, _)| project_key(root) == project).map(|(root, _)| root.clone());
        if let Some(project) = project_of(&request) {
            let Some(root) = root_of(project) else {
                reply(&outbox, failed("This project is not shared."));
                return;
            };
            if let Request::Act { action, .. } = request {
                if !control {
                    reply(&outbox, failed("This computer lets this device view only."));
                    return;
                }
                let source = match &action {
                    Action::Paste { source, .. } => root_of(source),
                    _ => None,
                };
                act::act(outbox, id, root, source, action, cx);
                return;
            }
            cx.background_executor().spawn(async move { reply(&outbox, answer(request, root)) }).detach();
            return;
        }
        match request {
            Request::History => {
                let sources = history::sources(&settings::get(cx).agents);
                cx.background_executor()
                    .spawn(async move {
                        let conversations = history::all(&sources, HISTORY_LIMIT)
                            .into_iter()
                            .filter_map(|item| conversation(item, &shared))
                            .collect();
                        reply(&outbox, Response::History { conversations });
                    })
                    .detach();
            }
            _ if !control => reply(&outbox, failed("This computer lets this device view only.")),
            Request::Tab { tab, action } => act::tab(&outbox, id, tab, action, cx),
            Request::Folders { path } => {
                let home = dirs::home_dir().unwrap_or_default();
                let folder = if path.is_empty() { home } else { PathBuf::from(path) };
                cx.background_executor()
                    .spawn(async move {
                        let folders = list_folder(&folder, true).into_iter().map(|entry| entry.name).collect();
                        reply(&outbox, Response::Folders { path: folder.to_string_lossy().into_owned(), folders });
                    })
                    .detach();
            }
            Request::AddProject { path } => {
                let path = PathBuf::from(path);
                if !path.is_dir() {
                    reply(&outbox, failed("This folder does not exist."));
                    return;
                }
                let workspace = workspace_of(&path, cx);
                actions::in_window(&workspace, cx, move |view, window, cx| {
                    actions::open_folder(view, path, window, cx);
                    reply(&outbox, Response::Done);
                });
            }
            Request::Resume { conversation } => {
                let sources = history::sources(&settings::get(cx).agents);
                cx.spawn(async move |_, cx| {
                    let found = cx
                        .background_executor()
                        .spawn(async move {
                            history::all(&sources, HISTORY_LIMIT).into_iter().find(|item| item.id == conversation)
                        })
                        .await;
                    let Some(found) = found.filter(|item| shared.iter().any(|(root, _)| item.cwd.starts_with(root)))
                    else {
                        reply(&outbox, failed("This conversation is not in a shared project."));
                        return;
                    };
                    cx.update(|cx| {
                        let workspace = workspace_of(&found.cwd, cx);
                        actions::in_window(&workspace, cx, move |view, window, cx| {
                            actions::resume_anywhere(view, found, window, cx);
                            reply(&outbox, Response::Done);
                        });
                    });
                })
                .detach();
            }
            _ => reply(&outbox, failed("Unsupported request.")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_requests_inside_the_project() {
        let base = std::env::temp_dir().join(format!("nucli-host-{}", uuid::Uuid::new_v4()));
        let root = base.join("project");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}").unwrap();
        std::fs::write(base.join("secret"), "x").unwrap();
        assert!(inside(&root, "src/main.rs").is_some());
        assert!(inside(&root, "").is_some());
        assert!(inside(&root, "../secret").is_none());
        assert!(inside(&root, "src/../../secret").is_none());
        assert!(inside(&root, base.join("secret").to_str().unwrap()).is_none());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(base.join("secret"), root.join("link")).unwrap();
            assert!(inside(&root, "link").is_none());
        }
        let _ = std::fs::remove_dir_all(base);
    }
}
