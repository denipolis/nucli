use std::path::PathBuf;

use gpui::{App, AppContext, Context, Entity, Image, PathPromptOptions, Window};

use super::protocol::{Request, Response};
use super::wire::Key;
use crate::palette::{Item, Palette};
use crate::session::Session;
use crate::window::WorkspaceView;
use crate::{actions, icons, store};

fn name_of(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

pub fn upload(key: Key, project: &str, files: Vec<PathBuf>, workspace: &str, cx: &mut App) {
    let Some(uplink) = super::uplink(cx) else { return };
    for file in files {
        let name = file.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        let progress = store::begin_progress(format!("uploading {name}"), cx);
        let done = uplink.update(cx, |uplink, cx| uplink.upload(key, project, "", file, cx));
        let workspace = workspace.to_string();
        cx.spawn(async move |cx| {
            let outcome = done.await.unwrap_or_else(|_| Err("The transfer stopped.".into()));
            cx.update(|cx| {
                store::finish_progress(progress, cx);
                if let Err(reason) = outcome {
                    actions::alert(&workspace, &format!("{name} was not uploaded"), reason, cx);
                }
            });
        })
        .detach();
    }
}

pub fn attach(session: Entity<Session>, files: Vec<PathBuf>, cx: &mut App) {
    let Some(remote) = session.read(cx).remote.clone().filter(|remote| remote.control) else { return };
    let Some(uplink) = super::uplink(cx) else { return };
    let id = session.read(cx).id.clone();
    let workspace = store::store(cx).read(cx).shower_of(&id).map(|workspace| workspace.id.clone()).unwrap_or_default();
    if uplink.read(cx).protocol(&remote.key) < 1 {
        let reason = format!("Update Nucli on {} to send files to its tabs.", remote.device);
        actions::alert(&workspace, "Files were not sent", reason, cx);
        return;
    }
    let sending: Vec<_> = files
        .into_iter()
        .map(|file| {
            let name = file.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
            let progress = store::begin_progress(format!("sending {name}"), cx);
            let done = uplink.update(cx, |uplink, cx| uplink.attach(remote.key, &remote.tab, file, cx));
            (name, progress, done)
        })
        .collect();
    cx.spawn(async move |cx| {
        let mut paths = Vec::new();
        for (name, progress, done) in sending {
            let outcome = done.await.unwrap_or_else(|_| Err("The transfer stopped.".into()));
            cx.update(|cx| {
                store::finish_progress(progress, cx);
                match outcome {
                    Ok(Response::Attached { text }) => paths.push(text),
                    Ok(_) => {}
                    Err(reason) => actions::alert(&workspace, &format!("{name} was not sent"), reason, cx),
                }
            });
        }
        if !paths.is_empty() {
            session.update(cx, |session, cx| session.paste_text(&format!("{} ", paths.join(" ")), cx));
        }
    })
    .detach();
}

pub fn attach_image(session: Entity<Session>, image: Image, cx: &mut App) {
    let folder = std::env::temp_dir().join("nucli-paste").join(uuid::Uuid::new_v4().to_string());
    let file = folder.join(format!("image.{}", image.format.extension()));
    cx.spawn(async move |cx| {
        let written = cx
            .background_executor()
            .spawn(async move {
                std::fs::create_dir_all(&folder).and_then(|_| std::fs::write(&file, &image.bytes)).map(|_| file)
            })
            .await;
        if let Ok(file) = written {
            cx.update(|cx| attach(session, vec![file], cx));
        }
    })
    .detach();
}

pub fn pick_upload(key: Key, project: String, workspace: String, cx: &mut App) {
    let picked = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: false,
        multiple: true,
        prompt: Some("Upload".into()),
    });
    cx.spawn(async move |cx| {
        if let Ok(Ok(Some(files))) = picked.await {
            cx.update(|cx| upload(key, &project, files, &workspace, cx));
        }
    })
    .detach();
}

pub(super) fn save(key: Key, project: String, path: String, workspace: String, cx: &mut App) {
    let name = name_of(&path);
    let folder = dirs::download_dir().or_else(dirs::home_dir).unwrap_or_default();
    let chosen = cx.prompt_for_new_path(&folder, Some(&name));
    cx.spawn(async move |cx| {
        let Ok(Ok(Some(target))) = chosen.await else { return };
        cx.update(|cx| {
            let Some(uplink) = super::uplink(cx) else { return };
            let progress = store::begin_progress(format!("downloading {name}"), cx);
            let done = uplink.update(cx, |uplink, cx| uplink.download(key, &project, &path, target, cx));
            cx.spawn(async move |cx| {
                let outcome = done.await.unwrap_or_else(|_| Err("The transfer stopped.".into()));
                cx.update(|cx| {
                    store::finish_progress(progress, cx);
                    if let Err(reason) = outcome {
                        actions::alert(&workspace, &format!("{name} was not downloaded"), reason, cx);
                    }
                });
            })
            .detach();
        });
    })
    .detach();
}

pub fn pick_download(
    view: &mut WorkspaceView,
    key: Key,
    project: String,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) {
    let Some(uplink) = super::uplink(cx) else { return };
    let answered = uplink.update(cx, |uplink, _| uplink.ask(key, Request::Tree { project: project.clone() }));
    let workspace = view.workspace.clone();
    cx.spawn_in(window, async move |view, cx| {
        let response = answered.await.unwrap_or(Response::Failed { reason: "No answer.".into() });
        let _ = view.update_in(cx, |view, window, cx| {
            let files = match response {
                Response::Tree { files } => files,
                Response::Failed { reason } => {
                    actions::alert(&workspace, "Files are not available", reason, cx);
                    return;
                }
                _ => return,
            };
            let items = files
                .into_iter()
                .map(|file| {
                    let (project, path, workspace) = (project.clone(), file.clone(), workspace.clone());
                    let (glyph, color) = icons::file(&name_of(&file));
                    let theme = crate::ui::theme(cx);
                    Item::new(file, move |_, _, cx| save(key, project.clone(), path.clone(), workspace.clone(), cx))
                        .glyph(glyph, Some(color.color(theme.dark)))
                })
                .collect();
            let palette = cx.new(|cx| Palette::new("download", "download a file", items, 0, cx));
            view.show_palette(palette, window, cx);
        });
    })
    .detach();
}
