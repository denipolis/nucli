use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use futures::channel::{mpsc, oneshot};
use gpui::{App, Context};

use super::Uplink;
use super::host::resolved;
use super::net::Outbox;
use super::protocol::{Message, Request, Response};
use super::transfer::{self, Piece};
use super::wire::Key;

const TREE_LIMIT: usize = 20000;

#[derive(Default)]
pub struct Transfers {
    uploads: HashMap<(Key, u64), mpsc::UnboundedSender<Piece>>,
    own: HashMap<u64, Own>,
    next: u64,
}

struct Own {
    key: Key,
    sink: Option<mpsc::UnboundedSender<Piece>>,
    stop: Arc<AtomicBool>,
    answer: oneshot::Sender<Response>,
}

fn failed(reason: impl Into<String>) -> Response {
    Response::Failed { reason: reason.into() }
}

fn reply(outbox: &Outbox, id: u64, response: Response) {
    outbox.send(&Message::Response { id, response });
}

fn walk(root: &Path, folder: &Path, files: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(folder) else { return };
    for entry in entries.flatten() {
        if files.len() >= TREE_LIMIT {
            return;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        if path.is_dir() && !path.is_symlink() {
            walk(root, &path, files);
        } else if let Some(relative) = crate::file_ops::relative(root, &path) {
            files.push(relative);
        }
    }
}

pub fn tree(root: &Path) -> Vec<String> {
    let mut files = crate::git::files(root).unwrap_or_else(|| {
        let mut files = Vec::new();
        walk(root, root, &mut files);
        files.sort();
        files
    });
    files.truncate(TREE_LIMIT);
    files
}

pub fn serve(outbox: Outbox, id: u64, root: PathBuf, path: String, cx: &mut App) {
    let executor = cx.background_executor().clone();
    cx.background_executor()
        .spawn(async move {
            let Some(file) = resolved(&root, &path).filter(|file| file.is_file()) else {
                reply(&outbox, id, failed("This file was not found in the project."));
                return;
            };
            let size = file.metadata().map(|metadata| metadata.len()).unwrap_or(0);
            outbox.send(&Message::Sending { id, size });
            let response = match transfer::send(&outbox, id, &file, &AtomicBool::new(false), &executor).await {
                Ok(()) => Response::Done,
                Err(reason) => failed(reason),
            };
            reply(&outbox, id, response);
        })
        .detach();
}

impl Uplink {
    pub(super) fn accept_upload(
        &mut self,
        key: Key,
        outbox: Outbox,
        id: u64,
        root: PathBuf,
        request: Request,
        cx: &mut Context<Self>,
    ) {
        let Request::Upload { folder, name, size, .. } = request else { return };
        let Some(folder) = resolved(&root, &folder).filter(|folder| folder.is_dir()) else {
            reply(&outbox, id, failed("This folder was not found in the project."));
            return;
        };
        if !transfer::valid_name(&name) {
            reply(&outbox, id, failed(format!("\"{name}\" is not a valid file name.")));
            return;
        }
        let (sink, finished) = transfer::receive(folder.join(&name), false, size, cx.background_executor());
        self.transfers.uploads.insert((key, id), sink);
        cx.background_executor()
            .spawn(async move {
                let response = match finished.await {
                    Ok(Ok(_)) => Response::Done,
                    Ok(Err(reason)) => failed(reason),
                    Err(_) => failed("The transfer stopped."),
                };
                reply(&outbox, id, response);
            })
            .detach();
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn accept_attach(
        &mut self,
        key: Key,
        outbox: Outbox,
        id: u64,
        tab: &str,
        name: String,
        size: u64,
        cx: &mut Context<Self>,
    ) {
        if !super::shared(tab, cx) {
            reply(&outbox, id, failed("This tab is not shared."));
            return;
        }
        if !transfer::valid_name(&name) {
            reply(&outbox, id, failed(format!("\"{name}\" is not a valid file name.")));
            return;
        }
        let folder = std::env::temp_dir().join("nucli-uplink").join(uuid::Uuid::new_v4().to_string());
        let (sink, finished) = transfer::receive(folder.join(&name), true, size, cx.background_executor());
        self.transfers.uploads.insert((key, id), sink);
        cx.background_executor()
            .spawn(async move {
                let response = match finished.await {
                    Ok(Ok(path)) => Response::Attached { text: crate::agents::quote(&path.to_string_lossy()) },
                    Ok(Err(reason)) => failed(reason),
                    Err(_) => failed("The transfer stopped."),
                };
                reply(&outbox, id, response);
            })
            .detach();
    }

    pub(super) fn chunk(&mut self, key: Key, id: u64, data: Vec<u8>) {
        if let Some(own) = self.transfers.own.get(&id).filter(|own| own.key == key) {
            if let Some(sink) = &own.sink {
                let _ = sink.unbounded_send(Piece::Data(data));
            }
            return;
        }
        if let Some(sink) = self.transfers.uploads.get(&(key, id))
            && sink.unbounded_send(Piece::Data(data)).is_err()
        {
            self.transfers.uploads.remove(&(key, id));
        }
    }

    pub(super) fn sent(&mut self, key: Key, id: u64) {
        if let Some(sink) = self.transfers.uploads.remove(&(key, id)) {
            let _ = sink.unbounded_send(Piece::End);
        }
    }

    pub(super) fn answered(&mut self, key: Key, id: u64, response: Response) {
        if self.transfers.own.get(&id).is_none_or(|own| own.key != key) {
            return;
        }
        let Some(own) = self.transfers.own.remove(&id) else { return };
        if let Some(sink) = &own.sink
            && response == Response::Done
        {
            let _ = sink.unbounded_send(Piece::End);
        }
        if response != Response::Done {
            own.stop.store(true, Ordering::Release);
        }
        let _ = own.answer.send(response);
    }

    pub(super) fn drop_transfers(&mut self, key: Key) {
        self.transfers.uploads.retain(|(owner, _), _| *owner != key);
        let ids: Vec<u64> = self.transfers.own.iter().filter(|(_, own)| own.key == key).map(|(id, _)| *id).collect();
        for id in ids {
            if let Some(own) = self.transfers.own.remove(&id) {
                own.stop.store(true, Ordering::Release);
                let _ = own.answer.send(failed("The computer went offline."));
            }
        }
    }

    fn request(
        &mut self,
        key: Key,
        request: Request,
        sink: Option<mpsc::UnboundedSender<Piece>>,
    ) -> Result<(u64, Outbox, Arc<AtomicBool>, oneshot::Receiver<Response>), String> {
        let outbox = self.links.get(&key).map(|link| link.outbox.clone()).ok_or("The computer is offline.")?;
        self.transfers.next += 1;
        let id = self.transfers.next;
        let (answer, answered) = oneshot::channel();
        let stop = transfer::stopper();
        self.transfers.own.insert(id, Own { key, sink, stop: stop.clone(), answer });
        outbox.send(&Message::Request { id, request });
        Ok((id, outbox, stop, answered))
    }

    pub fn ask(&mut self, key: Key, request: Request) -> oneshot::Receiver<Response> {
        match self.request(key, request, None) {
            Ok((_, _, _, answered)) => answered,
            Err(reason) => {
                let (answer, answered) = oneshot::channel();
                let _ = answer.send(failed(reason));
                answered
            }
        }
    }

    pub fn download(
        &mut self,
        key: Key,
        project: &str,
        path: &str,
        target: PathBuf,
        cx: &mut Context<Self>,
    ) -> oneshot::Receiver<Result<PathBuf, String>> {
        let (result, received) = oneshot::channel();
        let (sink, finished) = transfer::receive(target, true, u64::MAX, cx.background_executor());
        let request = Request::Download { project: project.to_string(), path: path.to_string() };
        match self.request(key, request, Some(sink)) {
            Ok((_, _, _, answered)) => {
                cx.background_executor()
                    .spawn(async move {
                        let outcome = match answered.await {
                            Ok(Response::Done) => {
                                finished.await.unwrap_or_else(|_| Err("The transfer stopped.".into()))
                            }
                            Ok(Response::Failed { reason }) => Err(reason),
                            _ => Err("The transfer stopped.".into()),
                        };
                        let _ = result.send(outcome);
                    })
                    .detach();
            }
            Err(reason) => {
                let _ = result.send(Err(reason));
            }
        }
        received
    }

    pub fn upload(
        &mut self,
        key: Key,
        project: &str,
        folder: &str,
        file: PathBuf,
        cx: &mut Context<Self>,
    ) -> oneshot::Receiver<Result<Response, String>> {
        let (project, folder) = (project.to_string(), folder.to_string());
        self.send_file(key, file, move |name, size| Request::Upload { project, folder, name, size }, cx)
    }

    pub fn attach(
        &mut self,
        key: Key,
        tab: &str,
        file: PathBuf,
        cx: &mut Context<Self>,
    ) -> oneshot::Receiver<Result<Response, String>> {
        let tab = tab.to_string();
        self.send_file(key, file, move |name, size| Request::Attach { tab, name, size }, cx)
    }

    fn send_file(
        &mut self,
        key: Key,
        file: PathBuf,
        request: impl FnOnce(String, u64) -> Request,
        cx: &mut Context<Self>,
    ) -> oneshot::Receiver<Result<Response, String>> {
        let (result, received) = oneshot::channel();
        let name = file.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        let size = match file.metadata() {
            Ok(metadata) if metadata.is_file() => metadata.len(),
            _ => {
                let _ = result.send(Err(format!("{name} cannot be read.")));
                return received;
            }
        };
        match self.request(key, request(name, size), None) {
            Ok((id, outbox, stop, answered)) => {
                let executor = cx.background_executor().clone();
                cx.background_executor()
                    .spawn(async move {
                        if transfer::send(&outbox, id, &file, &stop, &executor).await.is_ok() {
                            outbox.send(&Message::Sent { id });
                        }
                        let outcome = match answered.await {
                            Ok(Response::Failed { reason }) => Err(reason),
                            Ok(response) => Ok(response),
                            Err(_) => Err("The transfer stopped.".into()),
                        };
                        let _ = result.send(outcome);
                    })
                    .detach();
            }
            Err(reason) => {
                let _ = result.send(Err(reason));
            }
        }
        received
    }
}
