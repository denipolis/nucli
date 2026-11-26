use std::collections::HashSet;
use std::path::PathBuf;

use gpui::Context;

use super::protocol::Catalog;
use super::wire::{self, Key};
use super::{Uplink, mirror_id, valid_tab};
use crate::persist::Selected;
use crate::store::{self, Host, Project, Workspace};

fn host_of(workspace: &Workspace, key: &str) -> bool {
    workspace.host.as_ref().is_some_and(|host| host.key == key)
}

fn arrange(target: &mut Project, catalog: &Catalog, key: &Key, workspace: &str, path: &str) {
    let ids = |panel: bool| -> Vec<String> {
        catalog
            .tabs
            .iter()
            .filter(|tab| tab.workspace == workspace && tab.project == path && tab.panel == panel)
            .map(|tab| mirror_id(key, &tab.id))
            .collect()
    };
    target.tabs = ids(false);
    target.terminals = ids(true);
    if target.active_tab.as_ref().is_some_and(|tab| !target.tabs.contains(tab)) {
        target.active_tab = target.tabs.first().cloned();
    }
    if target.active_terminal.as_ref().is_none_or(|tab| !target.terminals.contains(tab)) {
        target.active_terminal = target.terminals.first().cloned();
    }
    if target.terminals.is_empty() {
        target.terminal_shown = false;
    }
}

fn mirror(remote: &mut Workspace, catalog: &Catalog, key: &Key) {
    let name = remote.name.clone();
    let place = match &remote.selected {
        Some(Selected::Tab(tab)) => remote.project_of(tab).map(|project| {
            let index = project.tabs.iter().position(|item| item == tab).unwrap_or(0);
            (project.path.clone(), index)
        }),
        _ => None,
    };
    let mut previous = std::mem::take(&mut remote.projects);
    for item in catalog.projects.iter().filter(|project| project.workspace == name) {
        if item.name.is_empty() {
            remote.scratch.color = item.color;
            arrange(&mut remote.scratch, catalog, key, &name, &item.path);
            continue;
        }
        let path = PathBuf::from(&item.path);
        let mut project = match previous.iter().position(|project| project.path == path) {
            Some(index) => previous.swap_remove(index),
            None => Project::new(path),
        };
        project.name = Some(item.name.clone());
        project.color = item.color;
        arrange(&mut project, catalog, key, &name, &item.path);
        remote.projects.push(project);
    }
    if !catalog.projects.iter().any(|project| project.workspace == name && project.name.is_empty()) {
        remote.scratch = Project::default();
    }
    let tabs: HashSet<String> =
        remote.all_projects().flat_map(|project| project.tabs.iter().chain(&project.terminals)).cloned().collect();
    let gone = match &remote.selected {
        Some(Selected::Tab(tab)) => !tabs.contains(tab),
        Some(Selected::Project(path)) => !path.as_os_str().is_empty() && remote.project(path).is_none(),
        None => false,
    };
    if gone {
        remote.selected = place.and_then(|(path, index)| {
            let project = remote.project(&path)?;
            let next =
                project.tabs.get(index).or_else(|| project.tabs.get(index.wrapping_sub(1))).or(project.tabs.last());
            match next {
                Some(tab) => Some(Selected::Tab(tab.clone())),
                None => (!project.is_scratch()).then(|| Selected::Project(path.clone())),
            }
        });
    }
    remote.mru.retain(|tab| tabs.contains(tab));
}

impl Uplink {
    pub fn open_remote(&mut self, key: Key, name: &str, cx: &mut Context<Self>) -> String {
        let encoded = wire::encode_key(&key);
        let device = self.device_name(&key);
        let id = store::store(cx).update(cx, |store, cx| {
            let existing = store.remotes.iter_mut().find(|remote| host_of(remote, &encoded) && remote.name == name);
            let id = match existing {
                Some(remote) => {
                    remote.closed = false;
                    remote.id.clone()
                }
                None => {
                    let remote = Workspace { host: Some(Host { key: encoded, device }), ..Workspace::new(name) };
                    let id = remote.id.clone();
                    store.remotes.push(remote);
                    id
                }
            };
            store.changed(cx);
            id
        });
        self.sync_remotes(&key, cx);
        id
    }

    pub fn sync_remotes(&mut self, key: &Key, cx: &mut Context<Self>) {
        let encoded = wire::encode_key(key);
        let Some(catalog) = self.links.get(key).and_then(|link| link.catalog.clone()) else { return };
        let names: HashSet<String> = store::store(cx)
            .read(cx)
            .remotes
            .iter()
            .filter(|remote| !remote.closed && host_of(remote, &encoded))
            .map(|remote| remote.name.clone())
            .collect();
        let wanted: Vec<String> = catalog
            .tabs
            .iter()
            .filter(|tab| names.contains(&tab.workspace) && valid_tab(&tab.id))
            .map(|tab| tab.id.clone())
            .collect();
        for tab in &wanted {
            self.ensure_mirror(*key, tab, cx);
        }
        let keep: HashSet<String> = wanted.iter().map(|tab| mirror_id(key, tab)).collect();
        let stale: Vec<String> = self
            .mirrors
            .iter()
            .filter(|(id, target)| target.key == *key && !keep.contains(*id))
            .map(|(id, _)| id.clone())
            .collect();
        for id in stale {
            self.close_mirror(&id, cx);
        }
        store::store(cx).update(cx, |store, cx| {
            for remote in store.remotes.iter_mut().filter(|remote| !remote.closed && host_of(remote, &encoded)) {
                mirror(remote, &catalog, key);
            }
            for project in catalog.projects.iter().filter(|project| names.contains(&project.workspace)) {
                let path = PathBuf::from(&project.path);
                if store.revision(&path) != project.revision {
                    store.revisions.insert(path.clone(), project.revision);
                    store.git_revisions.insert(path, project.revision);
                }
            }
            store.changed(cx);
        });
    }
}
