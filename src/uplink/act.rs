use std::path::{Component, Path, PathBuf};

use gpui::App;

use super::host::{inside, workspace_of};
use super::net::Outbox;
use super::protocol::{Action, Message, Reset, Response, TabAction};
use crate::window::Rename;
use crate::{actions, changes, file_ops, file_view, git, store};

enum Moved {
    Nothing,
    Path(String, Option<String>),
}

fn reply(outbox: &Outbox, id: u64, result: Result<(), String>) {
    let response = match result {
        Ok(()) => Response::Done,
        Err(reason) => Response::Failed { reason },
    };
    outbox.send(&Message::Response { id, response });
}

pub(super) fn target(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    if relative.is_empty() || path.components().any(|part| !matches!(part, Component::Normal(_))) {
        return Err("This path is outside the project.".into());
    }
    let parent = path.parent().map(|parent| parent.to_string_lossy().into_owned()).unwrap_or_default();
    let mut existing = Path::new(&parent);
    while !root.join(existing).exists() {
        existing = existing.parent().unwrap_or(Path::new(""));
    }
    inside(root, &existing.to_string_lossy()).ok_or("This path is outside the project.")?;
    Ok(root.join(path))
}

fn existing(root: &Path, relative: &str) -> Result<PathBuf, String> {
    inside(root, relative).filter(|_| !relative.is_empty()).ok_or_else(|| format!("{relative} was not found."))
}

fn name(text: &str) -> Result<&str, String> {
    let text = text.trim();
    if text.is_empty() || text.starts_with('-') || text.chars().any(char::is_whitespace) {
        return Err(format!("\"{text}\" is not a valid name."));
    }
    Ok(text)
}

pub(super) fn hash(text: &str) -> Result<&str, String> {
    if text.is_empty() || !text.chars().all(|character| character.is_ascii_hexdigit()) {
        return Err("This is not a commit.".into());
    }
    Ok(text)
}

fn git(root: &Path, args: &[&str]) -> Result<(), String> {
    git::run_checked(root, args).map(drop)
}

fn merge(root: &Path, hash: &str) -> bool {
    git::run(root, &["rev-list", "--parents", "-n", "1", hash]).is_some_and(|line| line.split_whitespace().count() > 2)
}

fn apply(root: &Path, command: &[&str], commit: &str) -> Result<(), String> {
    let commit = hash(commit)?;
    let mainline: &[&str] = if merge(root, commit) { &["-m", "1"] } else { &[] };
    git(root, &[command, mainline, &[commit]].concat())
}

fn files(root: &Path, source: Option<PathBuf>, action: Action) -> Result<Moved, String> {
    let error = |error: std::io::Error| error.to_string();
    match action {
        Action::Create { path, folder } => {
            file_ops::create_path(&target(root, &path)?, &path, folder).map_err(error)?;
            Ok(Moved::Nothing)
        }
        Action::Rename { from, to } => {
            let to = to.trim_matches('/').to_string();
            let (source, destination) = (existing(root, &from)?, target(root, &to)?);
            if !from.eq_ignore_ascii_case(&to) && destination.symlink_metadata().is_ok() {
                return Err(file_ops::exists_error(&to).to_string());
            }
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent).map_err(error)?;
            }
            file_ops::move_path(&source, &destination).map_err(error)?;
            Ok(Moved::Path(from, Some(to)))
        }
        Action::Duplicate { path } => {
            let source = existing(root, &path)?;
            file_ops::copy_all(&source, &file_ops::free_name(&source)).map_err(error)?;
            Ok(Moved::Nothing)
        }
        Action::Delete { path } => {
            trash::delete(existing(root, &path)?).map_err(|error| error.to_string())?;
            Ok(Moved::Path(path, None))
        }
        Action::Write { path, text } => {
            std::fs::write(existing(root, &path)?, text).map_err(error)?;
            Ok(Moved::Nothing)
        }
        Action::Replace { path, request, hits } => {
            let request: crate::search::Request = serde_json::from_value(request).map_err(|error| error.to_string())?;
            let hits: Vec<crate::find::Hit> = serde_json::from_value(hits).map_err(|error| error.to_string())?;
            crate::search::replace(&existing(root, &path)?, &request, &hits)?;
            Ok(Moved::Nothing)
        }
        Action::Paste { from, folder, cut, .. } => {
            let source_root = source.ok_or("The copied file is not in a shared project.")?;
            let source = existing(&source_root, &from)?;
            let folder = if folder.is_empty() { root.to_path_buf() } else { existing(root, &folder)? };
            let name = source.file_name().ok_or("Nothing to paste.")?;
            if folder.starts_with(&source) {
                return Err("A folder cannot go inside itself.".into());
            }
            let destination = file_ops::free_name(&folder.join(name));
            if cut {
                file_ops::move_path(&source, &destination).map_err(error)?;
                let to = (source_root == root).then(|| file_ops::relative(root, &destination)).flatten();
                return Ok(if source_root == root { Moved::Path(from, to) } else { Moved::Nothing });
            }
            file_ops::copy_all(&source, &destination).map_err(error)?;
            Ok(Moved::Nothing)
        }
        _ => Err("Unsupported action.".into()),
    }
}

fn changes_of(root: &Path, paths: &[String], staged: bool) -> Vec<git::Change> {
    git::status(root)
        .map(|status| status.changes)
        .unwrap_or_default()
        .into_iter()
        .filter(|change| change.staged == staged && paths.contains(&change.path))
        .collect()
}

fn repository(root: &Path, action: Action) -> Result<(), String> {
    let paths_ok = |paths: &[String]| {
        paths.iter().all(|path| Path::new(path).components().all(|part| matches!(part, Component::Normal(_))))
    };
    match action {
        Action::Stage { paths } if paths_ok(&paths) => git::stage(root, &paths),
        Action::Unstage { paths } if paths_ok(&paths) => git::unstage(root, &paths),
        Action::Discard { paths } if paths_ok(&paths) => changes::discard(root, &changes_of(root, &paths, false)),
        Action::Commit { message } if !message.trim().is_empty() => git::commit(root, message.trim()),
        Action::Switch { branch } => git::switch(root, name(&branch)?),
        Action::Track { remote } => git(root, &["switch", "--track", name(&remote)?]),
        Action::Detach { hash: commit } => git(root, &["switch", "--detach", hash(&commit)?]),
        Action::Branch { name: branch, at: Some(commit) } => {
            git(root, &["switch", "-c", name(&branch)?, hash(&commit)?])
        }
        Action::Branch { name: branch, at: None } => git::create_branch(root, name(&branch)?),
        Action::Tag { name: tag, at } => git(root, &["tag", name(&tag)?, hash(&at)?]),
        Action::CherryPick { hash: commit } => apply(root, &["cherry-pick"], &commit),
        Action::Revert { hash: commit } => apply(root, &["revert", "--no-edit"], &commit),
        Action::Reset { hash: commit, mode } => {
            let mode = match mode {
                Reset::Soft => "--soft",
                Reset::Mixed => "--mixed",
                Reset::Hard => "--hard",
            };
            git(root, &["reset", mode, hash(&commit)?])
        }
        Action::DeleteBranch { name: branch } => git(root, &["branch", "-D", name(&branch)?]),
        Action::DeleteTag { name: tag } => git(root, &["tag", "-d", name(&tag)?]),
        Action::Restore { hash: commit, path } => {
            target(root, &path)?;
            git(root, &["checkout", hash(&commit)?, "--", &path])
        }
        _ => Err("This action is not valid.".into()),
    }
}

fn project(root: &Path, action: Action, cx: &mut App) -> Result<(), String> {
    let workspace = workspace_of(root, cx);
    match action {
        Action::Open { path } => cx.open_with_system(&inside(root, &path).ok_or("This path was not found.")?),
        Action::Reveal { path } => cx.reveal_path(&inside(root, &path).ok_or("This path was not found.")?),
        Action::RenameProject { name } => actions::rename(&workspace, &Rename::Project(root.to_path_buf()), &name, cx),
        Action::ColorProject { color } => store::store(cx).update(cx, |store, cx| {
            if let Some(project) = store.workspace_mut(&workspace).and_then(|state| state.project_mut(root)) {
                project.color = color;
            }
            store.changed(cx);
        }),
        Action::MuteProject { muted } => actions::set_project_notifications(&workspace, root, !muted, cx),
        Action::MoveProject { workspace: name } => {
            let name = name.trim().to_string();
            if name.is_empty() {
                return Err("The workspace needs a name.".into());
            }
            let found =
                store::store(cx).read(cx).workspaces.iter().find(|item| item.name == name).map(|item| item.id.clone());
            let id = found.unwrap_or_else(|| {
                store::store(cx).update(cx, |store, cx| {
                    let created = store::Workspace::new(&name);
                    let id = created.id.clone();
                    store.workspaces.push(created);
                    store.changed(cx);
                    id
                })
            });
            if id != workspace {
                let root = root.to_path_buf();
                actions::in_window(&workspace, cx, move |view, window, cx| {
                    actions::move_project_to_workspace(view, &root, Some(id), window, cx)
                });
            }
        }
        Action::CloseProject => actions::remove_project(&workspace, root, cx),
        _ => return Err("Unsupported action.".into()),
    }
    Ok(())
}

pub fn act(outbox: Outbox, id: u64, root: PathBuf, source: Option<PathBuf>, action: Action, cx: &mut App) {
    match action {
        Action::Create { .. }
        | Action::Rename { .. }
        | Action::Duplicate { .. }
        | Action::Delete { .. }
        | Action::Write { .. }
        | Action::Replace { .. }
        | Action::Paste { .. } => {
            let task = cx.background_executor().spawn({
                let root = root.clone();
                async move { files(&root, source, action) }
            });
            cx.spawn(async move |cx| {
                let result = task.await;
                if let Ok(Moved::Path(from, to)) = &result {
                    let (from, to) = (from.clone(), to.clone());
                    cx.update(|cx| {
                        let workspace = workspace_of(&root, cx);
                        actions::in_window(&workspace, cx, move |view, window, cx| {
                            file_view::path_moved(view, &root, &from, to.as_deref(), window, cx)
                        });
                    });
                }
                reply(&outbox, id, result.map(drop));
            })
            .detach();
        }
        Action::Open { .. }
        | Action::Reveal { .. }
        | Action::RenameProject { .. }
        | Action::ColorProject { .. }
        | Action::MoveProject { .. }
        | Action::MuteProject { .. }
        | Action::CloseProject => reply(&outbox, id, project(&root, action, cx)),
        action => {
            cx.background_executor().spawn(async move { reply(&outbox, id, repository(&root, action)) }).detach();
        }
    }
}

pub fn tab(outbox: &Outbox, id: u64, tab: String, action: TabAction, cx: &mut App) {
    if !super::shared(&tab, cx) {
        reply(outbox, id, Err("This tab is not shared.".into()));
        return;
    }
    match action {
        TabAction::Rename { title } => actions::rename("", &Rename::Tab(tab), &title, cx),
        TabAction::Color { color } => {
            if let Some(session) = store::store(cx).read(cx).session(&tab).cloned() {
                session.update(cx, |session, cx| {
                    session.color = color;
                    cx.notify();
                });
            }
            store::store(cx).update(cx, |store, cx| store.changed(cx));
        }
        TabAction::MoveOut => {
            let store = store::store(cx).read(cx);
            let workspace = store
                .workspaces
                .iter()
                .find(|workspace| workspace.project_of(&tab).is_some())
                .map(|workspace| workspace.id.clone())
                .unwrap_or_default();
            actions::in_window(&workspace, cx, move |view, window, cx| {
                actions::move_tab(view, &tab, Path::new(""), None, window, cx)
            });
        }
        TabAction::Mute { muted } => actions::set_tab_notifications(&tab, !muted, cx),
        TabAction::Close => store::store(cx).update(cx, |store, cx| store.close_tab(&tab, cx)),
    }
    reply(outbox, id, Ok(()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_actions_inside_the_project() {
        let base = std::env::temp_dir().join(format!("nucli-act-{}", uuid::Uuid::new_v4()));
        let root = base.join("project");
        std::fs::create_dir_all(root.join("src")).unwrap();
        assert!(target(&root, "src/new/file.rs").is_ok());
        assert!(target(&root, "new.txt").is_ok());
        for bad in ["", "../x", "src/../../x", "/etc/passwd"] {
            assert!(target(&root, bad).is_err(), "{bad}");
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&base, root.join("out")).unwrap();
            assert!(target(&root, "out/escape.txt").is_err());
        }
        assert!(name("feature/x").is_ok());
        assert!(name("--upload-pack=x").is_err() && name("a b").is_err() && name(" ").is_err());
        assert!(hash("a1b2c3").is_ok() && hash("HEAD~1").is_err() && hash("-x").is_err());
        let _ = std::fs::remove_dir_all(base);
    }
}
