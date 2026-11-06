use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use futures::StreamExt;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use gpui::Context;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

use crate::store::Store;

fn relevant(path: &Path) -> bool {
    let text = path.to_string_lossy().replace('\\', "/");
    if text.ends_with(".lock") {
        return false;
    }
    match text.split_once("/.git/") {
        Some((_, rest)) => rest == "index" || rest == "HEAD" || rest.starts_with("refs/") || rest == "packed-refs",
        None => {
            !text.contains("/node_modules/") && !text.contains("/target/debug/") && !text.contains("/target/release/")
        }
    }
}

fn outside_git_dir(project: &Path) -> Option<PathBuf> {
    let mut current = project.to_path_buf();
    let mut first = true;
    loop {
        let dot_git = current.join(".git");
        if dot_git.is_dir() {
            return if first { None } else { Some(dot_git) };
        }
        if dot_git.is_file() {
            let text = std::fs::read_to_string(&dot_git).ok()?;
            let target = text.trim().strip_prefix("gitdir:")?.trim().to_string();
            let target = PathBuf::from(target);
            return Some(if target.is_absolute() { target } else { current.join(target) });
        }
        first = false;
        if !current.pop() {
            return None;
        }
    }
}

fn watch(project: &Path, sender: UnboundedSender<PathBuf>) -> Option<RecommendedWatcher> {
    let root = project.to_path_buf();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if let Ok(event) = event
            && event.paths.iter().any(|path| relevant(path))
        {
            let _ = sender.unbounded_send(root.clone());
        }
    })
    .ok()?;
    watcher.watch(project, RecursiveMode::Recursive).ok()?;
    if let Some(git) = outside_git_dir(project) {
        let _ = watcher.watch(&git, RecursiveMode::Recursive);
    }
    Some(watcher)
}

#[derive(Default)]
pub struct Watchers {
    active: HashMap<PathBuf, RecommendedWatcher>,
    sender: Option<UnboundedSender<PathBuf>>,
}

impl Store {
    pub fn start_watching(&mut self, cx: &mut Context<Self>) {
        let (sender, mut receiver) = unbounded::<PathBuf>();
        self.watchers.sender = Some(sender);
        cx.spawn(async move |this, cx| {
            while let Some(first) = receiver.next().await {
                let mut changed = HashSet::from([first]);
                cx.background_executor().timer(Duration::from_millis(250)).await;
                while let Ok(path) = receiver.try_recv() {
                    changed.insert(path);
                }
                let result = this.update(cx, |store, cx| {
                    for path in &changed {
                        *store.revisions.entry(path.clone()).or_insert(0) += 1;
                    }
                    store.refresh_diffstats(changed.into_iter().collect(), cx);
                    cx.notify();
                });
                if result.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    pub fn sync_watchers(&mut self) {
        let wanted: HashSet<PathBuf> = self
            .workspaces
            .iter()
            .filter(|workspace| self.windows.contains_key(&workspace.id))
            .flat_map(|workspace| workspace.projects.iter().map(|project| project.path.clone()))
            .collect();
        self.watchers.active.retain(|path, _| wanted.contains(path));
        let Some(sender) = self.watchers.sender.clone() else { return };
        for path in wanted {
            if !self.watchers.active.contains_key(&path)
                && let Some(watcher) = watch(&path, sender.clone())
            {
                self.watchers.active.insert(path, watcher);
            }
        }
    }

    pub fn revision(&self, path: &Path) -> u64 {
        self.revisions.get(path).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_events() {
        assert!(relevant(Path::new("/p/src/main.rs")));
        assert!(relevant(Path::new("/p/.git/index")));
        assert!(relevant(Path::new("/p/.git/refs/heads/main")));
        assert!(!relevant(Path::new("/p/.git/objects/ab/cd")));
        assert!(!relevant(Path::new("/p/.git/index.lock")));
        assert!(!relevant(Path::new("/p/node_modules/x/y.js")));
    }
}
