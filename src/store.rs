use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gpui::{App, AppContext, Context, Entity, Global, Task, WindowHandle};

use crate::agents::{AgentProfile, ShellProfile};
use crate::history::Conversation;
use crate::paths;
use crate::persist::{Layout, ProjectFile, SavedTab, Selected, StateFile, Ui, WindowState, WorkspaceFile};
use crate::session::{self, Kind, Session, SessionEvent, Spec};
use crate::settings;
use crate::theme::ItemColor;
use crate::window::WorkspaceView;

pub struct StoreHandle(pub Entity<Store>);

impl Global for StoreHandle {}

pub fn store(cx: &App) -> Entity<Store> {
    cx.global::<StoreHandle>().0.clone()
}

#[derive(Clone, Debug, Default)]
pub struct Project {
    pub path: PathBuf,
    pub name: Option<String>,
    pub color: Option<ItemColor>,
    pub collapsed: bool,
    pub tabs: Vec<String>,
    pub terminals: Vec<String>,
    pub active_tab: Option<String>,
    pub active_terminal: Option<String>,
    pub terminal_shown: bool,
    pub open_files: Vec<String>,
    pub active_file: Option<String>,
    pub file_shown: bool,
    pub recent_files: Vec<String>,
    pub last_launcher: Option<String>,
    pub saved: Vec<SavedTab>,
    pub saved_terminals: Vec<SavedTab>,
    pub expanded: BTreeSet<String>,
    pub languages: BTreeMap<String, String>,
}

impl Project {
    pub fn new(path: PathBuf) -> Project {
        Project { path, ..Default::default() }
    }

    pub fn is_scratch(&self) -> bool {
        self.path.as_os_str().is_empty()
    }

    pub fn cwd(&self) -> PathBuf {
        if self.is_scratch() { paths::home() } else { self.path.clone() }
    }

    pub fn display_name(&self) -> String {
        if self.is_scratch() {
            return "No Project".into();
        }
        self.name.clone().filter(|name| !name.is_empty()).unwrap_or_else(|| {
            self.path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| paths::display(&self.path))
        })
    }

    fn from_file(file: ProjectFile) -> Project {
        Project {
            path: file.path,
            name: file.name,
            color: file.color,
            collapsed: file.collapsed,
            active_tab: file.active_tab,
            terminal_shown: file.terminal_shown,
            open_files: file.open_files,
            active_file: file.active_file,
            file_shown: file.file_shown,
            languages: file.languages,
            recent_files: file.recent_files,
            last_launcher: file.last_launcher,
            saved: file.tabs,
            saved_terminals: file.terminals,
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub closed: bool,
    pub projects: Vec<Project>,
    pub scratch: Project,
    pub window: WindowState,
    pub selected: Option<Selected>,
    pub mru: Vec<String>,
    pub layouts: Vec<Layout>,
}

impl Workspace {
    pub fn new(name: &str) -> Workspace {
        Workspace {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            closed: false,
            projects: Vec::new(),
            scratch: Project::default(),
            window: WindowState::default(),
            selected: None,
            mru: Vec::new(),
            layouts: Vec::new(),
        }
    }

    pub fn all_projects(&self) -> impl Iterator<Item = &Project> {
        self.projects.iter().chain(std::iter::once(&self.scratch))
    }

    pub fn project(&self, path: &Path) -> Option<&Project> {
        self.all_projects().find(|project| project.path == path)
    }

    pub fn project_mut(&mut self, path: &Path) -> Option<&mut Project> {
        if path.as_os_str().is_empty() {
            return Some(&mut self.scratch);
        }
        self.projects.iter_mut().find(|project| project.path == path)
    }

    pub fn project_of(&self, tab: &str) -> Option<&Project> {
        self.all_projects().find(|project| project.tabs.iter().chain(&project.terminals).any(|id| id == tab))
    }

    pub fn project_of_mut(&mut self, tab: &str) -> Option<&mut Project> {
        if self.scratch.tabs.iter().chain(&self.scratch.terminals).any(|id| id == tab) {
            return Some(&mut self.scratch);
        }
        self.projects.iter_mut().find(|project| project.tabs.iter().chain(&project.terminals).any(|id| id == tab))
    }

    pub fn ordered_tabs(&self) -> Vec<String> {
        self.all_projects().flat_map(|project| project.tabs.iter().cloned()).collect()
    }

    pub fn current_project_mut(&mut self) -> Option<&mut Project> {
        match self.selected.clone() {
            Some(Selected::Project(path)) => self.project_mut(&path),
            Some(Selected::Tab(id)) => self.project_of_mut(&id),
            None => None,
        }
    }

    pub fn current_project(&self) -> Option<&Project> {
        match &self.selected {
            Some(Selected::Project(path)) => self.project(path),
            Some(Selected::Tab(id)) => self.project_of(id),
            None => None,
        }
    }

    pub fn selected_tab(&self) -> Option<&str> {
        match &self.selected {
            Some(Selected::Tab(id)) => Some(id),
            _ => None,
        }
    }

    pub fn file_shown(&self) -> bool {
        matches!(self.selected, Some(Selected::Project(_)))
            && self.current_project().is_some_and(|project| project.file_shown && project.active_file.is_some())
    }

    pub fn visible_tabs(&self) -> Vec<String> {
        let mut tabs: Vec<String> = match self.selected_tab() {
            Some(id) => match crate::splits::group(&self.layouts, id) {
                Some(index) => crate::splits::tabs(&self.layouts[index]),
                None => vec![id.to_string()],
            },
            None => Vec::new(),
        };
        let project = if self.selected.is_some() { self.current_project() } else { Some(&self.scratch) };
        if let Some(terminal) =
            project.filter(|project| project.terminal_shown).and_then(|project| project.active_terminal.as_ref())
        {
            tabs.push(terminal.clone());
        }
        tabs
    }

    pub fn shows(&self, id: &str) -> bool {
        self.visible_tabs().iter().any(|tab| tab == id) && !self.file_shown()
    }

    fn from_file(file: WorkspaceFile) -> Workspace {
        Workspace {
            id: file.id,
            name: file.name,
            closed: file.closed,
            projects: file.projects.into_iter().map(Project::from_file).collect(),
            scratch: Project::from_file(ProjectFile { path: PathBuf::new(), ..file.scratch }),
            window: file.window,
            selected: file.selected,
            mru: Vec::new(),
            layouts: file.layouts,
        }
    }
}

#[derive(Clone)]
pub enum Launcher {
    Agent(AgentProfile),
    Shell(ShellProfile),
    Resume(Conversation),
    Terminal,
}

pub struct Store {
    pub workspaces: Vec<Workspace>,
    pub sessions: HashMap<String, Entity<Session>>,
    pub windows: HashMap<String, WindowHandle<WorkspaceView>>,
    pub titles: HashMap<String, String>,
    pub last_launcher: Option<String>,
    pub last_shell: Option<String>,
    pub last_folder: Option<PathBuf>,
    pub last_workspace: Option<String>,
    pub ui: Ui,
    pub open_with: BTreeMap<String, Vec<String>>,
    pub skip_close_confirm_until: u64,
    pub conversations: HashMap<PathBuf, Vec<Conversation>>,
    pub history: Option<Vec<Conversation>>,
    conversations_loading: std::collections::HashSet<PathBuf>,
    pub tick: usize,
    pub tick_fast: usize,
    pub progress: Vec<Progress>,
    next_progress: u64,
    pub diffstats: HashMap<PathBuf, crate::git::DiffStat>,
    pub revisions: HashMap<PathBuf, u64>,
    pub watchers: crate::watcher::Watchers,
    save_task: Option<Task<()>>,
    pub(crate) poll_task: Option<Task<()>>,
}

impl Store {
    pub fn load() -> Store {
        let state = StateFile::read().unwrap_or_default();
        let mut workspaces: Vec<Workspace> = state.workspaces.into_iter().map(Workspace::from_file).collect();
        if workspaces.is_empty() {
            workspaces.push(Workspace::new("Personal"));
        }
        Store {
            workspaces,
            sessions: HashMap::new(),
            windows: HashMap::new(),
            titles: HashMap::new(),
            last_launcher: state.last_launcher,
            last_shell: state.last_shell,
            last_folder: state.last_folder,
            last_workspace: state.last_workspace,
            ui: state.ui,
            open_with: state.open_with,
            skip_close_confirm_until: state.skip_close_confirm_until,
            conversations: HashMap::new(),
            history: None,
            conversations_loading: Default::default(),
            tick: 0,
            tick_fast: 0,
            progress: Vec::new(),
            next_progress: 0,
            diffstats: HashMap::new(),
            revisions: HashMap::new(),
            watchers: Default::default(),
            save_task: None,
            poll_task: None,
        }
    }

    pub fn workspace(&self, id: &str) -> Option<&Workspace> {
        self.workspaces.iter().find(|workspace| workspace.id == id)
    }

    pub fn workspace_mut(&mut self, id: &str) -> Option<&mut Workspace> {
        self.workspaces.iter_mut().find(|workspace| workspace.id == id)
    }

    pub fn workspace_of(&self, tab: &str) -> Option<&Workspace> {
        self.workspaces.iter().find(|workspace| workspace.project_of(tab).is_some())
    }

    pub fn workspace_of_project(&self, path: &Path) -> Option<&Workspace> {
        self.workspaces.iter().find(|workspace| workspace.projects.iter().any(|project| project.path == path))
    }

    pub fn session(&self, id: &str) -> Option<&Entity<Session>> {
        self.sessions.get(id)
    }

    pub fn changed(&mut self, cx: &mut Context<Self>) {
        cx.notify();
        self.schedule_save(cx);
    }

    pub fn schedule_save(&mut self, cx: &mut Context<Self>) {
        self.save_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(600)).await;
            let _ = this.update(cx, |store, cx| store.snapshot(false, cx).write());
        }));
    }

    pub fn add_project(&mut self, workspace: &str, path: PathBuf, cx: &mut Context<Self>) -> (String, PathBuf) {
        if let Some(owner) = self.workspace_of_project(&path) {
            return (owner.id.clone(), path);
        }
        if let Some(target) = self.workspace_mut(workspace) {
            target.projects.push(Project::new(path.clone()));
        }
        self.last_folder = Some(path.clone());
        self.changed(cx);
        (workspace.to_string(), path)
    }

    pub fn launcher_name(launcher: &Launcher) -> Option<String> {
        match launcher {
            Launcher::Agent(profile) => Some(profile.name.clone()),
            Launcher::Shell(shell) => Some(shell.name.clone()),
            Launcher::Resume(conversation) => Some(conversation.profile.clone()),
            Launcher::Terminal => None,
        }
    }

    pub fn terminal_shell(&self, cx: &App) -> String {
        let shells = &settings::get(cx).shells;
        self.last_shell
            .clone()
            .filter(|path| shells.iter().any(|shell| &shell.path == path) || Path::new(path).is_file())
            .or_else(|| shells.iter().find(|shell| shell.is_launchable()).map(|shell| shell.path.clone()))
            .unwrap_or_else(session::default_shell)
    }

    pub fn open_tab(
        &mut self,
        workspace: &str,
        project: &Path,
        launcher: Launcher,
        after: Option<String>,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let cwd = self.workspace(workspace)?.project(project)?.cwd();
        let profiles = settings::get(cx).agents.clone();
        let (spec, panel) = match &launcher {
            Launcher::Agent(profile) => {
                let conversation = profile
                    .kind()
                    .filter(|kind| kind.assigns_conversation_id())
                    .map(|_| uuid::Uuid::new_v4().to_string());
                (spec(Kind::Agent, Some(profile.clone()), None, conversation, false, cwd), false)
            }
            Launcher::Resume(conversation) => {
                let profile = profiles.iter().find(|profile| profile.name == conversation.profile).cloned()?;
                (spec(Kind::Agent, Some(profile), None, Some(conversation.id.clone()), true, cwd), false)
            }
            Launcher::Shell(shell) => {
                self.last_shell = Some(shell.path.clone());
                (spec(Kind::Shell, None, Some(shell.path.clone()), None, false, cwd), false)
            }
            Launcher::Terminal => (spec(Kind::Terminal, None, Some(self.terminal_shell(cx)), None, false, cwd), true),
        };
        let id = self.spawn(spec, cx)?;
        let name = Self::launcher_name(&launcher);
        if name.is_some() {
            self.last_launcher = name.clone();
        }
        let target = self.workspace_mut(workspace)?.project_mut(project)?;
        if panel {
            target.terminals.push(id.clone());
            target.active_terminal = Some(id.clone());
            target.terminal_shown = true;
        } else {
            let position =
                after.and_then(|after| target.tabs.iter().position(|tab| *tab == after)).map(|index| index + 1);
            target.tabs.insert(position.unwrap_or(target.tabs.len()), id.clone());
            if name.is_some() {
                target.last_launcher = name;
            }
        }
        self.changed(cx);
        Some(id)
    }

    pub fn spawn(&mut self, spec: Spec, cx: &mut Context<Self>) -> Option<String> {
        let size = session::default_size(cx);
        let entity = match Session::spawn(spec, size, cx) {
            Ok(entity) => entity,
            Err(error) => {
                eprintln!("spawn failed: {error:?}");
                return None;
            }
        };
        let id = entity.read(cx).id.clone();
        cx.subscribe(&entity, |store, entity, event, cx| store.on_session_event(entity, event, cx)).detach();
        self.sessions.insert(id.clone(), entity);
        Some(id)
    }

    fn on_session_event(&mut self, entity: Entity<Session>, event: &SessionEvent, cx: &mut Context<Self>) {
        let id = entity.read(cx).id.clone();
        match event {
            SessionEvent::Changed => cx.notify(),
            SessionEvent::Exited(Some(0)) => self.remove_tab(&id, cx),
            SessionEvent::Exited(_) => {
                self.mark_attention(&id, None, cx);
                cx.notify();
            }
            SessionEvent::Attention(text) => self.mark_attention(&id, Some(text.clone()), cx),
            SessionEvent::OpenPath(path, line) => {
                if let Some(workspace) = self.workspace_of(&id).map(|workspace| workspace.id.clone())
                    && let Some(window) = self.windows.get(&workspace).copied()
                {
                    let (path, line) = (path.clone(), *line);
                    cx.defer(move |cx| {
                        let _ = window.update(cx, |view, window, cx| view.open_path(&path, line, window, cx));
                    });
                }
            }
        }
    }

    pub fn select(&mut self, workspace: &str, selected: Option<Selected>, cx: &mut Context<Self>) {
        if let Some(Selected::Tab(id)) = &selected
            && let Some(session) = self.sessions.get(id)
        {
            session.update(cx, |session, cx| {
                if session.attention {
                    session.attention = false;
                    cx.notify();
                }
            });
        }
        let Some(target) = self.workspace_mut(workspace) else { return };
        if let Some(Selected::Tab(id)) = &selected {
            target.mru.retain(|tab| tab != id);
            target.mru.insert(0, id.clone());
            if let Some(project) = target.project_of_mut(id) {
                if project.tabs.contains(id) {
                    project.active_tab = Some(id.clone());
                }
                project.file_shown = false;
            }
        }
        target.selected = selected;
        self.changed(cx);
    }

    pub fn close_tab(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(session) = self.sessions.get(id) {
            session.update(cx, |session, _| session.kill());
        }
        self.remove_tab(id, cx);
    }

    pub fn remove_tab(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(workspace) = self.workspace_of(id).map(|workspace| workspace.id.clone()) else {
            self.sessions.remove(id);
            return;
        };
        let target = self.workspace_mut(&workspace).expect("workspace exists");
        let was_selected = target.selected == Some(Selected::Tab(id.to_string()));
        let project = target.project_of_mut(id).expect("project exists");
        let project_path = project.path.clone();
        let mut next = None;
        if let Some(index) = project.tabs.iter().position(|tab| tab == id) {
            project.tabs.remove(index);
            next = project.tabs.get(index).or_else(|| project.tabs.get(index.wrapping_sub(1))).cloned();
            if project.active_tab.as_deref() == Some(id) {
                project.active_tab = next.clone();
            }
        }
        if let Some(index) = project.terminals.iter().position(|tab| tab == id) {
            project.terminals.remove(index);
            if project.active_terminal.as_deref() == Some(id) {
                project.active_terminal =
                    project.terminals.get(index).or_else(|| project.terminals.get(index.wrapping_sub(1))).cloned();
            }
            if project.terminals.is_empty() {
                project.terminal_shown = false;
            }
        }
        target.mru.retain(|tab| tab != id);
        crate::splits::remove(&mut target.layouts, id);
        if was_selected {
            target.selected = Some(match next {
                Some(next) => Selected::Tab(next),
                None => Selected::Project(project_path),
            });
            if matches!(&target.selected, Some(Selected::Project(path)) if path.as_os_str().is_empty()) {
                target.selected = None;
            }
        }
        if let Some(entity) = self.sessions.remove(id) {
            entity.update(cx, |session, _| session.kill());
        }
        self.changed(cx);
    }

    pub fn snapshot(&self, with_output: bool, cx: &App) -> StateFile {
        if with_output {
            let _ = std::fs::create_dir_all(paths::scrollback_dir());
        }
        let restore = settings::get(cx).restore_tabs;
        let taken: std::collections::HashSet<String> =
            self.sessions.values().filter_map(|session| session.read(cx).conversation.clone()).collect();
        let saved = |ids: &[String], fallback: &[SavedTab]| -> Vec<SavedTab> {
            if ids.is_empty() && !fallback.is_empty() {
                return fallback.to_vec();
            }
            if !restore {
                return Vec::new();
            }
            ids.iter()
                .filter_map(|id| self.sessions.get(id))
                .map(|entity| {
                    let session = entity.read(cx);
                    if with_output && session.kind != Kind::Agent {
                        let text = session.terminal.text(5000);
                        if !text.trim().is_empty() {
                            let _ = std::fs::write(crate::persist::scrollback_file(&session.id), text);
                        }
                    }
                    let mut tab = session.saved_tab();
                    if with_output
                        && session.agent == Some(crate::agents::AgentKind::Codex)
                        && tab.conversation.is_none()
                    {
                        tab.conversation = codex_conversation(session, &taken, cx);
                    }
                    tab
                })
                .collect()
        };
        let project_file = |project: &Project| ProjectFile {
            path: project.path.clone(),
            name: project.name.clone(),
            color: project.color,
            collapsed: project.collapsed,
            tabs: saved(&project.tabs, &project.saved),
            terminals: saved(&project.terminals, &project.saved_terminals),
            active_tab: project.active_tab.clone(),
            terminal_shown: project.terminal_shown,
            open_files: project.open_files.clone(),
            active_file: project.active_file.clone(),
            file_shown: project.file_shown,
            languages: project.languages.clone(),
            recent_files: project.recent_files.clone(),
            last_launcher: project.last_launcher.clone(),
        };
        let state = StateFile {
            workspaces: self
                .workspaces
                .iter()
                .map(|workspace| WorkspaceFile {
                    id: workspace.id.clone(),
                    name: workspace.name.clone(),
                    closed: workspace.closed,
                    window: workspace.window.clone(),
                    selected: workspace.selected.clone(),
                    projects: workspace.projects.iter().map(project_file).collect(),
                    scratch: project_file(&workspace.scratch),
                    layouts: workspace.layouts.clone(),
                })
                .collect(),
            last_launcher: self.last_launcher.clone(),
            last_shell: self.last_shell.clone(),
            last_folder: self.last_folder.clone(),
            last_workspace: self.last_workspace.clone(),
            ui: self.ui.clone(),
            open_with: self.open_with.clone(),
            skip_close_confirm_until: self.skip_close_confirm_until,
        };
        if with_output {
            crate::persist::prune_scrollback(&state);
        }
        state
    }

    pub fn restore(&mut self, workspace: &str, cx: &mut Context<Self>) {
        let Some(target) = self.workspace(workspace) else { return };
        let mut jobs: Vec<(PathBuf, SavedTab, bool)> = Vec::new();
        for project in target.all_projects() {
            for tab in &project.saved {
                jobs.push((project.path.clone(), tab.clone(), false));
            }
            for tab in &project.saved_terminals {
                jobs.push((project.path.clone(), tab.clone(), true));
            }
        }
        let profiles = settings::get(cx).agents.clone();
        for (path, tab, panel) in jobs {
            let cwd = match self.workspace(workspace).and_then(|workspace| workspace.project(&path)) {
                Some(project) => project.cwd(),
                None => continue,
            };
            let kind = tab.kind.unwrap_or(if panel { Kind::Terminal } else { Kind::Shell });
            let mut spec = match kind {
                Kind::Agent => {
                    let Some(profile) = tab.profile.as_ref().and_then(|name| profiles.iter().find(|p| &p.name == name))
                    else {
                        continue;
                    };
                    let (conversation, resume) = resume_target(profile, &cwd, tab.conversation.clone());
                    spec(Kind::Agent, Some(profile.clone()), None, conversation, resume, cwd)
                }
                _ => {
                    let folder = tab.cwd.clone().filter(|dir| dir.is_dir()).unwrap_or(cwd);
                    let mut spec = spec(kind, None, tab.shell.clone(), None, false, folder);
                    spec.restored = crate::persist::read_scrollback(&tab.id);
                    spec
                }
            };
            spec.id = Some(tab.id.clone());
            let Some(id) = self.spawn(spec, cx) else { continue };
            if let Some(entity) = self.sessions.get(&id) {
                entity.update(cx, |session, _| {
                    session.name = tab.name.clone();
                    session.color = tab.color;
                });
            }
            if let Some(project) = self.workspace_mut(workspace).and_then(|workspace| workspace.project_mut(&path)) {
                if panel { project.terminals.push(id) } else { project.tabs.push(id) }
            }
        }
        if let Some(target) = self.workspace_mut(workspace) {
            for project in target.projects.iter_mut().chain(std::iter::once(&mut target.scratch)) {
                project.saved.clear();
                project.saved_terminals.clear();
                if project.active_terminal.is_none() {
                    project.active_terminal = project.terminals.first().cloned();
                }
            }
            let valid: std::collections::HashSet<String> = target
                .all_projects()
                .flat_map(|project| project.tabs.iter().chain(&project.terminals).cloned())
                .collect();
            if matches!(&target.selected, Some(Selected::Tab(id)) if !valid.contains(id)) {
                target.selected = None;
            }
            target.layouts.retain(|layout| crate::splits::tabs(layout).iter().all(|tab| valid.contains(tab)));
        }
        cx.notify();
    }

    pub fn close_workspace(&mut self, workspace: &str, cx: &mut Context<Self>) {
        let state = self.snapshot(true, cx);
        let Some(file) = state.workspaces.into_iter().find(|file| file.id == workspace) else { return };
        let ids: Vec<String> = self
            .workspace(workspace)
            .map(|workspace| {
                workspace
                    .all_projects()
                    .flat_map(|project| project.tabs.iter().chain(&project.terminals).cloned())
                    .collect()
            })
            .unwrap_or_default();
        for id in ids {
            if let Some(entity) = self.sessions.remove(&id) {
                entity.update(cx, |session, _| session.kill());
            }
        }
        if let Some(target) = self.workspace_mut(workspace) {
            let mut restored = Workspace::from_file(file);
            restored.closed = true;
            *target = restored;
        }
        self.windows.remove(workspace);
        self.changed(cx);
    }

    pub fn conversations_for(&self, path: &Path) -> Option<&Vec<Conversation>> {
        self.conversations.get(path)
    }

    pub fn begin(&mut self, text: impl Into<String>) -> u64 {
        self.next_progress += 1;
        self.progress.push(Progress { id: self.next_progress, text: text.into(), started: Instant::now() });
        self.next_progress
    }

    pub fn finish(&mut self, id: u64, cx: &mut Context<Self>) {
        let shown = self.progress.iter().any(|item| item.id == id && item.started.elapsed() >= PROGRESS_DELAY);
        self.progress.retain(|item| item.id != id);
        if shown {
            cx.notify();
        }
    }

    pub fn refresh_conversations(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if !self.conversations_loading.insert(path.clone()) {
            return;
        }
        let sources = crate::history::sources(&settings::get(cx).agents);
        let progress = self.begin("loading conversations");
        cx.spawn(async move |this, cx| {
            let lookup = if path.as_os_str().is_empty() { paths::home() } else { path.clone() };
            let list = cx.background_executor().spawn(async move { crate::history::list(&lookup, &sources, 30) }).await;
            let _ = this.update(cx, |store, cx| {
                store.finish(progress, cx);
                store.conversations_loading.remove(&path);
                if store.conversations.get(&path) != Some(&list) {
                    store.conversations.insert(path, list);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub fn refresh_history(&mut self, cx: &mut Context<Self>) {
        let sources = crate::history::sources(&settings::get(cx).agents);
        let progress = self.begin("loading history");
        cx.spawn(async move |this, cx| {
            let list = cx.background_executor().spawn(async move { crate::history::all(&sources, 150) }).await;
            let _ = this.update(cx, |store, cx| {
                store.finish(progress, cx);
                store.history = Some(list);
                cx.notify();
            });
        })
        .detach();
    }
}

pub const PROGRESS_DELAY: Duration = Duration::from_millis(200);

pub struct Progress {
    pub id: u64,
    pub text: String,
    pub started: Instant,
}

pub fn begin_progress(text: impl Into<String>, cx: &mut App) -> u64 {
    store(cx).update(cx, |store, _| store.begin(text))
}

pub fn finish_progress(id: u64, cx: &mut App) {
    store(cx).update(cx, |store, cx| store.finish(id, cx));
}

pub fn spec(
    kind: Kind,
    profile: Option<AgentProfile>,
    shell: Option<String>,
    conversation: Option<String>,
    resume: bool,
    cwd: PathBuf,
) -> Spec {
    Spec { id: None, kind, profile, shell, conversation, resume, cwd, restored: None }
}

fn codex_conversation(session: &Session, taken: &std::collections::HashSet<String>, cx: &App) -> Option<String> {
    let name = session.profile.as_ref()?;
    let profile = settings::get(cx).agents.iter().find(|profile| &profile.name == name)?;
    crate::history::newest_codex_conversation(&profile.data_dir()?, &session.project, session.started, taken)
}

fn resume_target(profile: &AgentProfile, cwd: &Path, conversation: Option<String>) -> (Option<String>, bool) {
    use crate::agents::AgentKind;
    match (profile.kind(), conversation) {
        (Some(AgentKind::Claude), Some(id)) => {
            let exists =
                profile.data_dir().is_some_and(|dir| crate::history::claude_conversation_exists(&dir, cwd, &id));
            if exists { (Some(id), true) } else { (Some(uuid::Uuid::new_v4().to_string()), false) }
        }
        (Some(_), Some(id)) => (Some(id), true),
        (Some(kind), None) if kind.assigns_conversation_id() => (Some(uuid::Uuid::new_v4().to_string()), false),
        _ => (None, false),
    }
}

pub fn init(cx: &mut App) -> Entity<Store> {
    let entity = cx.new(|_| Store::load());
    cx.set_global(StoreHandle(entity.clone()));
    entity
}
