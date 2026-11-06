use std::collections::HashMap;
use std::path::{Path, PathBuf};

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, AppContext, Bounds, Context, Entity, InteractiveElement, IntoElement, MouseButton, ObjectFit,
    ParentElement, PathPromptOptions, PromptLevel, SharedString, StatefulInteractiveElement, Styled, StyledImage,
    Window, canvas, div, fill, img, point, px, size,
};

use crate::editor::{Content, Editor};
use crate::icons;
use crate::menu::{self, Item};
use crate::persist::Selected;
use crate::store;
use crate::ui;
use crate::window::WorkspaceView;

#[derive(Default)]
pub struct Files {
    pub open: HashMap<String, Entity<Editor>>,
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
    let editor = cx.new(|cx| Editor::new(root, path, commit, cx));
    cx.observe(&editor, |_, _, cx| cx.notify()).detach();
    view.files.open.insert(id, editor.clone());
    editor
}

pub fn active_editor(view: &WorkspaceView, cx: &App) -> Option<Entity<Editor>> {
    let project = view.state(cx)?.current_project()?;
    let file = project.active_file.as_ref()?;
    view.files.open.get(&key(&project.path, file)).cloned()
}

pub fn focus_active(view: &mut WorkspaceView, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let Some((path, file)) = view
        .state(cx)
        .and_then(|state| state.current_project())
        .and_then(|project| project.active_file.clone().map(|file| (project.path.clone(), file)))
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
    let workspace = view.workspace.clone();
    let path = project.to_path_buf();
    let name = relative.to_string();
    store::store(cx).update(cx, |store, cx| {
        let Some(state) = store.workspace_mut(&workspace) else { return };
        let Some(target) = state.project_mut(&path) else { return };
        if !target.open_files.contains(&name) {
            let index =
                target.active_file.as_ref().and_then(|active| target.open_files.iter().position(|file| file == active));
            target.open_files.insert(index.map_or(target.open_files.len(), |index| index + 1), name.clone());
        }
        target.active_file = Some(name.clone());
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
        if let Some(target) = store.workspace_mut(&workspace).and_then(|state| state.project_mut(&path))
            && let Some(index) = target.open_files.iter().position(|open| *open == name)
        {
            target.open_files.remove(index);
            if target.active_file.as_deref() == Some(name.as_str()) {
                target.active_file =
                    target.open_files.get(index).or_else(|| target.open_files.get(index.wrapping_sub(1))).cloned();
                target.file_shown = target.file_shown && target.active_file.is_some();
            }
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

pub fn close_active(view: &mut WorkspaceView, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let Some((path, file)) = view
        .state(cx)
        .and_then(|state| state.current_project())
        .and_then(|project| project.active_file.clone().map(|file| (project.path.clone(), file)))
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

pub fn cursor_label(view: &WorkspaceView, cx: &App) -> Option<String> {
    if !view.file_shown(cx) {
        return None;
    }
    let editor = active_editor(view, cx)?;
    let editor = editor.read(cx);
    let language = crate::highlight::language_name(&editor.full).unwrap_or("Plain Text");
    Some(format!("{}  {language}", editor.position_label()))
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

fn tab_strip(
    view: &WorkspaceView,
    project: &Path,
    files: &[String],
    active: Option<&str>,
    cx: &mut Context<WorkspaceView>,
) -> impl IntoElement {
    let theme = ui::theme(cx);
    let mut strip = div()
        .id("file-tabs")
        .flex()
        .flex_none()
        .items_center()
        .overflow_x_scroll()
        .border_b_1()
        .border_color(theme.border)
        .bg(theme.surface)
        .child(
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
                        if let Some(target) = store.workspace_mut(&workspace).and_then(|state| state.project_mut(&path))
                        {
                            target.file_shown = false;
                        }
                        store.changed(cx);
                    });
                    view.focus_current(window, cx);
                })),
        );
    for (index, file) in files.iter().enumerate() {
        let (path, commit) = spec_parts(file);
        let name = path.rsplit(['/', '\\']).next().unwrap_or(path).to_string();
        let (glyph, color) = icons::file(&name);
        let selected = active == Some(file.as_str());
        let dirty = view.files.open.get(&key(project, file)).is_some_and(|editor| editor.read(cx).dirty);
        let (select_project, select_file) = (project.to_path_buf(), file.clone());
        let (close_project, close_file) = (project.to_path_buf(), file.clone());
        let (middle_project, middle_file) = (project.to_path_buf(), file.clone());
        let label = match commit {
            Some(commit) => format!("{name} @ {}", &commit[..commit.len().min(7)]),
            None => name,
        };
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
                .when(selected, |this| this.bg(theme.accent).text_color(theme.on_accent))
                .when(!selected, |this| this.text_color(theme.muted).hover(|style| style.bg(theme.hover)))
                .child(div().text_color(if selected { theme.on_accent } else { color.color(theme.dark) }).child(glyph))
                .child(SharedString::from(label))
                .child(
                    div()
                        .id(("file-close", index))
                        .text_color(if selected { theme.on_accent } else { theme.faint })
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
                ),
        );
    }
    strip
}

fn path_bar(view: &WorkspaceView, editor: &Entity<Editor>, cx: &mut Context<WorkspaceView>) -> impl IntoElement {
    let theme = ui::theme(cx);
    let data = editor.read(cx);
    let display = if Path::new(&data.path).is_absolute() {
        crate::paths::display(Path::new(&data.path))
    } else {
        data.path.clone()
    };
    let crumbs = display.replace(['/', '\\'], "  ›  ");
    let (added, removed, content) = (data.added, data.removed, data.content.clone());
    let full = data.full.clone();
    let commit = data.commit.clone();
    let button = |id: &'static str, glyph: &'static str| {
        div().id(id).px(px(5.0)).text_color(theme.muted).hover(|style| style.text_color(theme.accent)).child(glyph)
    };
    let (previous, next) = (editor.clone(), editor.clone());
    let (default_app, reveal) = (full.clone(), full.clone());
    let _ = view;
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
        .when_some(commit, |this, commit| {
            this.child(div().text_color(theme.yellow).child(format!(
                "{} {}",
                icons::COMMIT,
                &commit[..commit.len().min(7)]
            )))
        })
        .when(added + removed > 0, |this| {
            this.child(div().text_color(theme.green).child(format!("+{added}")))
                .child(div().text_color(theme.red).child(format!("-{removed}")))
                .child(
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

pub fn render(view: &mut WorkspaceView, _window: &mut Window, cx: &mut Context<WorkspaceView>) -> AnyElement {
    let theme = ui::theme(cx);
    let Some(project) = view.state(cx).and_then(|state| state.current_project()).cloned() else {
        return div().into_any_element();
    };
    let Some(active) = project.active_file.clone() else { return div().into_any_element() };
    let editor = editor_for(view, &project.path, &active, cx);
    let content = editor.read(cx).content.clone();
    let full = editor.read(cx).full.clone();
    let body: AnyElement = match content {
        Content::Image => image_view(&editor, cx),
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
    };
    let is_text = editor.read(cx).content == Content::Text;
    let focus = editor.read(cx).focus.clone();
    div()
        .id("file-view")
        .key_context("FileView")
        .when(!is_text, |this| this.track_focus(&focus))
        .flex()
        .flex_col()
        .size_full()
        .bg(theme.background)
        .on_action(cx.listener(|view, _: &crate::commands::HideFileView, window, cx| hide(view, window, cx)))
        .child(tab_strip(view, &project.path, &project.open_files, Some(&active), cx))
        .child(path_bar(view, &editor, cx))
        .child(div().flex_1().min_h_0().child(body))
        .into_any_element()
}
