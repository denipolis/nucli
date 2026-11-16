use std::path::PathBuf;
use std::sync::atomic::Ordering;

use futures::channel::oneshot;
use gpui::Context;
use nucli_core::uplink::protocol::{Message, Request, Response};
use nucli_core::uplink::transfer::{self, Piece};
use nucli_core::uplink::wire::Key;

use crate::link::Link;

pub struct Transfer {
    pub name: String,
    pub upload: bool,
    pub size: u64,
    pub done: u64,
}

impl Link {
    fn raw(
        &mut self,
        key: Key,
        request: Request,
    ) -> Result<(u64, nucli_core::uplink::net::Outbox, oneshot::Receiver<Response>), String> {
        let outbox = self.online.get(&key).map(|online| online.outbox.clone()).ok_or("The computer is offline.")?;
        self.next_ask += 1;
        let id = self.next_ask;
        let (sender, receiver) = oneshot::channel();
        outbox.send(&Message::Request { id, request });
        self.asks.insert(id, (key, sender));
        Ok((id, outbox, receiver))
    }

    pub(crate) fn finish_transfer(&mut self, id: u64, response: &Response) {
        if let Some(sink) = self.sinks.remove(&id)
            && *response == Response::Done
        {
            let _ = sink.unbounded_send(Piece::End);
        }
        if let Some(stop) = self.stops.remove(&id)
            && *response != Response::Done
        {
            stop.store(true, Ordering::Release);
        }
    }

    pub(crate) fn chunk(&mut self, id: u64, data: Vec<u8>) {
        let Some(sink) = self.sinks.get(&id) else { return };
        if let Some(transfer) = self.transfers.get_mut(&id) {
            transfer.done += data.len() as u64;
        }
        let _ = sink.unbounded_send(Piece::Data(data));
    }

    pub(crate) fn sending(&mut self, id: u64, size: u64) {
        if let Some(transfer) = self.transfers.get_mut(&id) {
            transfer.size = size;
        }
    }

    pub fn download(
        &mut self,
        key: Key,
        project: &str,
        path: &str,
        cx: &mut Context<Self>,
    ) -> oneshot::Receiver<Result<PathBuf, String>> {
        let (result, received) = oneshot::channel();
        let name = path.rsplit('/').next().unwrap_or(path).to_string();
        let folder = crate::picker::downloads();
        let _ = std::fs::create_dir_all(&folder);
        let (sink, finished) = transfer::receive(folder.join(&name), false, u64::MAX, cx.background_executor());
        let request = Request::Download { project: project.to_string(), path: path.to_string() };
        match self.raw(key, request) {
            Ok((id, _, answered)) => {
                self.sinks.insert(id, sink);
                self.transfers.insert(id, Transfer { name, upload: false, size: 0, done: 0 });
                cx.spawn(async move |this, cx| {
                    let outcome = match answered.await {
                        Ok(Response::Done) => finished.await.unwrap_or_else(|_| Err("The transfer stopped.".into())),
                        Ok(Response::Failed { reason }) => Err(reason),
                        _ => Err("The transfer stopped.".into()),
                    };
                    let _ = this.update(cx, |link, cx| {
                        link.transfers.remove(&id);
                        cx.notify();
                    });
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
    ) -> oneshot::Receiver<Result<(), String>> {
        let (result, received) = oneshot::channel();
        let name = file.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        let Some(size) = file.metadata().ok().filter(|metadata| metadata.is_file()).map(|metadata| metadata.len())
        else {
            let _ = result.send(Err(format!("{name} cannot be read.")));
            return received;
        };
        let request =
            Request::Upload { project: project.to_string(), folder: folder.to_string(), name: name.clone(), size };
        match self.raw(key, request) {
            Ok((id, outbox, answered)) => {
                let stop = transfer::stopper();
                self.stops.insert(id, stop.clone());
                self.transfers.insert(id, Transfer { name, upload: true, size, done: 0 });
                let executor = cx.background_executor().clone();
                let sending = cx.background_executor().spawn(async move {
                    if transfer::send(&outbox, id, &file, &stop, &executor).await.is_ok() {
                        outbox.send(&Message::Sent { id });
                    }
                });
                cx.spawn(async move |this, cx| {
                    sending.await;
                    let outcome = match answered.await {
                        Ok(Response::Done) => Ok(()),
                        Ok(Response::Failed { reason }) => Err(reason),
                        _ => Err("The transfer stopped.".into()),
                    };
                    let _ = this.update(cx, |link, cx| {
                        link.transfers.remove(&id);
                        cx.notify();
                    });
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
