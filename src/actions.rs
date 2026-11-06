use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui::{
    App, AppContext, Context, ExternalPaths, Focusable, InteractiveElement, PathPromptOptions, PromptLevel, Stateful,
    Window,
};

use crate::agents;
use crate::commands::*;
use crate::history::Conversation;
use crate::icons;
use crate::menu::{self, Item};
use crate::palette::{self, Palette};
use crate::persist::{PanelTab, Selected, Side, SidebarTab};
use crate::session::Kind;
use crate::settings;
use crate::store::{self, Launcher, Workspace};
use crate::text_input::{InputEvent, TextInput};
use crate::window::{Rename, WorkspaceView};

pub fn reveal_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "Reveal in Finder"
    } else if cfg!(windows) {
        "Reveal in Explorer"
    } else {
        "Reveal in File Manager"
    }
}

fn current_target(view: &WorkspaceView, cx: &App) -> PathBuf {
    view.current_project_path(cx).unwrap_or_default()
}

pub fn launch(
    view: &mut WorkspaceView,
    project: &Path,
    launcher: Launcher,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) {
    let workspace = view.workspace.clone();
    let project = project.to_path_buf();
    let id = store::store(cx).update(cx, |store, cx| store.open_tab(&workspace, &project, launcher, None, cx));
    if let Some(id) = id {
        view.select_tab(&id, window, cx);
    }
}

pub fn last_launcher(view: &WorkspaceView, project: &Path, cx: &App) -> Option<Launcher> {
    let settings = settings::get(cx);
    let store = view.store(cx);
    let names = [
        view.state(cx).and_then(|state| state.project(project)).and_then(|project| project.last_launcher.clone()),
        store.last_launcher.clone(),
    ];
    for name in names.into_iter().flatten() {
        if let Some(profile) = settings.agents.iter().find(|profile| profile.name == name && profile.is_launchable()) {
            return Some(Launcher::Agent(profile.clone()));
        }
        if let Some(shell) = settings.shells.iter().find(|shell| shell.name == name && shell.is_launchable()) {
            return Some(Launcher::Shell(shell.clone()));
        }
    }
    settings
        .agents
        .iter()
        .find(|profile| profile.is_launchable())
        .map(|profile| Launcher::Agent(profile.clone()))
        .or_else(|| {
            settings.shells.iter().find(|shell| shell.is_launchable()).map(|shell| Launcher::Shell(shell.clone()))
        })
}

fn new_tab(view: &mut WorkspaceView, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let target = current_target(view, cx);
    match last_launcher(view, &target, cx) {
        Some(launcher) => launch(view, &target, launcher, window, cx),
        None => picker(view, &target, window, cx),
    }
}

pub fn picker(view: &mut WorkspaceView, project: &Path, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let settings = settings::get(cx).clone();
    let theme = crate::ui::theme(cx);
    let last = view
        .state(cx)
        .and_then(|state| state.project(project))
        .and_then(|project| project.last_launcher.clone())
        .or(view.store(cx).last_launcher.clone());
    let mut items = Vec::new();
    let mut digit = 0;
    let mut initial = 0;
    for profile in settings.agents.iter().filter(|profile| profile.is_launchable()) {
        digit += 1;
        if Some(&profile.name) == last.as_ref() {
            initial = items.len();
        }
        let target = project.to_path_buf();
        let launcher = Launcher::Agent(profile.clone());
        let (glyph, color) = icons::agent(&agents::command_name(&profile.executable));
        let mut item = palette::Item::new(profile.name.clone(), move |view, window, cx| {
            launch(view, &target, launcher.clone(), window, cx)
        })
        .subtitle(profile.summary())
        .glyph(glyph, Some(color.color(theme.dark)))
        .detail(if digit <= 9 { digit.to_string() } else { String::new() })
        .section("agents");
        item.digit = Some(digit);
        items.push(item);
    }
    for shell in settings.shells.iter().filter(|shell| shell.is_launchable()) {
        digit += 1;
        if Some(&shell.name) == last.as_ref()
            && !settings.agents.iter().any(|profile| Some(&profile.name) == last.as_ref())
        {
            initial = items.len();
        }
        let target = project.to_path_buf();
        let launcher = Launcher::Shell(shell.clone());
        let mut item = palette::Item::new(shell.name.clone(), move |view, window, cx| {
            launch(view, &target, launcher.clone(), window, cx)
        })
        .subtitle(shell.path.clone())
        .glyph(icons::TERMINAL, None)
        .detail(if digit <= 9 { digit.to_string() } else { String::new() })
        .section("shells");
        item.digit = Some(digit);
        items.push(item);
    }
    let open: Vec<String> = view
        .state(cx)
        .and_then(|state| state.project(project))
        .map(|project| {
            project
                .tabs
                .iter()
                .filter_map(|id| view.store(cx).session(id))
                .filter_map(|session| session.read(cx).conversation.clone())
                .collect()
        })
        .unwrap_or_default();
    if let Some(list) = view.store(cx).conversations_for(project).cloned() {
        for conversation in list.into_iter().take(15) {
            let target = project.to_path_buf();
            let detail = format!(
                "{} · {}{}",
                conversation.profile,
                crate::history::relative_age(conversation.modified),
                if open.contains(&conversation.id) { " · open" } else { "" }
            );
            let launcher = Launcher::Resume(conversation.clone());
            let (glyph, color) = icons::agent(conversation.kind.command());
            items.push(
                palette::Item::new(conversation.title.clone(), move |view, window, cx| {
                    launch(view, &target, launcher.clone(), window, cx)
                })
                .glyph(glyph, Some(color.color(theme.dark)))
                .detail(detail)
                .section("conversations"),
            );
        }
    }
    let name = view
        .state(cx)
        .and_then(|state| state.project(project))
        .map(|project| project.display_name())
        .unwrap_or_default();
    let palette = cx.new(|cx| {
        let mut palette = Palette::new("new tab", &format!("in {name}"), items, initial, cx);
        palette.digits = true;
        palette
    });
    view.show_palette(palette, window, cx);
    let path = project.to_path_buf();
    store::store(cx).update(cx, |store, cx| store.refresh_conversations(path, cx));
}

pub fn prompt(
    view: &mut WorkspaceView,
    title: &str,
    value: &str,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
    done: impl Fn(String, &mut WorkspaceView, &mut Window, &mut Context<WorkspaceView>) + 'static,
) {
    let done = Rc::new(done);
    let item = palette::Item::new(title.to_string(), move |view, window, cx| {
        let text = view.prompt_text.trim().to_string();
        done(text, view, window, cx)
    });
    view.prompt_text = value.to_string();
    let value = value.to_string();
    let palette = cx.new(|cx| {
        let mut palette = Palette::new(title, "", vec![item], 0, cx);
        palette.prompt = true;
        palette.input.update(cx, |input, cx| input.set_text(&value, cx));
        palette
    });
    view.show_palette(palette, window, cx);
}

pub fn choose_folder(
    view: &mut WorkspaceView,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
    done: impl FnOnce(PathBuf, &mut WorkspaceView, &mut Window, &mut Context<WorkspaceView>) + 'static,
) {
    let receiver = cx.prompt_for_paths(PathPromptOptions {
        files: false,
        directories: true,
        multiple: false,
        prompt: Some("Open".into()),
    });
    let _ = view;
    cx.spawn_in(window, async move |this, cx| {
        if let Ok(Ok(Some(paths))) = receiver.await
            && let Some(path) = paths.into_iter().next()
        {
            let _ = this.update_in(cx, |view, window, cx| done(path, view, window, cx));
        }
    })
    .detach();
}

pub fn open_folder(
    view: &mut WorkspaceView,
    path: PathBuf,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) -> PathBuf {
    let workspace = view.workspace.clone();
    let (owner, path) = store::store(cx).update(cx, |store, cx| store.add_project(&workspace, path, cx));
    if owner == view.workspace {
        view.select(Some(Selected::Project(path.clone())), window, cx);
    } else {
        cx.defer(move |cx| {
            if let Some(handle) = crate::window::open(&owner, cx) {
                let _ = handle.update(cx, |view, window, cx| view.select(Some(Selected::Project(path)), window, cx));
            }
        });
    }
    PathBuf::new()
}

pub fn show_project(view: &mut WorkspaceView, path: &Path, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    view.select(Some(Selected::Project(path.to_path_buf())), window, cx);
    let workspace = view.workspace.clone();
    store::store(cx).update(cx, |store, cx| {
        if let Some(project) = store.workspace_mut(&workspace).and_then(|state| state.project_mut(path)) {
            project.file_shown = project.active_file.is_some();
        }
        store.changed(cx);
    });
    view.focus_current(window, cx);
}

pub fn new_terminal_in(view: &mut WorkspaceView, path: &Path, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let workspace = view.workspace.clone();
    let path = path.to_path_buf();
    let id = store::store(cx).update(cx, |store, cx| store.open_tab(&workspace, &path, Launcher::Terminal, None, cx));
    if view.current_project_path(cx).as_deref() != Some(path.as_path()) {
        view.select(Some(Selected::Project(path.clone())), window, cx);
    }
    if let Some(id) = id
        && let Some(session) = view.store(cx).session(&id)
    {
        let focus = session.read(cx).focus.clone();
        window.focus(&focus);
    }
    cx.notify();
}

pub fn resume_anywhere(
    view: &mut WorkspaceView,
    conversation: Conversation,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) {
    let path = conversation.cwd.clone();
    let workspace = view.workspace.clone();
    let (owner, path) = store::store(cx).update(cx, |store, cx| store.add_project(&workspace, path, cx));
    if owner == view.workspace {
        launch(view, &path, Launcher::Resume(conversation), window, cx);
    } else {
        cx.defer(move |cx| {
            if let Some(handle) = crate::window::open(&owner, cx) {
                let _ = handle
                    .update(cx, |view, window, cx| launch(view, &path, Launcher::Resume(conversation), window, cx));
            }
        });
    }
}

fn running_agent(view: &WorkspaceView, id: &str, cx: &App) -> bool {
    view.store(cx).session(id).is_some_and(|session| {
        let session = session.read(cx);
        session.is_running()
            && (session.kind == Kind::Agent || session.running_agent.is_some())
            && crate::terminal::now_ms()
                .saturating_sub(session.terminal.activity.last_output.load(std::sync::atomic::Ordering::Acquire))
                < 2 * 3600 * 1000
    })
}

pub fn close_tab(view: &mut WorkspaceView, id: &str, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let skip = view.store(cx).skip_close_confirm_until > crate::terminal::now_ms();
    let id = id.to_string();
    if !running_agent(view, &id, cx) || skip {
        store::store(cx).update(cx, |store, cx| store.close_tab(&id, cx));
        view.focus_current(window, cx);
        return;
    }
    let title = view.store(cx).session(&id).map(|session| session.read(cx).display_title()).unwrap_or_default();
    let answer = window.prompt(
        PromptLevel::Warning,
        &format!("Close {title}?"),
        Some("The agent is still running and will be stopped."),
        &["Close", "Close and Don't Ask for 1 Hour", "Cancel"],
        cx,
    );
    cx.spawn_in(window, async move |this, cx| {
        let Ok(choice) = answer.await else { return };
        if choice == 2 {
            return;
        }
        let _ = this.update_in(cx, |view, window, cx| {
            store::store(cx).update(cx, |store, cx| {
                if choice == 1 {
                    store.skip_close_confirm_until = crate::terminal::now_ms() + 3600 * 1000;
                }
                store.close_tab(&id, cx);
            });
            view.focus_current(window, cx);
        });
    })
    .detach();
}

fn close(view: &mut WorkspaceView, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    if view.file_shown(cx) {
        crate::file_view::close_active(view, window, cx);
        return;
    }
    if let Some(id) = crate::panel::focused_terminal(view, window, cx) {
        store::store(cx).update(cx, |store, cx| store.close_tab(&id, cx));
        view.focus_current(window, cx);
        return;
    }
    if let Some(id) = view.state(cx).and_then(|state| state.selected_tab()).map(String::from) {
        close_tab(view, &id, window, cx);
    }
}

pub fn close_project(view: &mut WorkspaceView, path: &Path, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let Some(project) = view.state(cx).and_then(|state| state.project(path)).cloned() else { return };
    if project.is_scratch() {
        return;
    }
    let running = project.tabs.iter().any(|id| running_agent(view, id, cx));
    let path = path.to_path_buf();
    let finish = move |view: &mut WorkspaceView, window: &mut Window, cx: &mut Context<WorkspaceView>| {
        let workspace = view.workspace.clone();
        store::store(cx).update(cx, |store, cx| {
            let ids: Vec<String> = store
                .workspace(&workspace)
                .and_then(|state| state.project(&path))
                .map(|project| project.tabs.iter().chain(&project.terminals).cloned().collect())
                .unwrap_or_default();
            for id in ids {
                store.close_tab(&id, cx);
            }
            if let Some(state) = store.workspace_mut(&workspace) {
                state.projects.retain(|project| project.path != path);
                if matches!(&state.selected, Some(Selected::Project(selected)) if *selected == path) {
                    state.selected = None;
                }
            }
            store.changed(cx);
        });
        view.focus_current(window, cx);
    };
    if !running {
        finish(view, window, cx);
        return;
    }
    let answer = window.prompt(
        PromptLevel::Warning,
        &format!("Close {}?", project.display_name()),
        Some("Its running agents will be stopped."),
        &["Close Project", "Cancel"],
        cx,
    );
    cx.spawn_in(window, async move |this, cx| {
        if answer.await == Ok(0) {
            let _ = this.update_in(cx, |view, window, cx| finish(view, window, cx));
        }
    })
    .detach();
}

pub fn begin_rename(view: &mut WorkspaceView, target: Rename, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let current = match &target {
        Rename::Project(path) => view
            .state(cx)
            .and_then(|state| state.project(path))
            .and_then(|project| project.name.clone())
            .unwrap_or_else(|| {
                view.state(cx)
                    .and_then(|state| state.project(path))
                    .map(|project| project.display_name())
                    .unwrap_or_default()
            }),
        Rename::Tab(id) => {
            view.store(cx).session(id).map(|session| session.read(cx).display_title()).unwrap_or_default()
        }
    };
    if matches!(&target, Rename::Project(path) if path.as_os_str().is_empty()) {
        return;
    }
    let input = cx.new(|cx| {
        let mut input = TextInput::new("", cx);
        input.set_text(&current, cx);
        input
    });
    cx.subscribe_in(&input, window, |view, _, event, window, cx| match event {
        InputEvent::Submit => finish_rename(view, true, window, cx),
        InputEvent::Cancel => finish_rename(view, false, window, cx),
        _ => {}
    })
    .detach();
    input.read(cx).focus(window);
    let focus = input.read(cx).focus_handle(cx);
    cx.on_blur(&focus, window, |view, window, cx| finish_rename(view, true, window, cx)).detach();
    view.rename = Some((target, input));
    cx.notify();
}

fn finish_rename(view: &mut WorkspaceView, apply: bool, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let Some((target, input)) = view.rename.take() else { return };
    if apply {
        let text = input.read(cx).text.trim().to_string();
        let name = (!text.is_empty()).then_some(text);
        let workspace = view.workspace.clone();
        store::store(cx).update(cx, |store, cx| {
            match &target {
                Rename::Project(path) => {
                    if let Some(project) = store.workspace_mut(&workspace).and_then(|state| state.project_mut(path)) {
                        let folder = project.path.file_name().map(|name| name.to_string_lossy().into_owned());
                        project.name = name.filter(|name| Some(name) != folder.as_ref());
                    }
                }
                Rename::Tab(id) => {
                    if let Some(session) = store.sessions.get(id) {
                        session.update(cx, |session, cx| {
                            session.name = None;
                            let automatic = session.display_title();
                            session.name = name.filter(|name| *name != automatic);
                            cx.notify();
                        });
                    }
                }
            }
            store.changed(cx);
        });
    }
    view.focus_current(window, cx);
    cx.notify();
}

pub fn move_tab(
    view: &mut WorkspaceView,
    id: &str,
    project: &Path,
    before: Option<String>,
    cx: &mut Context<WorkspaceView>,
) {
    let workspace = view.workspace.clone();
    let id = id.to_string();
    let project = project.to_path_buf();
    store::store(cx).update(cx, |store, cx| {
        let Some(source) = store.workspaces.iter().position(|state| state.project_of(&id).is_some()) else { return };
        let Some(source_project) = store.workspaces[source].project_of_mut(&id) else { return };
        if !source_project.tabs.contains(&id) {
            return;
        }
        source_project.tabs.retain(|tab| *tab != id);
        if source_project.active_tab.as_deref() == Some(&id) {
            source_project.active_tab = source_project.tabs.first().cloned();
        }
        let Some(target) = store.workspace_mut(&workspace).and_then(|state| state.project_mut(&project)) else {
            return;
        };
        let index =
            before.and_then(|before| target.tabs.iter().position(|tab| *tab == before)).unwrap_or(target.tabs.len());
        target.tabs.insert(index, id.clone());
        let cwd = target.cwd();
        if let Some(session) = store.sessions.get(&id) {
            session.update(cx, |session, _| session.project = cwd);
        }
        store.changed(cx);
    });
}

pub fn move_project(view: &mut WorkspaceView, from: &Path, to: &Path, cx: &mut Context<WorkspaceView>) {
    let workspace = view.workspace.clone();
    store::store(cx).update(cx, |store, cx| {
        let Some(state) = store.workspace_mut(&workspace) else { return };
        let Some(index) = state.projects.iter().position(|project| project.path == from) else { return };
        let project = state.projects.remove(index);
        let target = state.projects.iter().position(|project| project.path == to).unwrap_or(state.projects.len());
        state.projects.insert(target, project);
        store.changed(cx);
    });
}

pub fn move_project_to_workspace(
    view: &mut WorkspaceView,
    path: &Path,
    target: Option<String>,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) {
    let source = view.workspace.clone();
    let path = path.to_path_buf();
    let move_to = move |target: String, cx: &mut App| {
        store::store(cx).update(cx, |store, cx| {
            let Some(state) = store.workspace_mut(&source) else { return };
            let Some(index) = state.projects.iter().position(|project| project.path == path) else { return };
            let project = state.projects.remove(index);
            if matches!(&state.selected, Some(Selected::Project(selected)) if *selected == path) {
                state.selected = None;
            }
            if let Some(selected) = state.selected_tab().map(String::from)
                && project.tabs.contains(&selected)
            {
                state.selected = None;
            }
            if let Some(destination) = store.workspace_mut(&target) {
                destination.projects.push(project);
            }
            store.changed(cx);
        });
        crate::window::open(&target, cx);
    };
    match target {
        Some(target) => move_to(target, cx),
        None => prompt(view, "new workspace", "", window, cx, move |name, _, _, cx| {
            if name.is_empty() {
                return;
            }
            let id = store::store(cx).update(cx, |store, cx| {
                let workspace = Workspace::new(&name);
                let id = workspace.id.clone();
                store.workspaces.push(workspace);
                store.changed(cx);
                id
            });
            move_to(id, cx);
        }),
    }
}

pub fn workspace_menu(view: &WorkspaceView, cx: &App) -> Vec<Item> {
    let mut items = vec![Item::Header("workspaces".into())];
    for workspace in view.store(cx).workspaces.iter() {
        let id = workspace.id.clone();
        let label = if workspace.id == view.workspace {
            format!("● {}", workspace.name)
        } else if workspace.closed {
            format!("  {} (closed)", workspace.name)
        } else {
            format!("  {}", workspace.name)
        };
        items.push(menu::action(label, move |_, _, cx| {
            let id = id.clone();
            cx.defer(move |cx| {
                crate::window::open(&id, cx);
            });
        }));
    }
    items.push(Item::Separator);
    items.push(menu::command("Open Project…", "open_project", cx));
    items.push(menu::command("New Workspace…", "new_workspace", cx));
    items.push(menu::command("Rename Workspace…", "rename_workspace", cx));
    items.push(menu::command("Delete Workspace…", "delete_workspace", cx));
    items
}

pub fn add_menu(cx: &App) -> Vec<Item> {
    vec![
        menu::command("New Tab", "new_tab", cx),
        menu::command("New Tab…", "new_tab_picker", cx),
        menu::command("New Tab Without Project…", "new_tab_without_project", cx),
        menu::command("New Terminal", "new_terminal", cx),
        menu::command("Open Project…", "open_project", cx),
    ]
}

pub fn project_menu(view: &WorkspaceView, path: &Path, cx: &App) -> Vec<Item> {
    let project = view.state(cx).and_then(|state| state.project(path)).cloned().unwrap_or_default();
    let target = path.to_path_buf();
    let mut items = Vec::new();
    {
        let target = target.clone();
        items.push(menu::action("New Terminal", move |view, window, cx| new_terminal_in(view, &target, window, cx)));
    }
    {
        let target = target.clone();
        items.push(menu::action("New Tab…", move |view, window, cx| picker(view, &target, window, cx)));
    }
    if project.is_scratch() {
        return items;
    }
    items.push(Item::Separator);
    {
        let target = target.clone();
        items.push(menu::action("Rename…", move |view, window, cx| {
            begin_rename(view, Rename::Project(target.clone()), window, cx)
        }));
    }
    {
        let target = target.clone();
        items.push(Item::Colors {
            current: project.color,
            run: Rc::new(move |color, view, _, cx| {
                let workspace = view.workspace.clone();
                let target = target.clone();
                store::store(cx).update(cx, |store, cx| {
                    if let Some(project) = store.workspace_mut(&workspace).and_then(|state| state.project_mut(&target))
                    {
                        project.color = color;
                    }
                    store.changed(cx);
                });
            }),
        });
    }
    items.push(Item::Header("move to workspace".into()));
    for workspace in view.store(cx).workspaces.iter().filter(|workspace| workspace.id != view.workspace) {
        let (id, target) = (workspace.id.clone(), target.clone());
        items.push(menu::action(format!("  {}", workspace.name), move |view, window, cx| {
            move_project_to_workspace(view, &target, Some(id.clone()), window, cx)
        }));
    }
    {
        let target = target.clone();
        items.push(menu::action("  New Workspace…", move |view, window, cx| {
            move_project_to_workspace(view, &target, None, window, cx)
        }));
    }
    items.push(Item::Separator);
    {
        let target = target.clone();
        items.push(menu::action(reveal_label(), move |_, _, cx| cx.reveal_path(&target)));
    }
    items.push(menu::action("Close Project", move |view, window, cx| close_project(view, &target, window, cx)));
    items
}

pub fn tab_menu(view: &WorkspaceView, id: &str, cx: &App) -> Vec<Item> {
    let color = view.store(cx).session(id).and_then(|session| session.read(cx).color);
    let grouped = view.state(cx).is_some_and(|state| crate::splits::group(&state.layouts, id).is_some());
    let mut items = Vec::new();
    {
        let id = id.to_string();
        items.push(menu::action("Rename…", move |view, window, cx| {
            begin_rename(view, Rename::Tab(id.clone()), window, cx)
        }));
    }
    {
        let id = id.to_string();
        items.push(Item::Colors {
            current: color,
            run: Rc::new(move |color, view, _, cx| {
                if let Some(session) = view.store(cx).session(&id).cloned() {
                    session.update(cx, |session, cx| {
                        session.color = color;
                        cx.notify();
                    });
                }
                store::store(cx).update(cx, |store, cx| store.changed(cx));
            }),
        });
    }
    if grouped {
        items.push(Item::Separator);
        let (detach, unsplit) = (id.to_string(), id.to_string());
        items.push(menu::action("Remove from Split", move |view, window, cx| {
            crate::splits_view::detach(view, &detach, window, cx)
        }));
        items.push(menu::action("Unsplit All", move |view, window, cx| {
            crate::splits_view::unsplit(view, &unsplit, window, cx)
        }));
    }
    items.push(Item::Separator);
    let id = id.to_string();
    items.push(menu::action("Close Tab", move |view, window, cx| close_tab(view, &id, window, cx)));
    items
}

pub fn drop_folders(
    view: &mut WorkspaceView,
    paths: &ExternalPaths,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) {
    for path in paths.paths().iter().filter(|path| path.is_dir()) {
        open_folder(view, path.clone(), window, cx);
    }
}

pub fn request_quit(cx: &mut App) {
    let handles: Vec<_> = store::store(cx).read(cx).windows.values().copied().collect();
    let dirty: Vec<_> = handles
        .into_iter()
        .filter(|handle| handle.read(cx).is_ok_and(|view| crate::file_view::any_dirty(view, cx)))
        .collect();
    let Some(first) = dirty.first().copied() else {
        cx.quit();
        return;
    };
    let answer = first.update(cx, |_, window, cx| {
        window.prompt(
            PromptLevel::Warning,
            "Save changes before quitting?",
            Some("Some files have unsaved changes."),
            &["Save All", "Don't Save", "Cancel"],
            cx,
        )
    });
    let Ok(answer) = answer else { return };
    cx.spawn(async move |cx| {
        let Ok(choice) = answer.await else { return };
        if choice == 2 {
            return;
        }
        let _ = cx.update(|cx| {
            if choice == 0 {
                for handle in dirty {
                    let _ = handle.update(cx, |view, _, cx| crate::file_view::save_all(view, cx));
                }
            }
            cx.quit();
        });
    })
    .detach();
}

pub fn close_workspace_window(id: &str, window: &mut Window, cx: &mut App) {
    let store = store::store(cx);
    let running = store.read(cx).workspace(id).is_some_and(|state| {
        state
            .all_projects()
            .flat_map(|project| project.tabs.iter().chain(&project.terminals))
            .any(|tab| store.read(cx).session(tab).is_some_and(|session| session.read(cx).is_running()))
    });
    let view = window.root::<WorkspaceView>().flatten();
    let dirty = view.as_ref().is_some_and(|view| crate::file_view::any_dirty(view.read(cx), cx));
    let id = id.to_string();
    let finish = move |window: &mut Window, cx: &mut App| {
        store::store(cx).update(cx, |store, cx| store.close_workspace(&id, cx));
        window.remove_window();
        cx.defer(|cx| {
            if store::store(cx).read(cx).windows.is_empty() {
                cx.quit();
            }
        });
    };
    if !running && !dirty {
        finish(window, cx);
        return;
    }
    let handle = window.window_handle();
    if dirty {
        let answer = window.prompt(
            PromptLevel::Warning,
            "Save changes before closing this workspace?",
            Some("Some files have unsaved changes."),
            &["Save All", "Don't Save", "Cancel"],
            cx,
        );
        cx.spawn(async move |cx| {
            let Ok(choice) = answer.await else { return };
            if choice == 2 {
                return;
            }
            let _ = cx.update(|cx| {
                if choice == 0
                    && let Some(view) = view
                {
                    view.update(cx, |view, cx| crate::file_view::save_all(view, cx));
                }
                let _ = handle.update(cx, |_, window, cx| finish(window, cx));
            });
        })
        .detach();
        return;
    }
    let answer = window.prompt(
        PromptLevel::Info,
        "Close this workspace?",
        Some("Its tabs are saved and come back when the workspace is reopened."),
        &["Close Workspace", "Cancel"],
        cx,
    );
    cx.spawn(async move |cx| {
        if answer.await == Ok(0) {
            let _ = handle.update(cx, |_, window, cx| finish(window, cx));
        }
    })
    .detach();
}

pub fn end_theme_preview(cx: &mut App) {
    if let Ok(Some(saved)) = settings::Settings::read()
        && saved.theme != settings::get(cx).theme
    {
        cx.set_global(saved);
    }
}

fn new_workspace(view: &mut WorkspaceView, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    prompt(view, "new workspace", "", window, cx, |name, _, _, cx| {
        if name.is_empty() {
            return;
        }
        let id = store::store(cx).update(cx, |store, cx| {
            let workspace = Workspace::new(&name);
            let id = workspace.id.clone();
            store.workspaces.push(workspace);
            store.changed(cx);
            id
        });
        cx.defer(move |cx| {
            crate::window::open(&id, cx);
        });
    });
}

fn rename_workspace(view: &mut WorkspaceView, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let current = view.state(cx).map(|state| state.name.clone()).unwrap_or_default();
    prompt(view, "rename workspace", &current, window, cx, |name, view, window, cx| {
        if name.is_empty() {
            return;
        }
        let id = view.workspace.clone();
        store::store(cx).update(cx, |store, cx| {
            if let Some(state) = store.workspace_mut(&id) {
                state.name = name.clone();
            }
            store.changed(cx);
        });
        window.set_window_title(&name);
    });
}

fn delete_workspace(view: &mut WorkspaceView, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let name = view.state(cx).map(|state| state.name.clone()).unwrap_or_default();
    let answer = window.prompt(
        PromptLevel::Critical,
        &format!("Delete the workspace {name}?"),
        Some("Its projects are removed from Nucli and its tabs are stopped. Files on disk are not touched."),
        &["Delete", "Cancel"],
        cx,
    );
    let id = view.workspace.clone();
    cx.spawn_in(window, async move |_, cx| {
        if answer.await != Ok(0) {
            return;
        }
        let _ = cx.update(|window, cx| {
            store::store(cx).update(cx, |store, cx| {
                let ids: Vec<String> = store
                    .workspace(&id)
                    .map(|state| {
                        state
                            .all_projects()
                            .flat_map(|project| project.tabs.iter().chain(&project.terminals).cloned())
                            .collect()
                    })
                    .unwrap_or_default();
                for tab in ids {
                    store.close_tab(&tab, cx);
                }
                store.workspaces.retain(|state| state.id != id);
                store.windows.remove(&id);
                if store.workspaces.is_empty() {
                    store.workspaces.push(Workspace::new("Personal"));
                }
                store.changed(cx);
            });
            window.remove_window();
            cx.defer(|cx| {
                let store = store::store(cx);
                if store.read(cx).windows.is_empty() {
                    let first = store.read(cx).workspaces.first().map(|state| state.id.clone());
                    if let Some(first) = first {
                        crate::window::open(&first, cx);
                    }
                }
            });
        });
    })
    .detach();
}

fn cycle(view: &mut WorkspaceView, delta: i32, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let Some(state) = view.state(cx) else { return };
    let tabs = state.ordered_tabs();
    if tabs.is_empty() {
        return;
    }
    let current = state.selected_tab().and_then(|id| tabs.iter().position(|tab| tab == id));
    let next = match current {
        Some(index) => (index as i32 + delta).rem_euclid(tabs.len() as i32) as usize,
        None => 0,
    };
    let id = tabs[next].clone();
    view.select_tab(&id, window, cx);
}

fn select_number(view: &mut WorkspaceView, number: usize, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    if let Some(id) = view.state(cx).and_then(|state| state.ordered_tabs().get(number - 1).cloned()) {
        view.select_tab(&id, window, cx);
    }
}

fn set_ui(cx: &mut App, change: impl FnOnce(&mut store::Store)) {
    store::store(cx).update(cx, |store, cx| {
        change(store);
        store.changed(cx);
    });
}

fn set_window(view: &WorkspaceView, cx: &mut App, change: impl FnOnce(&mut crate::persist::WindowState)) {
    let id = view.workspace.clone();
    store::store(cx).update(cx, |store, cx| {
        if let Some(state) = store.workspace_mut(&id) {
            change(&mut state.window);
        }
        store.changed(cx);
    });
}

fn show_sidebar_tab(view: &mut WorkspaceView, tab: SidebarTab, cx: &mut Context<WorkspaceView>) {
    set_window(view, cx, |state| state.sidebar_hidden = false);
    set_ui(cx, |store| store.ui.sidebar_tab = tab);
    if tab == SidebarTab::History {
        store::store(cx).update(cx, |store, cx| store.refresh_history(cx));
    }
}

fn show_panel_tab(tab: PanelTab, cx: &mut App) {
    set_ui(cx, |store| {
        store.ui.panel_hidden = false;
        store.ui.panel_tab = tab;
    });
}

fn change_font(delta: f32, cx: &mut App) {
    settings::update(cx, |settings| {
        settings.font_size = if delta == 0.0 { 13.0 } else { (settings.font_size + delta).clamp(6.0, 48.0) };
    });
}

pub fn detect(cx: &mut App, agents_only: bool) {
    let progress = crate::store::begin_progress(if agents_only { "detecting agents" } else { "detecting shells" }, cx);
    cx.spawn(async move |cx| {
        let (agents, shells) =
            cx.background_executor()
                .spawn(async move {
                    (if agents_only { agents::detect_agents() } else { Vec::new() }, agents::detect_shells())
                })
                .await;
        let _ = cx.update(|cx| {
            crate::store::finish_progress(progress, cx);
            settings::update(cx, |settings| {
                for profile in agents {
                    if !settings.agents.iter().any(|existing| existing.name == profile.name) {
                        settings.agents.push(profile);
                    }
                }
                for shell in shells {
                    if !settings.shells.iter().any(|existing| existing.path == shell.path) {
                        settings.shells.push(shell);
                    }
                }
            });
        });
    })
    .detach();
}

pub fn command_palette(view: &mut WorkspaceView, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let items = crate::palettes::everything(view, cx);
    let palette = cx.new(|cx| Palette::new("palette", "search tabs, projects, commands, files…", items, 0, cx));
    view.show_palette(palette, window, cx);
}

pub fn register(root: Stateful<gpui::Div>, cx: &mut Context<WorkspaceView>) -> Stateful<gpui::Div> {
    root.on_action(cx.listener(|view, _: &NewWorkspace, window, cx| new_workspace(view, window, cx)))
        .on_action(cx.listener(|view, _: &RenameWorkspace, window, cx| rename_workspace(view, window, cx)))
        .on_action(cx.listener(|view, _: &DeleteWorkspace, window, cx| delete_workspace(view, window, cx)))
        .on_action(cx.listener(|view, _: &NewTab, window, cx| new_tab(view, window, cx)))
        .on_action(cx.listener(|view, _: &NewTabPicker, window, cx| {
            let target = current_target(view, cx);
            picker(view, &target, window, cx)
        }))
        .on_action(cx.listener(|view, _: &NewTabWithoutProject, window, cx| picker(view, Path::new(""), window, cx)))
        .on_action(cx.listener(|view, _: &NewTabInFolder, window, cx| {
            choose_folder(view, window, cx, |path, view, window, cx| {
                let workspace = view.workspace.clone();
                let (owner, path) = store::store(cx).update(cx, |store, cx| store.add_project(&workspace, path, cx));
                if owner == view.workspace {
                    picker(view, &path, window, cx);
                }
            })
        }))
        .on_action(cx.listener(|view, _: &OpenProject, window, cx| {
            choose_folder(view, window, cx, |path, view, window, cx| {
                open_folder(view, path, window, cx);
            })
        }))
        .on_action(cx.listener(|view, _: &CommandPalette, window, cx| command_palette(view, window, cx)))
        .on_action(cx.listener(|view, _: &QuickOpen, window, cx| crate::palettes::quick_open(view, window, cx)))
        .on_action(cx.listener(|view, _: &SelectTheme, window, cx| crate::palettes::themes(view, window, cx)))
        .on_action(cx.listener(|view, _: &SelectLanguage, window, cx| crate::palettes::languages(view, window, cx)))
        .on_action(cx.listener(|view, _: &Close, window, cx| close(view, window, cx)))
        .on_action(cx.listener(|view, _: &CloseProject, window, cx| {
            if let Some(path) = view.current_project_path(cx) {
                close_project(view, &path, window, cx);
            }
        }))
        .on_action(cx.listener(|view, _: &RenameTab, window, cx| {
            if let Some(id) = view.state(cx).and_then(|state| state.selected_tab()).map(String::from) {
                begin_rename(view, Rename::Tab(id), window, cx);
            }
        }))
        .on_action(cx.listener(|view, _: &NextTab, window, cx| cycle(view, 1, window, cx)))
        .on_action(cx.listener(|view, _: &PreviousTab, window, cx| cycle(view, -1, window, cx)))
        .on_action(cx.listener(|view, _: &SelectTab1, window, cx| select_number(view, 1, window, cx)))
        .on_action(cx.listener(|view, _: &SelectTab2, window, cx| select_number(view, 2, window, cx)))
        .on_action(cx.listener(|view, _: &SelectTab3, window, cx| select_number(view, 3, window, cx)))
        .on_action(cx.listener(|view, _: &SelectTab4, window, cx| select_number(view, 4, window, cx)))
        .on_action(cx.listener(|view, _: &SelectTab5, window, cx| select_number(view, 5, window, cx)))
        .on_action(cx.listener(|view, _: &SelectTab6, window, cx| select_number(view, 6, window, cx)))
        .on_action(cx.listener(|view, _: &SelectTab7, window, cx| select_number(view, 7, window, cx)))
        .on_action(cx.listener(|view, _: &SelectTab8, window, cx| select_number(view, 8, window, cx)))
        .on_action(cx.listener(|view, _: &SelectTab9, window, cx| select_number(view, 9, window, cx)))
        .on_action(cx.listener(|view, _: &ToggleSidebar, _, cx| {
            set_window(view, cx, |state| state.sidebar_hidden = !state.sidebar_hidden)
        }))
        .on_action(
            cx.listener(|_, _: &TogglePanel, _, cx| set_ui(cx, |store| store.ui.panel_hidden = !store.ui.panel_hidden)),
        )
        .on_action(cx.listener(|_, _: &MovePanel, _, cx| {
            set_ui(cx, |store| {
                store.ui.panel_side = if store.ui.panel_side == Side::Left { Side::Right } else { Side::Left }
            })
        }))
        .on_action(cx.listener(|view, _: &FilterSidebar, window, cx| {
            show_sidebar_tab(view, view.store(cx).ui.sidebar_tab, cx);
            view.filter.read(cx).focus(window);
        }))
        .on_action(cx.listener(|view, _: &ShowProjects, _, cx| show_sidebar_tab(view, SidebarTab::Projects, cx)))
        .on_action(cx.listener(|view, _: &ShowAgents, _, cx| show_sidebar_tab(view, SidebarTab::Agents, cx)))
        .on_action(cx.listener(|view, _: &ShowHistory, _, cx| show_sidebar_tab(view, SidebarTab::History, cx)))
        .on_action(cx.listener(|_, _: &ShowFiles, _, cx| show_panel_tab(PanelTab::Files, cx)))
        .on_action(cx.listener(|_, _: &ShowChanges, _, cx| show_panel_tab(PanelTab::Changes, cx)))
        .on_action(cx.listener(|_, _: &ShowCommits, _, cx| show_panel_tab(PanelTab::Commits, cx)))
        .on_action(cx.listener(|_, _: &ToggleFullScreen, window, _| window.toggle_fullscreen()))
        .on_action(cx.listener(|_, _: &Minimize, window, _| window.minimize_window()))
        .on_action(cx.listener(|_, _: &Zoom, window, _| window.zoom_window()))
        .on_action(cx.listener(|_, _: &FontBigger, _, cx| change_font(1.0, cx)))
        .on_action(cx.listener(|_, _: &FontSmaller, _, cx| change_font(-1.0, cx)))
        .on_action(cx.listener(|_, _: &FontReset, _, cx| change_font(0.0, cx)))
        .on_action(cx.listener(|_, _: &DetectAgents, _, cx| detect(cx, true)))
        .on_action(cx.listener(|_, _: &DetectShells, _, cx| detect(cx, false)))
        .on_action(cx.listener(|_, _: &ToggleNotifications, _, cx| {
            settings::update(cx, |settings| settings.notifications = !settings.notifications)
        }))
        .on_action(cx.listener(|view, _: &OpenSettings, window, cx| crate::settings_view::toggle(view, window, cx)))
        .on_action(cx.listener(|view, _: &OpenSettingsFile, window, cx| {
            crate::file_view::open_config(view, crate::paths::settings_file(), window, cx)
        }))
        .on_action(cx.listener(|view, _: &OpenKeybindings, window, cx| {
            crate::settings_view::show(view, Some(crate::persist::SettingsSection::Keybindings), window, cx)
        }))
        .on_action(cx.listener(|view, _: &OpenKeybindingsFile, window, cx| {
            crate::file_view::open_config(view, crate::paths::keybindings_file(), window, cx)
        }))
        .on_action(cx.listener(|view, _: &ToggleTerminal, window, cx| crate::panel::toggle_terminal(view, window, cx)))
        .on_action(cx.listener(|view, _: &NewTerminal, window, cx| {
            let target = current_target(view, cx);
            new_terminal_in(view, &target, window, cx)
        }))
        .on_action(cx.listener(|view, _: &SplitRight, window, cx| crate::splits_view::split(view, true, window, cx)))
        .on_action(cx.listener(|view, _: &SplitDown, window, cx| crate::splits_view::split(view, false, window, cx)))
        .on_action(cx.listener(|view, _: &CloseSplit, window, cx| {
            if let Some(id) = view.state(cx).and_then(|state| state.selected_tab()).map(String::from) {
                crate::splits_view::detach(view, &id, window, cx);
            }
        }))
        .on_action(cx.listener(|view, _: &FocusLeft, window, cx| {
            crate::splits_view::focus(view, crate::splits::Direction::Left, window, cx)
        }))
        .on_action(cx.listener(|view, _: &FocusRight, window, cx| {
            crate::splits_view::focus(view, crate::splits::Direction::Right, window, cx)
        }))
        .on_action(cx.listener(|view, _: &FocusUp, window, cx| {
            crate::splits_view::focus(view, crate::splits::Direction::Up, window, cx)
        }))
        .on_action(cx.listener(|view, _: &FocusDown, window, cx| {
            crate::splits_view::focus(view, crate::splits::Direction::Down, window, cx)
        }))
        .on_action(cx.listener(|view, _: &TabSwitcher, window, cx| crate::switcher::open(view, 1, window, cx)))
        .on_action(cx.listener(|view, _: &TabSwitcherBack, window, cx| crate::switcher::open(view, -1, window, cx)))
        .on_action(cx.listener(|_, _: &Hide, _, cx| cx.hide()))
        .on_action(cx.listener(|_, _: &HideOthers, _, cx| cx.hide_other_apps()))
        .on_action(cx.listener(|_, _: &ShowAll, _, cx| cx.unhide_other_apps()))
        .on_action(cx.listener(|_, _: &About, window, cx| {
            let detail =
                format!("Version {}\nMIT License. Maple Mono NF font under SIL OFL 1.1.", env!("CARGO_PKG_VERSION"));
            drop(window.prompt(PromptLevel::Info, crate::paths::app_name(), Some(&detail), &["OK"], cx));
        }))
}
