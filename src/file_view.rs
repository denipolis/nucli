use std::collections::HashMap;
use std::path::{Path, PathBuf};

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, AppContext, Bounds, Context, Entity, InteractiveElement, IntoElement, MouseButton, ObjectFit,
    ParentElement, PathPromptOptions, PromptLevel, SharedString, StatefulInteractiveElement, Styled, StyledImage,
    Window, canvas, div, fill, img, point, px, size,
};

use crate::editor::{Content, Editor, EditorEvent};
use crate::icons;
use crate::menu::{self, Item};
use crate::persist::Selected;
use crate::splits::{Direction, Rect, Zone};
use crate::splits_view::DropTarget;
use crate::store::{self, FilePlace};
use crate::ui;
use crate::window::WorkspaceView;

#[derive(Default)]
pub struct Files {
    pub open: HashMap<String, Entity<Editor>>,
    pub strips: HashMap<String, (gpui::ScrollHandle, Option<String>)>,
}

#[derive(Clone)]
pub struct DraggedFile {
    pub file: String,
    pub title: String,
}

const COMMIT_MARK: &str = "@@";

pub fn spec(path: &str, commit: Option<&str>) -> String {
    match commit {
        Some(commit) => format!("{path}{COMMIT_MARK}{commit}"),
        None => path.to_string(),
    }
}

pub fn spec_parts(spec: &str) -> (&str, Option<&str>) {
    match spec.rsplit_once(COMMIT_MARK) {
        Some((path, commit)) => (path, Some(commit)),
        None => (spec, None),
    }
}

fn key(project: &Path, spec: &str) -> String {
    format!("{}\u{0}{spec}", project.display())
}

pub fn editor_for(
    view: &mut WorkspaceView,
    project: &Path,
    file: &str,
    cx: &mut Context<WorkspaceView>,
) -> Entity<Editor> {
    let id = key(project, file);
    if let Some(editor) = view.files.open.get(&id) {
        return editor.clone();
    }
    let (path, commit) = spec_parts(file);
    let (root, path, commit) = (project.to_path_buf(), path.to_string(), commit.map(String::from));
    let language = view
        .state(cx)
        .and_then(|state| state.project(project))
        .and_then(|project| project.languages.get(file))
        .and_then(|name| crate::highlight::known_language(name));
    let editor = cx.new(|cx| Editor::new(root, path, commit, language, cx));
    cx.observe(&editor, |_, _, cx| cx.notify()).detach();
    cx.subscribe(&editor, |view, editor, event: &EditorEvent, cx| match event {
        EditorEvent::Menu(position) => {
            let items = editor_menu(view, &editor, cx);
            view.open_menu(*position, items, cx);
        }
    })
    .detach();
    view.files.open.insert(id, editor.clone());
    editor
}

pub fn set_active_language(view: &WorkspaceView, language: Option<&'static str>, cx: &mut Context<WorkspaceView>) {
    let Some(editor) = active_editor(view, cx) else { return };
    editor.update(cx, |editor, cx| editor.set_language(language, cx));
    let workspace = view.workspace.clone();
    store::store(cx).update(cx, |store, cx| {
        if let Some(project) = store.workspace_mut(&workspace).and_then(|state| state.current_project_mut())
            && let Some(file) = project.active_file().cloned()
        {
            match language {
                Some(language) => project.languages.insert(file, language.to_string()),
                None => project.languages.remove(&file),
            };
        }
        store.changed(cx);
    });
}

pub fn active_editor(view: &WorkspaceView, cx: &App) -> Option<Entity<Editor>> {
    let project = view.state(cx)?.current_project()?;
    let file = project.active_file()?;
    view.files.open.get(&key(&project.path, file)).cloned()
}

pub fn focus_active(view: &mut WorkspaceView, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let Some((path, file)) = view
        .state(cx)
        .and_then(|state| state.current_project())
        .and_then(|project| project.active_file().cloned().map(|file| (project.path.clone(), file)))
    else {
        return;
    };
    let editor = editor_for(view, &path, &file, cx);
    let focus = editor.read(cx).focus.clone();
    window.focus(&focus);
}

pub fn open_file(
    view: &mut WorkspaceView,
    project: &Path,
    relative: &str,
    line: Option<usize>,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) {
    view.settings = None;
    let workspace = view.workspace.clone();
    let path = project.to_path_buf();
    let name = relative.to_string();
    store::store(cx).update(cx, |store, cx| {
        let Some(state) = store.workspace_mut(&workspace) else { return };
        let Some(target) = state.project_mut(&path) else { return };
        target.show_file(&name);
        target.file_shown = true;
        if !name.contains(COMMIT_MARK) {
            target.recent_files.retain(|file| *file != name);
            target.recent_files.insert(0, name.clone());
            target.recent_files.truncate(20);
        }
        state.selected = Some(Selected::Project(path.clone()));
        store.changed(cx);
    });
    let editor = editor_for(view, project, relative, cx);
    if let Some(line) = line {
        editor.update(cx, |editor, cx| {
            editor.goto_line(line);
            cx.notify();
        });
    }
    let focus = editor.read(cx).focus.clone();
    window.focus(&focus);
    cx.notify();
}

pub fn open_path(
    view: &mut WorkspaceView,
    path: &Path,
    line: Option<usize>,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) {
    let project = view.current_project_path(cx).unwrap_or_default();
    let relative = if project.as_os_str().is_empty() { None } else { path.strip_prefix(&project).ok() };
    match relative {
        Some(relative) => {
            let relative = relative.to_string_lossy().replace('\\', "/");
            open_file(view, &project, &relative, line, window, cx);
        }
        None => {
            let absolute = path.to_string_lossy().into_owned();
            open_file(view, &project, &absolute, line, window, cx);
        }
    }
}

pub fn open_config(view: &mut WorkspaceView, path: PathBuf, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    if !path.exists() {
        let _ = crate::paths::write_atomic(&path, b"{\n}\n");
    }
    let project = view.current_project_path(cx).unwrap_or_default();
    let absolute = path.to_string_lossy().into_owned();
    open_file(view, &project, &absolute, None, window, cx);
}

fn remove_tab(
    view: &mut WorkspaceView,
    project: &Path,
    file: &str,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) {
    let workspace = view.workspace.clone();
    let (path, name) = (project.to_path_buf(), file.to_string());
    view.files.open.remove(&key(project, file));
    store::store(cx).update(cx, |store, cx| {
        if let Some(target) = store.workspace_mut(&workspace).and_then(|state| state.project_mut(&path)) {
            target.close_file(&name);
            target.file_shown = target.file_shown && target.active_file().is_some();
        }
        store.changed(cx);
    });
    if view.file_shown(cx) {
        focus_active(view, window, cx);
    } else {
        hide(view, window, cx);
    }
}

pub fn close_tab(
    view: &mut WorkspaceView,
    project: &Path,
    file: &str,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) {
    let dirty = view.files.open.get(&key(project, file)).filter(|editor| editor.read(cx).dirty).cloned();
    let Some(editor) = dirty else {
        remove_tab(view, project, file, window, cx);
        return;
    };
    let name = spec_parts(file).0.rsplit('/').next().unwrap_or(file).to_string();
    let answer = window.prompt(
        PromptLevel::Warning,
        &format!("Save changes to {name}?"),
        Some("Your changes are lost if you don't save them."),
        &["Save", "Don't Save", "Cancel"],
        cx,
    );
    let (project, file) = (project.to_path_buf(), file.to_string());
    cx.spawn_in(window, async move |this, cx| {
        let Ok(choice) = answer.await else { return };
        if choice == 2 {
            return;
        }
        let _ = this.update_in(cx, |view, window, cx| {
            if choice == 0 {
                editor.update(cx, |editor, cx| editor.save_file(cx));
            }
            remove_tab(view, &project, &file, window, cx);
        });
    })
    .detach();
}

fn under(file: &str, path: &str) -> bool {
    file.strip_prefix(path).is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

pub fn path_moved(
    view: &mut WorkspaceView,
    project: &Path,
    from: &str,
    to: Option<&str>,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) {
    let files: Vec<String> = view
        .state(cx)
        .and_then(|state| state.project(project))
        .map(|state| {
            state.groups.iter().flat_map(|group| group.files.iter()).filter(|file| under(file, from)).cloned().collect()
        })
        .unwrap_or_default();
    let Some(to) = to else {
        for file in &files {
            remove_tab(view, project, file, window, cx);
        }
        return;
    };
    let renamed: Vec<(String, String)> =
        files.into_iter().map(|file| (format!("{to}{}", &file[from.len()..]), file)).collect();
    for (new, old) in &renamed {
        if let Some(editor) = view.files.open.remove(&key(project, old)) {
            editor.update(cx, |editor, _| {
                editor.full = project.join(new);
                editor.path = new.clone();
            });
            view.files.open.insert(key(project, new), editor);
        }
    }
    let (workspace, path) = (view.workspace.clone(), project.to_path_buf());
    store::store(cx).update(cx, |store, cx| {
        if let Some(target) = store.workspace_mut(&workspace).and_then(|state| state.project_mut(&path)) {
            for (new, old) in &renamed {
                target.rename_file(old, new);
            }
        }
        store.changed(cx);
    });
}

fn close_tabs(
    view: &mut WorkspaceView,
    project: &Path,
    files: Vec<String>,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) {
    let (dirty, clean): (Vec<String>, Vec<String>) = files
        .into_iter()
        .partition(|file| view.files.open.get(&key(project, file)).is_some_and(|editor| editor.read(cx).dirty));
    for file in &clean {
        remove_tab(view, project, file, window, cx);
    }
    if let [file] = dirty.as_slice() {
        close_tab(view, project, file, window, cx);
    }
    if dirty.len() < 2 {
        return;
    }
    let answer = window.prompt(
        PromptLevel::Warning,
        &format!("Save changes to {} files?", dirty.len()),
        Some("Your changes are lost if you don't save them."),
        &["Save All", "Don't Save", "Cancel"],
        cx,
    );
    let project = project.to_path_buf();
    cx.spawn_in(window, async move |this, cx| {
        let Ok(choice) = answer.await else { return };
        if choice == 2 {
            return;
        }
        let _ = this.update_in(cx, |view, window, cx| {
            for file in &dirty {
                if choice == 0
                    && let Some(editor) = view.files.open.get(&key(&project, file)).cloned()
                {
                    editor.update(cx, |editor, cx| editor.save_file(cx));
                }
                remove_tab(view, &project, file, window, cx);
            }
        });
    })
    .detach();
}

pub fn close_active(view: &mut WorkspaceView, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let Some((path, file)) = view
        .state(cx)
        .and_then(|state| state.current_project())
        .and_then(|project| project.active_file().cloned().map(|file| (project.path.clone(), file)))
    else {
        return;
    };
    close_tab(view, &path, &file, window, cx);
}

pub fn hide(view: &mut WorkspaceView, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let workspace = view.workspace.clone();
    let path = view.current_project_path(cx).unwrap_or_default();
    let tab = view.state(cx).and_then(|state| state.project(&path)).and_then(|project| project.active_tab.clone());
    store::store(cx).update(cx, |store, cx| {
        if let Some(target) = store.workspace_mut(&workspace).and_then(|state| state.project_mut(&path)) {
            target.file_shown = false;
        }
        store.changed(cx);
    });
    match tab {
        Some(tab) => view.select_tab(&tab, window, cx),
        None => view.focus_current(window, cx),
    }
    cx.notify();
}

pub struct FileStatus {
    pub position: Option<String>,
    pub language: Option<&'static str>,
    pub icon_file: String,
    pub added: usize,
    pub removed: usize,
    pub commit: Option<String>,
}

pub fn file_status(editor: &Entity<Editor>, cx: &App) -> FileStatus {
    let editor = editor.read(cx);
    let text = editor.content == Content::Text;
    FileStatus {
        position: text.then(|| editor.position_label()),
        language: text.then(|| editor.language_label()),
        icon_file: match editor.language {
            Some(language) => crate::highlight::sample_file(language).unwrap_or("a.txt").to_string(),
            None => {
                Path::new(&editor.full).file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
            }
        },
        added: editor.added,
        removed: editor.removed,
        commit: editor.commit.clone(),
    }
}

pub fn any_dirty(view: &WorkspaceView, cx: &App) -> bool {
    view.files.open.values().any(|editor| editor.read(cx).dirty)
}

pub fn save_all(view: &WorkspaceView, cx: &mut App) {
    let editors: Vec<Entity<Editor>> =
        view.files.open.values().filter(|editor| editor.read(cx).dirty).cloned().collect();
    for editor in editors {
        editor.update(cx, |editor, cx| editor.save_file(cx));
    }
}

fn run_with(app: &str, file: &Path) {
    let mut command = if cfg!(target_os = "macos") && app.ends_with(".app") {
        let mut command = std::process::Command::new("open");
        command.arg("-a").arg(app);
        command
    } else {
        std::process::Command::new(app)
    };
    let _ = command.arg(file).spawn();
}

pub fn open_with_menu(view: &WorkspaceView, file: PathBuf, cx: &App) -> Vec<Item> {
    let extension = file.extension().map(|ext| ext.to_string_lossy().to_lowercase()).unwrap_or_default();
    let known = view.store(cx).open_with.get(&extension).cloned().unwrap_or_default();
    let mut items = Vec::new();
    {
        let file = file.clone();
        items.push(menu::action("Open in Default App", move |_, _, cx| cx.open_with_system(&file)));
    }
    for app in known {
        let (file, name) =
            (file.clone(), app.rsplit(['/', '\\']).next().unwrap_or(&app).trim_end_matches(".app").to_string());
        items.push(menu::action(format!("Open with {name}"), move |_, _, _| run_with(&app, &file)));
    }
    items.push(menu::action("Open With…", move |_, window, cx| {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: cfg!(target_os = "macos"),
            multiple: false,
            prompt: Some("Open".into()),
        });
        let (file, extension) = (file.clone(), extension.clone());
        cx.spawn_in(window, async move |_, cx| {
            if let Ok(Ok(Some(paths))) = receiver.await
                && let Some(app) = paths.into_iter().next()
            {
                let app = app.to_string_lossy().into_owned();
                run_with(&app, &file);
                let _ = cx.update(|_, cx| {
                    store::store(cx).update(cx, |store, cx| {
                        let list = store.open_with.entry(extension).or_default();
                        list.retain(|known| *known != app);
                        list.insert(0, app);
                        list.truncate(5);
                        store.changed(cx);
                    });
                });
            }
        })
        .detach();
    }));
    items
}

pub fn path_menu(full: &Path, relative: &str) -> Vec<Item> {
    let reveal = full.to_path_buf();
    vec![
        menu::action(crate::actions::reveal_label(), move |_, _, cx| cx.reveal_path(&reveal)),
        menu::copy("Copy Path", full.to_string_lossy()),
        menu::copy("Copy Relative Path", relative),
    ]
}

pub fn file_menu(view: &WorkspaceView, full: &Path, relative: &str, cx: &App) -> Vec<Item> {
    let mut items = open_with_menu(view, full.to_path_buf(), cx);
    items.push(Item::Separator);
    items.extend(path_menu(full, relative));
    items
}

fn editor_menu(view: &WorkspaceView, editor: &Entity<Editor>, cx: &App) -> Vec<Item> {
    let data = editor.read(cx);
    let (editable, selected) = (data.is_editable(), data.selection().is_some());
    let mut items = Vec::new();
    if editable && data.can_undo() {
        items.push(menu::command("Undo", "undo", cx));
    }
    if editable && data.can_redo() {
        items.push(menu::command("Redo", "redo", cx));
    }
    if !items.is_empty() {
        items.push(Item::Separator);
    }
    if editable && selected {
        items.push(menu::command("Cut", "cut", cx));
    }
    if selected {
        items.push(menu::command("Copy", "copy", cx));
    }
    if editable {
        items.push(menu::command("Paste", "paste", cx));
    }
    items.push(menu::command("Select All", "select_all", cx));
    items.push(Item::Separator);
    items.push(menu::command("Find", "find", cx));
    if editable {
        items.push(menu::command("Replace", "replace", cx));
    }
    if data.added + data.removed > 0 {
        items.push(menu::command("Next Change", "next_change", cx));
        items.push(menu::command("Previous Change", "previous_change", cx));
    }
    if data.dirty {
        items.push(Item::Separator);
        items.push(menu::command("Save", "save", cx));
    }
    items.push(Item::Separator);
    items.extend(file_menu(view, &data.full, &data.path, cx));
    items
}

fn tab_menu(view: &WorkspaceView, project: &Path, files: &[String], index: usize, cx: &App) -> Vec<Item> {
    let file = files[index].clone();
    let path = spec_parts(&file).0.to_string();
    let full = if Path::new(&path).is_absolute() { PathBuf::from(&path) } else { project.join(&path) };
    let closer = |label: &'static str, files: Vec<String>| {
        let project = project.to_path_buf();
        menu::action(label, move |view, window, cx| close_tabs(view, &project, files.clone(), window, cx))
    };
    let others: Vec<String> = files.iter().filter(|other| **other != file).cloned().collect();
    let mut items = vec![closer("Close", vec![file.clone()])];
    if !others.is_empty() {
        items.push(closer("Close Others", others));
    }
    if index + 1 < files.len() {
        items.push(closer("Close to the Right", files[index + 1..].to_vec()));
    }
    items.push(closer("Close All", files.to_vec()));
    items.push(Item::Separator);
    for (label, direction) in [("Split Right", Direction::Right), ("Split Down", Direction::Down)] {
        let (project, file) = (project.to_path_buf(), file.clone());
        items.push(menu::action(label, move |view, window, cx| {
            open_file(view, &project, &file, None, window, cx);
            split(view, direction, window, cx);
        }));
    }
    items.push(Item::Separator);
    items.extend(file_menu(view, &full, &path, cx));
    items
}

#[allow(clippy::too_many_arguments)]
fn tab_strip(
    view: &mut WorkspaceView,
    project: &Path,
    group: &str,
    files: &[String],
    active: Option<&str>,
    (focused, home): (bool, bool),
    cx: &mut Context<WorkspaceView>,
) -> AnyElement {
    let theme = ui::theme(cx);
    let (handle, revealed) = view.files.strips.entry(group.to_string()).or_default();
    if revealed.as_deref() != active {
        if let Some(index) = active.and_then(|active| files.iter().position(|file| file == active)) {
            handle.scroll_to_item(index + usize::from(home));
        }
        *revealed = active.map(String::from);
    }
    let handle = handle.clone();
    let (drop_project, drop_group, hover) = (project.to_path_buf(), group.to_string(), theme.hover);
    let mut strip = div()
        .id("file-tabs")
        .flex()
        .flex_none()
        .items_center()
        .min_h(px((crate::settings::get(cx).ui_font_size() * 1.5).round()))
        .track_scroll(&handle)
        .overflow_x_scroll()
        .border_b_1()
        .border_color(theme.border)
        .bg(theme.surface)
        .drag_over::<DraggedFile>(move |style, _, _, _| style.bg(hover))
        .on_drop(cx.listener(move |view, drag: &DraggedFile, window, cx| {
            move_file(view, &drop_project, &drag.file, &drop_group, FilePlace::Zone(Zone::Center), window, cx);
        }))
        .when(home, |this| {
            this.child(
                div()
                    .id("file-home")
                    .flex_none()
                    .px(px(8.0))
                    .text_color(theme.muted)
                    .hover(|style| style.text_color(theme.accent))
                    .child(icons::HOME)
                    .on_click(cx.listener(|view, _, window, cx| {
                        let workspace = view.workspace.clone();
                        let path = view.current_project_path(cx).unwrap_or_default();
                        store::store(cx).update(cx, |store, cx| {
                            if let Some(target) =
                                store.workspace_mut(&workspace).and_then(|state| state.project_mut(&path))
                            {
                                target.file_shown = false;
                            }
                            store.changed(cx);
                        });
                        view.focus_current(window, cx);
                    })),
            )
        });
    for (index, file) in files.iter().enumerate() {
        let (path, commit) = spec_parts(file);
        let name = path.rsplit(['/', '\\']).next().unwrap_or(path).to_string();
        let (glyph, color) = icons::file(&name);
        let selected = active == Some(file.as_str());
        let (fill, text) = match (selected, focused) {
            (true, true) => (Some(theme.accent), theme.on_accent),
            (true, false) => (Some(theme.selection), theme.text),
            _ => (None, theme.muted),
        };
        let dirty = view.files.open.get(&key(project, file)).is_some_and(|editor| editor.read(cx).dirty);
        let (select_project, select_file) = (project.to_path_buf(), file.clone());
        let (close_project, close_file) = (project.to_path_buf(), file.clone());
        let (middle_project, middle_file) = (project.to_path_buf(), file.clone());
        let (menu_project, menu_files) = (project.to_path_buf(), files.to_vec());
        let (drop_project, drop_file, tab_group) = (project.to_path_buf(), file.clone(), group.to_string());
        let label = match commit {
            Some(commit) => format!("{name} @ {}", &commit[..commit.len().min(7)]),
            None => name,
        };
        let dragged = DraggedFile { file: file.clone(), title: label.clone() };
        let accent = theme.accent;
        strip = strip.child(
            div()
                .id(("file-tab", index))
                .flex()
                .flex_none()
                .items_center()
                .gap(px(6.0))
                .px(px(8.0))
                .border_l_1()
                .border_color(theme.border)
                .text_color(text)
                .when_some(fill, |this, fill| this.bg(fill))
                .when(fill.is_none(), |this| this.hover(|style| style.bg(theme.hover)))
                .child(
                    div()
                        .text_color(if selected && focused { theme.on_accent } else { color.color(theme.dark) })
                        .child(glyph),
                )
                .child(SharedString::from(label))
                .child(
                    div()
                        .id(("file-close", index))
                        .text_color(if selected && focused { theme.on_accent } else { theme.faint })
                        .child(if dirty { icons::DOT } else { icons::CLOSE })
                        .on_click(cx.listener(move |view, _, window, cx| {
                            close_tab(view, &close_project, &close_file, window, cx);
                            cx.stop_propagation();
                        })),
                )
                .on_click(cx.listener(move |view, _, window, cx| {
                    open_file(view, &select_project, &select_file, None, window, cx);
                }))
                .on_mouse_down(
                    MouseButton::Middle,
                    cx.listener(move |view, _, window, cx| {
                        close_tab(view, &middle_project, &middle_file, window, cx);
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |view, event: &gpui::MouseDownEvent, _, cx| {
                        let items = tab_menu(view, &menu_project, &menu_files, index, cx);
                        view.open_menu(event.position, items, cx);
                    }),
                )
                .on_drag(dragged, |drag: &DraggedFile, _, _, cx| {
                    cx.new(|_| crate::sidebar::DragPreview(drag.title.clone()))
                })
                .drag_over::<DraggedFile>(move |style, _, _, _| style.border_l_2().border_color(accent))
                .on_drop(cx.listener(move |view, drag: &DraggedFile, window, cx| {
                    let place = FilePlace::Before(drop_file.clone());
                    move_file(view, &drop_project, &drag.file, &tab_group, place, window, cx);
                })),
        );
    }
    strip.into_any_element()
}

fn path_bar(editor: &Entity<Editor>, cx: &mut Context<WorkspaceView>) -> impl IntoElement {
    let theme = ui::theme(cx);
    let data = editor.read(cx);
    let display = if Path::new(&data.path).is_absolute() {
        crate::paths::display(Path::new(&data.path))
    } else {
        data.path.clone()
    };
    let crumbs = display.replace(['/', '\\'], " › ");
    let (changes, content) = (data.added + data.removed, data.content.clone());
    let full = data.full.clone();
    let button = |id: &'static str, glyph: &'static str| {
        div().id(id).px(px(5.0)).text_color(theme.muted).hover(|style| style.text_color(theme.accent)).child(glyph)
    };
    let (previous, next) = (editor.clone(), editor.clone());
    let (default_app, reveal) = (full.clone(), full.clone());
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(10.0))
        .px(px(10.0))
        .py(px(2.0))
        .border_b_1()
        .border_color(theme.border)
        .child(div().flex_1().min_w_0().truncate().text_color(theme.muted).child(crumbs))
        .when(changes > 0, |this| {
            this.child(
                button("change-previous", icons::ARROW_UP)
                    .on_click(move |_, _, cx| previous.update(cx, |editor, cx| editor.jump_change(false, cx))),
            )
            .child(
                button("change-next", icons::ARROW_DOWN)
                    .on_click(move |_, _, cx| next.update(cx, |editor, cx| editor.jump_change(true, cx))),
            )
        })
        .when(content != Content::Missing, |this| {
            this.child(button("file-open", icons::LINK).on_click(cx.listener(
                move |view, event: &gpui::ClickEvent, _, cx| {
                    let items = open_with_menu(view, default_app.clone(), cx);
                    view.open_menu(event.position(), items, cx);
                },
            )))
            .child(button("file-reveal", icons::REVEAL).on_click(move |_, _, cx| cx.reveal_path(&reveal)))
        })
}

fn image_view(editor: &Entity<Editor>, cx: &mut Context<WorkspaceView>) -> AnyElement {
    let theme = ui::theme(cx);
    let full = editor.read(cx).full.clone();
    let bytes = std::fs::metadata(&full).map(|meta| meta.len()).unwrap_or(0);
    let pixels = image::image_dimensions(&full).ok().map(|(w, h)| format!("{w}×{h}  ")).unwrap_or_default();
    let (dark, light) = if theme.dark {
        (gpui::rgb(0x1A1A1A), gpui::rgb(0x262626))
    } else {
        (gpui::rgb(0xE8E8E8), gpui::rgb(0xFFFFFF))
    };
    div()
        .flex()
        .flex_col()
        .size_full()
        .child(
            div()
                .relative()
                .flex_1()
                .min_h_0()
                .child(
                    canvas(
                        |_, _, _| {},
                        move |bounds: Bounds<gpui::Pixels>, _, window, _| {
                            window.paint_quad(fill(bounds, dark));
                            let square = 12.0;
                            let columns = (f32::from(bounds.size.width) / square) as usize + 1;
                            let rows = (f32::from(bounds.size.height) / square) as usize + 1;
                            window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
                                for row in 0..rows {
                                    for column in 0..columns {
                                        if (row + column) % 2 == 0 {
                                            let origin = point(
                                                bounds.origin.x + px(column as f32 * square),
                                                bounds.origin.y + px(row as f32 * square),
                                            );
                                            window.paint_quad(fill(
                                                Bounds::new(origin, size(px(square), px(square))),
                                                light,
                                            ));
                                        }
                                    }
                                }
                            });
                        },
                    )
                    .absolute()
                    .size_full(),
                )
                .child(img(full).absolute().size_full().object_fit(ObjectFit::ScaleDown)),
        )
        .child(
            div()
                .px(px(10.0))
                .border_t_1()
                .border_color(theme.border)
                .text_color(theme.muted)
                .child(format!("{pixels}{}", human_size(bytes))),
        )
        .into_any_element()
}

pub fn human_size(bytes: u64) -> String {
    match bytes {
        0..1024 => format!("{bytes} B"),
        1024..1048576 => format!("{:.1} KB", bytes as f64 / 1024.0),
        _ => format!("{:.1} MB", bytes as f64 / 1048576.0),
    }
}

fn body(editor: &Entity<Editor>, cx: &mut Context<WorkspaceView>) -> AnyElement {
    let theme = ui::theme(cx);
    let content = editor.read(cx).content.clone();
    let full = editor.read(cx).full.clone();
    match content {
        Content::Image => image_view(editor, cx),
        Content::Binary(bytes) => div()
            .id("binary")
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .text_color(theme.muted)
            .child(format!("binary file, {}", human_size(bytes)))
            .child(
                div()
                    .id("binary-open")
                    .px(px(8.0))
                    .bg(theme.accent)
                    .text_color(theme.on_accent)
                    .child("Open in Default App")
                    .on_click(move |_, _, cx| cx.open_with_system(&full)),
            )
            .into_any_element(),
        Content::Missing => div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(theme.faint)
            .child("file not found")
            .into_any_element(),
        Content::Loading => div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(theme.faint)
            .child("loading…")
            .into_any_element(),
        Content::Text => editor.clone().into_any_element(),
    }
}

pub fn move_file(
    view: &mut WorkspaceView,
    project: &Path,
    file: &str,
    group: &str,
    place: FilePlace,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) {
    view.drop_target = None;
    let workspace = view.workspace.clone();
    let (path, file, group) = (project.to_path_buf(), file.to_string(), group.to_string());
    store::store(cx).update(cx, |store, cx| {
        if let Some(target) = store.workspace_mut(&workspace).and_then(|state| state.project_mut(&path)) {
            target.place_file(&file, &group, place);
        }
        store.changed(cx);
    });
    focus_active(view, window, cx);
    cx.notify();
}

fn change_project(
    view: &mut WorkspaceView,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
    change: impl FnOnce(&mut crate::store::Project),
) {
    let workspace = view.workspace.clone();
    let path = view.current_project_path(cx).unwrap_or_default();
    store::store(cx).update(cx, |store, cx| {
        if let Some(target) = store.workspace_mut(&workspace).and_then(|state| state.project_mut(&path)) {
            change(target);
        }
        store.changed(cx);
    });
    focus_active(view, window, cx);
}

pub fn split(view: &mut WorkspaceView, direction: Direction, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    change_project(view, window, cx, |project| project.split_active(direction));
}

pub fn merge_group(view: &mut WorkspaceView, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    change_project(view, window, cx, crate::store::Project::merge_group);
}

pub fn focus_group(
    view: &mut WorkspaceView,
    direction: Direction,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) {
    change_project(view, window, cx, |project| project.focus_group(direction));
}

fn open_dropped(
    view: &mut WorkspaceView,
    project: &Path,
    group: &str,
    paths: &[PathBuf],
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) {
    let group = group.to_string();
    change_project(view, window, cx, |project| project.focused_group = Some(group));
    for path in paths {
        if path.is_dir() {
            crate::actions::open_folder(view, path.clone(), window, cx);
            continue;
        }
        let file = crate::file_ops::relative(project, path).unwrap_or_else(|| path.to_string_lossy().into_owned());
        open_file(view, project, &file, None, window, cx);
    }
}

fn group_view(
    view: &mut WorkspaceView,
    project: &crate::store::Project,
    id: &str,
    home: bool,
    cx: &mut Context<WorkspaceView>,
) -> AnyElement {
    let Some(group) = project.groups.iter().find(|group| group.id == id) else { return div().into_any_element() };
    let Some(active) = group.active.clone().or_else(|| group.files.first().cloned()) else {
        return div().into_any_element();
    };
    let focused = project.focused_index().is_some_and(|index| project.groups[index].id == id);
    let editor = editor_for(view, &project.path, &active, cx);
    let is_text = editor.read(cx).content == Content::Text;
    let focus = editor.read(cx).focus.clone();
    let chips = crate::statusbar::every_pane(cx).then(|| crate::statusbar::file_row(&editor, cx)).flatten();
    let (drop_project, drop_group, move_group, focus_id) =
        (project.path.clone(), id.to_string(), id.to_string(), id.to_string());
    let (open_project, open_group, external_project, external_group) =
        (project.path.clone(), id.to_string(), project.path.clone(), id.to_string());
    let strip = tab_strip(view, &project.path, id, &group.files, Some(&active), (focused, home), cx);
    div()
        .id(SharedString::from(format!("file-group-{id}")))
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .size_full()
        .when(!is_text, |this| this.track_focus(&focus))
        .capture_any_mouse_down(cx.listener(move |view, _, window, cx| {
            if !focused {
                let id = focus_id.clone();
                change_project(view, window, cx, |project| project.focused_group = Some(id));
            }
        }))
        .on_drop(cx.listener(move |view, drag: &crate::file_ops::DraggedPath, window, cx| {
            open_dropped(view, &open_project, &open_group, std::slice::from_ref(&drag.path), window, cx)
        }))
        .on_drop(cx.listener(move |view, paths: &gpui::ExternalPaths, window, cx| {
            open_dropped(view, &external_project, &external_group, paths.paths(), window, cx)
        }))
        .child(strip)
        .child(path_bar(&editor, cx))
        .child(
            div()
                .flex_1()
                .min_h_0()
                .child(body(&editor, cx))
                .on_drag_move::<DraggedFile>(cx.listener(
                    move |view, event: &gpui::DragMoveEvent<DraggedFile>, _, cx| {
                        let inside = event.bounds.contains(&event.event.position);
                        let mine =
                            matches!(&view.drop_target, Some(DropTarget::Files { group, .. }) if *group == move_group);
                        if !inside && !mine {
                            return;
                        }
                        let zone =
                            crate::splits_view::zone_at(event.bounds, view.drop_area.get(), event.event.position);
                        let next = inside.then(|| DropTarget::Files {
                            group: move_group.clone(),
                            file: event.drag(cx).file.clone(),
                            place: FilePlace::Zone(zone),
                        });
                        if view.drop_target != next {
                            view.drop_target = next;
                            cx.notify();
                        }
                    },
                ))
                .on_drop(cx.listener(move |view, drag: &DraggedFile, window, cx| {
                    let place = match &view.drop_target {
                        Some(DropTarget::Files { group, place, .. }) if *group == drop_group => place.clone(),
                        _ => FilePlace::Zone(Zone::Center),
                    };
                    move_file(view, &drop_project, &drag.file, &drop_group, place, window, cx);
                })),
        )
        .children(chips)
        .into_any_element()
}

fn file_name(file: &str) -> String {
    let path = spec_parts(file).0;
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

fn files_preview(project: &crate::store::Project, target: &DropTarget, cx: &App) -> Option<AnyElement> {
    let DropTarget::Files { group, file, place } = target else { return None };
    let mut result = project.clone();
    result.place_file(file, group, place.clone());
    if result.groups == project.groups && result.file_layout == project.file_layout {
        return None;
    }
    let theme = ui::theme(cx);
    let unit = Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 };
    let mut rects = Vec::new();
    match &result.file_layout {
        Some(layout) => crate::splits::rects(layout, unit, &mut rects),
        None => rects.extend(result.groups.first().map(|group| (group.id.clone(), unit))),
    }
    let focused = result.focused_index().map(|index| result.groups[index].id.clone());
    let (filled, hint) = match place {
        FilePlace::Zone(Zone::Center) | FilePlace::Before(_) => (false, "move here"),
        FilePlace::Zone(zone) => (true, crate::splits_view::zone_hint(*zone)),
    };
    let items = rects
        .into_iter()
        .filter_map(|(id, rect)| {
            let group = result.groups.iter().find(|group| group.id == id)?;
            let name = file_name(group.active.as_ref().or(group.files.first())?);
            let (glyph, color) = icons::file(&name);
            let strong = focused.as_deref() == Some(id.as_str());
            Some(crate::splits_view::Ghost {
                rect,
                glyph: (glyph, color.color(theme.dark)),
                title: name,
                strong,
                filled,
                hint: strong.then(|| hint.to_string()),
            })
        })
        .collect();
    Some(crate::splits_view::ghosts(items, cx))
}

pub fn render(view: &mut WorkspaceView, _window: &mut Window, cx: &mut Context<WorkspaceView>) -> AnyElement {
    let theme = ui::theme(cx);
    let Some(project) = view.state(cx).and_then(|state| state.current_project()).cloned() else {
        return div().into_any_element();
    };
    let order = project.group_order();
    let first = order.first().cloned().unwrap_or_default();
    let content = match &project.file_layout {
        Some(layout) => crate::splits_view::layout_tree(layout, theme.border, &mut |id| {
            group_view(view, &project, id, id == first, cx)
        }),
        None => group_view(view, &project, &first, true, cx),
    };
    let preview = view.drop_target.as_ref().and_then(|target| files_preview(&project, target, cx));
    div()
        .id("file-view")
        .key_context("FileView")
        .relative()
        .flex()
        .size_full()
        .bg(theme.background)
        .on_action(cx.listener(|view, _: &crate::commands::HideFileView, window, cx| hide(view, window, cx)))
        .child(content)
        .child(crate::splits_view::area_probe(view))
        .children(preview)
        .into_any_element()
}
