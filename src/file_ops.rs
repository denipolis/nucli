use std::io;
use std::path::{Path, PathBuf};

use gpui::{App, Context, Global, PromptLevel, Window};

use crate::actions::prompt;
use crate::file_view;
use crate::menu::{self, Item};
use crate::window::WorkspaceView;

#[derive(Clone)]
pub struct DraggedPath {
    pub path: PathBuf,
    pub title: String,
}

#[derive(Default)]
struct Clipboard(Option<(PathBuf, bool)>);

impl Global for Clipboard {}

pub fn relative(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    Some(relative.components().map(|part| part.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/"))
}

fn join(folder: &str, name: &str) -> String {
    let name = name.trim_matches('/');
    if folder.is_empty() { name.to_string() } else { format!("{folder}/{name}") }
}

fn parent(path: &str) -> String {
    path.rsplit_once('/').map(|(parent, _)| parent.to_string()).unwrap_or_default()
}

fn name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

fn free_name(path: &Path) -> PathBuf {
    if path.symlink_metadata().is_err() {
        return path.to_path_buf();
    }
    let stem = path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
    let extension = path.extension().map(|ext| format!(".{}", ext.to_string_lossy())).unwrap_or_default();
    (1..)
        .map(|count| {
            let suffix = if count == 1 { " copy".to_string() } else { format!(" copy {count}") };
            path.with_file_name(format!("{stem}{suffix}{extension}"))
        })
        .find(|candidate| candidate.symlink_metadata().is_err())
        .unwrap_or_else(|| path.to_path_buf())
}

fn copy_all(from: &Path, to: &Path) -> io::Result<()> {
    if !from.symlink_metadata()?.is_dir() {
        return std::fs::copy(from, to).map(drop);
    }
    std::fs::create_dir(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        copy_all(&entry.path(), &to.join(entry.file_name()))?;
    }
    Ok(())
}

fn move_path(from: &Path, to: &Path) -> io::Result<()> {
    match std::fs::rename(from, to) {
        Err(error) if error.kind() == io::ErrorKind::CrossesDevices => {
            copy_all(from, to)?;
            trash::delete(from).map_err(io::Error::other)
        }
        result => result,
    }
}

fn exists_error(path: &str) -> io::Error {
    io::Error::other(format!("{path} already exists"))
}

fn run<T: Send + 'static>(
    op: impl FnOnce() -> io::Result<T> + Send + 'static,
    then: impl FnOnce(T, &mut WorkspaceView, &mut Window, &mut Context<WorkspaceView>) + 'static,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) {
    let task = cx.background_executor().spawn(async move { op() });
    cx.spawn_in(window, async move |this, cx| {
        let result = task.await;
        let _ = this.update_in(cx, |view, window, cx| match result {
            Ok(value) => then(value, view, window, cx),
            Err(error) => drop(window.prompt(
                PromptLevel::Warning,
                "File operation failed",
                Some(&error.to_string()),
                &["OK"],
                cx,
            )),
        });
    })
    .detach();
}

fn create(root: &Path, relative: String, folder: bool, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let (root, path) = (root.to_path_buf(), root.join(&relative));
    let shown = relative.clone();
    run(
        move || {
            if path.symlink_metadata().is_ok() {
                return Err(exists_error(&shown));
            }
            if folder {
                return std::fs::create_dir_all(&path);
            }
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::OpenOptions::new().write(true).create_new(true).open(&path).map(drop)
        },
        move |(), view, window, cx| {
            if !folder {
                file_view::open_file(view, &root, &relative, None, window, cx);
            }
        },
        window,
        cx,
    );
}

fn rename(root: &Path, from: String, to: String, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let to = to.trim_matches('/').to_string();
    if to.is_empty() || to == from {
        return;
    }
    let (root, source, target) = (root.to_path_buf(), root.join(&from), root.join(&to));
    let (case_only, shown) = (from.eq_ignore_ascii_case(&to), to.clone());
    run(
        move || {
            if !case_only && target.symlink_metadata().is_ok() {
                return Err(exists_error(&shown));
            }
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            move_path(&source, &target)
        },
        move |(), view, window, cx| file_view::path_moved(view, &root, &from, Some(&to), window, cx),
        window,
        cx,
    );
}

fn delete(root: &Path, relative: String, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let answer = window.prompt(
        PromptLevel::Warning,
        &format!("Delete {}?", name(&relative)),
        Some("It is moved to the trash."),
        &["Delete", "Cancel"],
        cx,
    );
    let root = root.to_path_buf();
    cx.spawn_in(window, async move |this, cx| {
        if answer.await != Ok(0) {
            return;
        }
        let _ = this.update_in(cx, |_, window, cx| {
            let path = root.join(&relative);
            run(
                move || trash::delete(&path).map_err(io::Error::other),
                move |(), view, window, cx| file_view::path_moved(view, &root, &relative, None, window, cx),
                window,
                cx,
            );
        });
    })
    .detach();
}

fn duplicate(root: &Path, relative: &str, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let source = root.join(relative);
    run(move || copy_all(&source, &free_name(&source)), |(), _, _, _| {}, window, cx);
}

pub fn transfer(
    root: &Path,
    sources: Vec<PathBuf>,
    folder: &str,
    cut: bool,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) {
    let (root, folder) = (root.to_path_buf(), root.join(folder));
    run(
        move || {
            let mut moved = Vec::new();
            for source in sources {
                let Some(name) = source.file_name() else { continue };
                if folder.starts_with(&source) || (cut && source.parent() == Some(folder.as_path())) {
                    continue;
                }
                let target = free_name(&folder.join(name));
                if cut {
                    move_path(&source, &target)?
                } else {
                    copy_all(&source, &target)?
                }
                moved.push((source, target));
            }
            Ok(moved)
        },
        move |moved, view, window, cx| {
            if !cut {
                return;
            }
            for (source, target) in moved {
                if let (Some(from), Some(to)) = (relative(&root, &source), relative(&root, &target)) {
                    file_view::path_moved(view, &root, &from, Some(&to), window, cx);
                }
            }
        },
        window,
        cx,
    );
}

fn paste(root: &Path, folder: &str, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let Some((source, cut)) = cx.try_global::<Clipboard>().and_then(|clipboard| clipboard.0.clone()) else { return };
    if cut {
        cx.set_global(Clipboard(None));
    }
    transfer(root, vec![source], folder, cut, window, cx);
}

fn asked(
    label: &'static str,
    title: String,
    value: String,
    done: impl Fn(String, &mut Window, &mut Context<WorkspaceView>) + 'static,
) -> Item {
    let done = std::rc::Rc::new(done);
    menu::action(label, move |view, window, cx| {
        let done = done.clone();
        prompt(view, &title, &value, window, cx, move |text, _, window, cx| {
            if !text.is_empty() {
                done(text, window, cx);
            }
        });
    })
}

pub fn menu(root: &Path, relative: &str, is_dir: bool, view: &WorkspaceView, cx: &App) -> Vec<Item> {
    let folder = if is_dir { relative.to_string() } else { parent(relative) };
    let place = if folder.is_empty() { String::new() } else { format!(" in {folder}") };
    let mut items = Vec::new();
    for (label, title, is_folder) in [("New File…", "new file", false), ("New Folder…", "new folder", true)] {
        let (root, folder) = (root.to_path_buf(), folder.clone());
        items.push(asked(label, format!("{title}{place}"), String::new(), move |name, window, cx| {
            create(&root, join(&folder, &name), is_folder, window, cx)
        }));
    }
    let has_clipboard = cx.try_global::<Clipboard>().is_some_and(|clipboard| clipboard.0.is_some());
    if relative.is_empty() {
        if has_clipboard {
            let root = root.to_path_buf();
            items.push(menu::action("Paste", move |_, window, cx| paste(&root, "", window, cx)));
        }
        items.push(Item::Separator);
        let reveal = root.to_path_buf();
        items.push(menu::action(crate::actions::reveal_label(), move |_, _, cx| cx.reveal_path(&reveal)));
        items.push(menu::copy("Copy Path", root.to_string_lossy()));
        return items;
    }
    items.push(Item::Separator);
    let full = root.join(relative);
    if is_dir {
        let project = full.clone();
        items.push(menu::action("Open as Project", move |view, window, cx| {
            crate::actions::open_folder(view, project.clone(), window, cx);
        }));
    }
    items.extend(file_view::file_menu(view, &full, relative, cx));
    items.push(Item::Separator);
    for (label, cut) in [("Cut", true), ("Copy", false)] {
        let path = full.clone();
        items.push(menu::action(label, move |_, _, cx| cx.set_global(Clipboard(Some((path.clone(), cut))))));
    }
    if has_clipboard {
        let (root, folder) = (root.to_path_buf(), folder.clone());
        items.push(menu::action("Paste", move |_, window, cx| paste(&root, &folder, window, cx)));
    }
    {
        let (root, relative) = (root.to_path_buf(), relative.to_string());
        items.push(menu::action("Duplicate", move |_, window, cx| duplicate(&root, &relative, window, cx)));
    }
    items.push(Item::Separator);
    {
        let (root, from, parent) = (root.to_path_buf(), relative.to_string(), parent(relative));
        items.push(asked("Rename…", format!("rename {}", name(relative)), name(relative), move |to, window, cx| {
            rename(&root, from.clone(), join(&parent, &to), window, cx)
        }));
    }
    {
        let (root, from) = (root.to_path_buf(), relative.to_string());
        items.push(asked(
            "Move…",
            format!("move {}", name(relative)),
            relative.to_string(),
            move |to, window, cx| rename(&root, from.clone(), to, window, cx),
        ));
    }
    let (root, relative) = (root.to_path_buf(), relative.to_string());
    items.push(menu::action("Delete", move |_, window, cx| delete(&root, relative.clone(), window, cx)));
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_and_names_free_targets() {
        let base = std::env::temp_dir().join(format!("nucli-ops-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(base.join("src/inner")).unwrap();
        std::fs::write(base.join("src/inner/a.rs"), "a").unwrap();
        std::fs::write(base.join("notes.txt"), "n").unwrap();
        assert_eq!(free_name(&base.join("new.txt")), base.join("new.txt"));
        assert_eq!(free_name(&base.join("notes.txt")), base.join("notes copy.txt"));
        std::fs::write(base.join("notes copy.txt"), "n").unwrap();
        assert_eq!(free_name(&base.join("notes.txt")), base.join("notes copy 2.txt"));
        copy_all(&base.join("src"), &free_name(&base.join("src"))).unwrap();
        assert_eq!(std::fs::read_to_string(base.join("src copy/inner/a.rs")).unwrap(), "a");
        assert_eq!(relative(&base, &base.join("src copy/inner")), Some("src copy/inner".to_string()));
        assert_eq!((join("", "/a.rs"), join("src", "b/c.rs")), ("a.rs".to_string(), "src/b/c.rs".to_string()));
        assert_eq!(
            (parent("src/a.rs"), parent("a.rs"), name("src/a.rs")),
            ("src".into(), String::new(), "a.rs".into())
        );
        std::fs::remove_dir_all(&base).unwrap();
    }
}
