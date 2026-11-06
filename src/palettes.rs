use std::path::{Path, PathBuf};

use gpui::{App, AppContext, Context, PromptLevel, Window};

use crate::commands::COMMANDS;
use crate::icons;
use crate::palette::{Item, Palette};
use crate::paths;
use crate::persist::Selected;
use crate::settings;
use crate::store::{self, Launcher};
use crate::ui;
use crate::window::WorkspaceView;

fn select_anywhere(
    view: &mut WorkspaceView,
    workspace: String,
    selected: Selected,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) {
    if workspace == view.workspace {
        view.select(Some(selected), window, cx);
        return;
    }
    cx.defer(move |cx| {
        if let Some(handle) = crate::window::open(&workspace, cx) {
            let _ = handle.update(cx, |view, window, cx| view.select(Some(selected), window, cx));
        }
    });
}

pub fn list_files(dir: &Path) -> Vec<String> {
    if let Some(files) = crate::git::files(dir) {
        return files;
    }
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(folder) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else { continue };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name == "node_modules" || name == "target" {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(relative) = path.strip_prefix(dir) {
                files.push(relative.to_string_lossy().replace('\\', "/"));
            }
            if files.len() >= 20000 {
                return files;
            }
        }
    }
    files.sort();
    files
}

fn file_items(project: &Path, recent: &[String], files: &[String], cx: &App) -> Vec<Item> {
    let theme = ui::theme(cx);
    let mut items = Vec::new();
    for (index, path) in recent.iter().chain(files.iter().filter(|file| !recent.contains(file))).enumerate() {
        let name = path.rsplit('/').next().unwrap_or(path);
        let (glyph, color) = icons::file(name);
        let target = project.to_path_buf();
        let relative = path.clone();
        let full = project.join(path);
        let mut item = Item::new(path.clone(), move |view, window, cx| {
            crate::file_view::open_file(view, &target, &relative, None, window, cx)
        })
        .glyph(glyph, Some(color.color(theme.dark)))
        .rank(if index < recent.len() { 30 - index as i32 } else { 0 });
        if index < recent.len() {
            item = item.subtitle("recently opened");
        }
        item.path = true;
        item.alternate = Some(std::rc::Rc::new(move |_, _, cx| cx.open_with_system(&full)));
        items.push(item);
    }
    items
}

fn load_files(
    project: PathBuf,
    palette: gpui::Entity<Palette>,
    recent: Vec<String>,
    as_files: bool,
    cx: &mut Context<WorkspaceView>,
) {
    let progress = store::begin_progress("listing files", cx);
    cx.spawn(async move |_, cx| {
        let dir = project.clone();
        let files = cx.background_executor().spawn(async move { list_files(&dir) }).await;
        let _ = cx.update(|cx| {
            store::finish_progress(progress, cx);
            palette.update(cx, |palette, cx| {
                let mut extra = file_items(&project, &recent, &files, cx);
                if !as_files {
                    for item in extra.iter_mut() {
                        item.rank -= 50;
                        item.detail = "file".into();
                        item.hidden_when_empty = true;
                    }
                    let mut items: Vec<Item> = palette.items.iter().filter(|item| !item.path).cloned().collect();
                    items.extend(extra);
                    palette.set_items(items, cx);
                } else {
                    palette.set_items(extra, cx);
                }
            });
        });
    })
    .detach();
}

pub fn quick_open(view: &mut WorkspaceView, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let Some(project) =
        view.state(cx).and_then(|state| state.current_project()).filter(|project| !project.is_scratch()).cloned()
    else {
        crate::actions::command_palette(view, window, cx);
        return;
    };
    let items = file_items(&project.path, &project.recent_files, &[], cx);
    let placeholder = format!("search files in {}", project.display_name());
    let palette = cx.new(|cx| Palette::new("open", &placeholder, items, 0, cx));
    load_files(project.path.clone(), palette.clone(), project.recent_files.clone(), true, cx);
    view.show_palette(palette, window, cx);
}

pub fn everything(view: &WorkspaceView, cx: &mut Context<WorkspaceView>) -> Vec<Item> {
    let theme = ui::theme(cx);
    let store = view.store(cx);
    let mut items = Vec::new();
    for workspace in store.workspaces.iter().filter(|workspace| !workspace.closed) {
        for project in workspace.all_projects() {
            for id in &project.tabs {
                let Some(session) = store.session(id) else { continue };
                let session = session.read(cx);
                let (workspace_id, tab) = (workspace.id.clone(), id.clone());
                let (glyph, color) = crate::sidebar::tab_glyph(session);
                items.push(
                    Item::new(session.display_title(), move |view, window, cx| {
                        select_anywhere(view, workspace_id.clone(), Selected::Tab(tab.clone()), window, cx)
                    })
                    .subtitle(format!("{} · {}", project.display_name(), paths::display(session.working_dir())))
                    .glyph(glyph, color.map(|color| color.color(theme.dark)))
                    .detail("tab")
                    .rank(if workspace.id == view.workspace { 12 } else { 8 }),
                );
            }
        }
    }
    for workspace in store.workspaces.iter() {
        for project in &workspace.projects {
            let (workspace_id, path) = (workspace.id.clone(), project.path.clone());
            items.push(
                Item::new(project.display_name(), move |view, window, cx| {
                    select_anywhere(view, workspace_id.clone(), Selected::Project(path.clone()), window, cx)
                })
                .subtitle(paths::display(&project.path))
                .glyph(icons::FOLDER, Some(project.color.map(|color| color.color(theme.dark)).unwrap_or(theme.accent)))
                .detail("project")
                .rank(10),
            );
        }
    }
    for workspace in store.workspaces.iter() {
        let id = workspace.id.clone();
        let subtitle =
            format!("{} projects{}", workspace.projects.len(), if workspace.closed { ", closed" } else { "" });
        items.push(
            Item::new(workspace.name.clone(), move |_, _, cx| {
                let id = id.clone();
                cx.defer(move |cx| {
                    crate::window::open(&id, cx);
                });
            })
            .subtitle(subtitle)
            .glyph(icons::WINDOW, None)
            .detail("workspace")
            .rank(6),
        );
    }
    let target = view.current_project_path(cx).unwrap_or_default();
    for profile in settings::get(cx).agents.iter().filter(|profile| profile.is_launchable()) {
        let (target, launcher) = (target.clone(), Launcher::Agent(profile.clone()));
        let (glyph, color) = icons::agent(&crate::agents::command_name(&profile.executable));
        items.push(
            Item::new(format!("New Tab: {}", profile.name), move |view, window, cx| {
                crate::actions::launch(view, &target, launcher.clone(), window, cx)
            })
            .subtitle(profile.summary())
            .glyph(glyph, Some(color.color(theme.dark)))
            .detail("agent")
            .rank(4),
        );
    }
    for shell in settings::get(cx).shells.iter().filter(|shell| shell.is_launchable()) {
        let (target, launcher) = (target.clone(), Launcher::Shell(shell.clone()));
        items.push(
            Item::new(format!("New Shell: {}", shell.name), move |view, window, cx| {
                crate::actions::launch(view, &target, launcher.clone(), window, cx)
            })
            .subtitle(shell.path.clone())
            .glyph(icons::TERMINAL, None)
            .detail("shell")
            .rank(4),
        );
    }
    if let Some(list) = store.conversations_for(&target) {
        for conversation in list.iter().cloned() {
            let (target, title) = (target.clone(), conversation.title.clone());
            let subtitle =
                format!("{} · {}", conversation.profile, crate::history::relative_age(conversation.modified));
            let (glyph, color) = icons::agent(conversation.kind.command());
            let launcher = Launcher::Resume(conversation);
            items.push(
                Item::new(title, move |view, window, cx| {
                    crate::actions::launch(view, &target, launcher.clone(), window, cx)
                })
                .subtitle(subtitle)
                .glyph(glyph, Some(color.color(theme.dark)))
                .detail("conversation")
                .rank(2),
            );
        }
    }
    for command in COMMANDS.iter().filter(|command| !matches!(command.name, "command_palette" | "quick_open")) {
        if !cfg!(target_os = "macos") && matches!(command.name, "hide" | "hide_others" | "show_all") {
            continue;
        }
        let action = command.action;
        items.push(
            Item::new(command.title, move |_, window, cx| window.dispatch_action(action(), cx))
                .subtitle(command.name)
                .glyph(icons::COMMAND, Some(theme.faint))
                .detail(crate::commands::key_for(command.name, cx)),
        );
    }
    let closed: Vec<_> = store.workspaces.iter().filter(|workspace| workspace.closed).cloned().collect();
    for workspace in closed {
        let id = workspace.id.clone();
        items.push(
            Item::new(format!("Reopen Workspace: {}", workspace.name), move |_, _, cx| {
                let id = id.clone();
                cx.defer(move |cx| {
                    crate::window::open(&id, cx);
                });
            })
            .glyph(icons::WINDOW, None)
            .detail("workspace"),
        );
    }
    items
}

pub fn themes(view: &mut WorkspaceView, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let current = settings::get(cx).theme.clone();
    let mut initial = 0;
    let items: Vec<Item> = crate::theme::all()
        .into_iter()
        .enumerate()
        .map(|(index, (id, name))| {
            if id == current {
                initial = index;
            }
            let theme_id = id.to_string();
            let preview_id = id.to_string();
            let mut item =
                Item::new(name, move |_, _, cx| settings::update(cx, |settings| settings.theme = theme_id.clone()))
                    .glyph(icons::THEME, None)
                    .detail(if crate::theme::is_automatic(id) { "light + dark" } else { "" });
            item.preview = Some(std::rc::Rc::new(move |_, _, cx| {
                let mut settings = settings::get(cx).clone();
                settings.theme = preview_id.clone();
                cx.set_global(settings);
            }));
            item
        })
        .collect();
    let palette = cx.new(|cx| Palette::new("theme", "select a theme", items, initial, cx));
    view.show_palette(palette, window, cx);
}

pub fn branches(view: &mut WorkspaceView, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let Some(dir) = view
        .selected_session(cx)
        .map(|session| session.read(cx).working_dir().to_path_buf())
        .or_else(|| view.current_project_path(cx))
    else {
        return;
    };
    let current = crate::git::current_branch(&dir);
    let list = crate::git::branches(&dir, 30);
    let mut items = Vec::new();
    for branch in list {
        let (target, name) = (dir.clone(), branch.clone());
        items.push(
            Item::new(branch.clone(), move |_, window, cx| {
                let (target, name) = (target.clone(), name.clone());
                run_git(format!("git switch {name}"), window, cx, move || crate::git::switch(&target, &name))
            })
            .glyph(icons::BRANCH, None)
            .detail(if Some(&branch) == current.as_ref() { "current" } else { "" }),
        );
    }
    let target = dir.clone();
    items.push(
        Item::new("New Branch…", move |view, window, cx| {
            let target = target.clone();
            crate::actions::prompt(view, "new branch", "", window, cx, move |name, _, window, cx| {
                if !name.is_empty() {
                    let target = target.clone();
                    run_git(format!("git switch -c {name}"), window, cx, move || {
                        crate::git::create_branch(&target, &name)
                    });
                }
            });
        })
        .glyph(icons::ADD, None),
    );
    let palette = cx.new(|cx| Palette::new("branch", "switch branch", items, 0, cx));
    view.show_palette(palette, window, cx);
}

fn run_git(
    text: String,
    window: &mut Window,
    cx: &mut App,
    action: impl FnOnce() -> Result<(), String> + Send + 'static,
) {
    let handle = window.window_handle();
    let progress = store::begin_progress(text, cx);
    cx.spawn(async move |cx| {
        let result = cx.background_executor().spawn(async move { action() }).await;
        let _ = cx.update(|cx| {
            store::finish_progress(progress, cx);
            if let Err(error) = result {
                let _ = handle.update(cx, |_, window, cx| {
                    drop(window.prompt(PromptLevel::Warning, "git failed", Some(&error), &["OK"], cx));
                });
            }
            store::store(cx).update(cx, |_, cx| cx.notify());
        });
    })
    .detach();
}
