use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use gpui::{App, Task};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::uplink::protocol::{Action, Request, Response};
use crate::uplink::wire::Key;

const QUERY_PROTOCOL: u32 = 2;
pub const WRITE_PROTOCOL: u32 = 3;

#[derive(Clone, Debug, PartialEq)]
pub enum Backend {
    Local(PathBuf),
    Remote { key: Key, root: PathBuf },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "query", rename_all = "snake_case")]
pub enum Query {
    List { expanded: Vec<String> },
    Decorations,
    Status,
    Log { limit: usize },
    CommitChanges { hash: String },
    CommitDetails { hash: String },
    Files,
    Branches { limit: usize },
    Search { request: crate::search::Request },
    Load { path: String, commit: Option<String> },
}

fn hash(text: &str) -> Result<&str, String> {
    let valid = !text.is_empty() && text.len() <= 64 && text.chars().all(|char| char.is_ascii_hexdigit());
    valid.then_some(text).ok_or_else(|| "Not a commit hash.".to_string())
}

fn json(value: impl Serialize) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|error| error.to_string())
}

pub fn compute(root: &Path, query: Query) -> Result<Value, String> {
    match query {
        Query::List { expanded } => {
            let inside: BTreeSet<String> =
                expanded.into_iter().filter(|folder| crate::uplink::host::inside(root, folder).is_some()).collect();
            json(crate::files::list(root, &inside))
        }
        Query::Decorations => json(crate::files::decorations(root)),
        Query::Status => json(crate::git::status(root)),
        Query::Log { limit } => {
            let commits = crate::git::log(root, limit.min(2000));
            let graph = commits.as_ref().map(|commits| crate::git::graph(commits)).unwrap_or_default();
            json((commits, graph))
        }
        Query::CommitChanges { hash: text } => json(crate::git::commit_changes(root, hash(&text)?)),
        Query::CommitDetails { hash: text } => json(crate::git::commit_details(root, hash(&text)?)),
        Query::Files => json(crate::palettes::list_files(root)),
        Query::Branches { limit } => {
            json((crate::git::current_branch(root), crate::git::branches(root, limit.min(500))))
        }
        Query::Search { request } => json(crate::search::search(root, &request, &|| false)),
        Query::Load { path, commit } => {
            if Path::new(&path).is_absolute() || crate::uplink::host::inside(root, &path).is_none() {
                return Err("This file is outside the project.".into());
            }
            let commit = commit.map(|text| hash(&text).map(String::from)).transpose()?;
            json(crate::editor::load(root, &path, commit.as_deref()))
        }
    }
}

pub fn answer(root: &Path, query: Value) -> Result<Value, String> {
    let query: Query =
        serde_json::from_value(query).map_err(|_| "This computer does not know that query.".to_string())?;
    compute(root, query)
}

impl Backend {
    pub fn of(workspace: &str, root: &Path, cx: &App) -> Backend {
        match crate::uplink::remote_actions::host(workspace, cx) {
            Some(key) => Backend::Remote { key, root: root.to_path_buf() },
            None => Backend::Local(root.to_path_buf()),
        }
    }

    pub fn is_remote(&self) -> bool {
        matches!(self, Backend::Remote { .. })
    }

    pub fn writable(&self, cx: &App) -> bool {
        match self {
            Backend::Local(_) => true,
            Backend::Remote { key, .. } => {
                crate::uplink::uplink(cx).is_some_and(|uplink| uplink.read(cx).protocol(key) >= WRITE_PROTOCOL)
                    && crate::uplink::remote_actions::catalog(key, cx).is_some_and(|catalog| catalog.control)
            }
        }
    }

    pub fn act(&self, action: Action, cx: &mut App) -> Task<Result<(), String>> {
        let Backend::Remote { key, root } = self else { return Task::ready(Err("Not a remote project.".into())) };
        let Some(uplink) = crate::uplink::uplink(cx) else { return Task::ready(Err("Uplink is off.".into())) };
        let project = root.to_string_lossy().into_owned();
        let answered = uplink.update(cx, |uplink, _| uplink.ask(*key, Request::Act { project, action }));
        cx.background_executor().spawn(async move {
            match answered.await {
                Ok(Response::Failed { reason }) => Err(reason),
                Ok(_) => Ok(()),
                Err(_) => Err("The other computer did not answer.".into()),
            }
        })
    }

    pub fn ask<T: DeserializeOwned + Send + 'static>(&self, query: Query, cx: &mut App) -> Task<Option<T>> {
        match self {
            Backend::Local(root) => {
                let root = root.clone();
                cx.background_executor().spawn(async move {
                    compute(&root, query).ok().and_then(|value| serde_json::from_value(value).ok())
                })
            }
            Backend::Remote { key, root } => {
                let Some(uplink) = crate::uplink::uplink(cx) else { return Task::ready(None) };
                if uplink.read(cx).protocol(key) < QUERY_PROTOCOL {
                    return Task::ready(None);
                }
                let Ok(query) = serde_json::to_value(&query) else { return Task::ready(None) };
                let project = root.to_string_lossy().into_owned();
                let answered = uplink.update(cx, |uplink, _| uplink.ask(*key, Request::Query { project, query }));
                cx.background_executor().spawn(async move {
                    match answered.await {
                        Ok(Response::Answer { value }) => serde_json::from_value(value).ok(),
                        _ => None,
                    }
                })
            }
        }
    }
}
