use std::path::{Path, PathBuf};
use std::time::SystemTime;

use futures::StreamExt;
use gpui::{App, AppContext, ClipboardItem, Context, Entity, EventEmitter, FocusHandle, Pixels, Point, Size, Task};

use crate::agents::{self, AgentKind, AgentProfile};
use crate::settings;
use crate::terminal::{self, Event, GridSize, Launch, Terminal};
use crate::theme::ItemColor;
use crate::ui;

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Agent,
    Shell,
    Terminal,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Activity {
    #[default]
    Idle,
    Working,
    Waiting,
}

pub enum SessionEvent {
    Changed,
    Exited(Option<u32>),
    Attention(String),
    OpenPath(PathBuf, Option<usize>),
}

pub struct Spec {
    pub id: Option<String>,
    pub kind: Kind,
    pub profile: Option<AgentProfile>,
    pub shell: Option<String>,
    pub conversation: Option<String>,
    pub resume: bool,
    pub cwd: PathBuf,
    pub restored: Option<String>,
}

pub struct Session {
    pub id: String,
    pub kind: Kind,
    pub profile: Option<String>,
    pub agent: Option<AgentKind>,
    pub shell: Option<String>,
    pub name: Option<String>,
    pub title: Option<String>,
    pub color: Option<ItemColor>,
    pub conversation: Option<String>,
    pub project: PathBuf,
    pub terminal: Terminal,
    pub focus: FocusHandle,
    pub exit: Option<Option<u32>>,
    pub activity: Activity,
    pub attention: bool,
    pub cwd: Option<PathBuf>,
    pub foreground: Option<String>,
    pub running_agent: Option<String>,
    pub branch: Option<String>,
    pub subagents: Vec<crate::claude::Subagent>,
    pub started: SystemTime,
    pub view: terminal::view::ViewState,
    _events: Task<()>,
}

impl EventEmitter<SessionEvent> for Session {}

pub fn default_size(cx: &App) -> GridSize {
    let size = settings::get(cx).font_size();
    GridSize { cols: 100, rows: 30, cell_width: size * 0.6, cell_height: size * settings::get(cx).line_height() }
}

impl Session {
    pub fn spawn(spec: Spec, size: GridSize, cx: &mut App) -> anyhow::Result<Entity<Session>> {
        let (program, args, profile_env) = command(&spec);
        let launch = environment(program, args, profile_env, spec.cwd.clone());
        let settings = settings::get(cx);
        let cursor = terminal::cursor_style(settings.cursor, settings.cursor_blink);
        let (mut terminal, mut events) = Terminal::spawn(launch, size, cursor)?;
        terminal.anchor.store(spec.kind != Kind::Agent, std::sync::atomic::Ordering::Release);
        if let Some(text) = spec.restored.as_deref().filter(|text| !text.trim().is_empty()) {
            let body = text.replace('\n', "\r\n");
            let mark = terminal::RESTORED_MARK;
            terminal.feed(format!("\x1b[2m{body}\x1b[0m\r\n\x1b[2m{mark}\x1b[0m\r\n").as_bytes());
        }
        let agent = spec.profile.as_ref().and_then(AgentProfile::kind);
        Ok(cx.new(|cx| {
            let task = cx.spawn(async move |this: gpui::WeakEntity<Session>, cx: &mut gpui::AsyncApp| {
                while let Some(event) = events.next().await {
                    if this.update(cx, |session, cx| session.handle(event, cx)).is_err() {
                        break;
                    }
                }
            });
            cx.spawn(async move |this: gpui::WeakEntity<Session>, cx: &mut gpui::AsyncApp| {
                loop {
                    cx.background_executor().timer(std::time::Duration::from_millis(530)).await;
                    let alive = this.update(cx, |session, cx| {
                        if session.view.focused && settings::get(cx).cursor_blink && session.exit.is_none() {
                            session.view.cursor_hidden = !session.view.cursor_hidden;
                            cx.notify();
                        }
                    });
                    if alive.is_err() {
                        break;
                    }
                }
            })
            .detach();
            let mut session = Session {
                id: spec.id.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                kind: spec.kind,
                profile: spec.profile.as_ref().map(|profile| profile.name.clone()),
                agent,
                shell: spec.shell.clone(),
                name: None,
                title: None,
                color: None,
                conversation: spec.conversation.clone(),
                project: spec.cwd.clone(),
                terminal,
                focus: cx.focus_handle(),
                exit: None,
                activity: Activity::Idle,
                attention: false,
                cwd: None,
                foreground: None,
                running_agent: None,
                branch: None,
                subagents: Vec::new(),
                started: SystemTime::now(),
                view: Default::default(),
                _events: task,
            };
            session.apply_theme(cx);
            session
        }))
    }

    fn handle(&mut self, event: Event, cx: &mut Context<Self>) {
        match event {
            Event::Wakeup => {
                self.terminal.take_wakeup();
                self.refresh_search(cx);
                cx.notify();
            }
            Event::Title(title) => {
                self.title = title.filter(|title| !title.trim().is_empty());
                cx.emit(SessionEvent::Changed);
                cx.notify();
            }
            Event::Bell => cx.emit(SessionEvent::Attention("Bell".into())),
            Event::Notify(text) => cx.emit(SessionEvent::Attention(text)),
            Event::Clipboard(text) => cx.write_to_clipboard(ClipboardItem::new_string(text)),
            Event::Exit(code) => {
                self.exit = Some(code);
                self.activity = Activity::Idle;
                if code != Some(0) {
                    let text = match code {
                        Some(code) => format!("\r\n\x1b[2m[process exited with code {code}]\x1b[0m\r\n"),
                        None => "\r\n\x1b[2m[process exited]\x1b[0m\r\n".to_string(),
                    };
                    self.terminal.feed(text.as_bytes());
                }
                cx.emit(SessionEvent::Exited(code));
                cx.notify();
            }
        }
    }

    pub fn display_title(&self) -> String {
        if let Some(name) = self.name.as_ref().filter(|name| !name.is_empty()) {
            return name.clone();
        }
        let base = match self.kind {
            Kind::Agent => self.profile.clone().unwrap_or_else(|| "agent".into()),
            _ => self.shell.as_deref().map(agents::command_name).unwrap_or_else(|| "shell".into()),
        };
        match self.title.as_deref() {
            Some(title) if self.kind != Kind::Agent => title.to_string(),
            Some(title) if !title.is_empty() && title != base => format!("{base} · {title}"),
            _ => base,
        }
    }

    pub fn is_running(&self) -> bool {
        self.exit.is_none()
    }

    pub fn apply_theme(&mut self, cx: &App) {
        let own = self.profile_theme(cx).map(|id| std::rc::Rc::new(crate::theme::resolve(&id, ui::system_dark(cx))));
        let theme = own.clone().unwrap_or_else(|| ui::theme(cx));
        self.terminal.set_colors(terminal::view::color_table(&theme));
        self.view.theme = own;
        let settings = settings::get(cx);
        self.terminal.set_cursor(terminal::cursor_style(settings.cursor, settings.cursor_blink));
    }

    pub fn profile_theme(&self, cx: &App) -> Option<String> {
        let name = self.profile.as_ref()?;
        settings::get(cx)
            .agents
            .iter()
            .find(|profile| &profile.name == name)
            .map(|profile| profile.theme.clone())
            .filter(|theme| !theme.is_empty())
    }

    pub fn saved_tab(&self) -> crate::persist::SavedTab {
        crate::persist::SavedTab {
            id: self.id.clone(),
            kind: Some(self.kind),
            profile: self.profile.clone(),
            shell: self.shell.clone(),
            name: self.name.clone(),
            color: self.color,
            conversation: self.conversation.clone(),
            cwd: self.cwd.clone(),
        }
    }

    pub fn kill(&mut self) {
        if self.exit.is_none() {
            self.terminal.kill();
        }
    }

    pub fn cell_at(&self, position: Point<Pixels>) -> Option<(usize, usize)> {
        let (origin, cell) = self.view.layout?;
        let x = f32::from(position.x - origin.x) / f32::from(cell.width);
        let y = f32::from(position.y - origin.y) / f32::from(cell.height);
        if x < 0.0 || y < 0.0 {
            return None;
        }
        Some((x as usize, y as usize))
    }

    pub fn cell_size(&self) -> Option<Size<Pixels>> {
        self.view.layout.map(|(_, cell)| cell)
    }

    pub fn working_dir(&self) -> &Path {
        self.cwd.as_deref().unwrap_or(&self.project)
    }
}

fn command(spec: &Spec) -> (String, Vec<String>, Vec<(String, String)>) {
    let extra: Vec<String> = match (&spec.profile, &spec.conversation) {
        (Some(profile), Some(id)) => match profile.kind() {
            Some(kind) if spec.resume => kind.resume_arguments(id),
            Some(kind) => kind.new_arguments(id),
            None => Vec::new(),
        },
        _ => Vec::new(),
    };
    match (&spec.profile, spec.kind) {
        (Some(profile), Kind::Agent) => {
            if cfg!(windows) {
                let env = profile
                    .environment()
                    .into_iter()
                    .map(|(key, value)| (key, crate::paths::expand(&value).to_string_lossy().into_owned()))
                    .collect();
                ("cmd.exe".into(), vec!["/C".into(), profile.windows_command(&extra)], env)
            } else {
                (
                    agents::login_shell(),
                    vec!["-l".into(), "-i".into(), "-c".into(), profile.shell_command(&extra)],
                    Vec::new(),
                )
            }
        }
        _ => {
            let shell = spec.shell.clone().unwrap_or_else(default_shell);
            let args = if cfg!(windows) { Vec::new() } else { vec!["-l".into()] };
            (shell, args, Vec::new())
        }
    }
}

pub fn default_shell() -> String {
    if cfg!(windows) {
        agents::find_in_path("pwsh.exe")
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|| std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into()))
    } else {
        agents::login_shell()
    }
}

fn blocked(key: &str) -> bool {
    key == "CLAUDECODE"
        || key.starts_with("CLAUDE_")
        || key.starts_with("CODEX_")
        || key == "PI_CODING_AGENT_DIR"
        || key.starts_with("TERM_PROGRAM")
        || key == "TERM_SESSION_ID"
}

fn environment(program: String, args: Vec<String>, extra: Vec<(String, String)>, cwd: PathBuf) -> Launch {
    let mut env: Vec<(String, String)> = Vec::new();
    let clear_env = cfg!(target_os = "macos");
    if clear_env {
        for key in ["HOME", "USER", "LOGNAME", "TMPDIR", "SSH_AUTH_SOCK", "__CF_USER_TEXT_ENCODING"] {
            if let Ok(value) = std::env::var(key) {
                env.push((key.into(), value));
            }
        }
        if std::env::var("HOME").is_err() {
            env.push(("HOME".into(), crate::paths::home().to_string_lossy().into_owned()));
        }
        env.push(("LANG".into(), std::env::var("LANG").unwrap_or_else(|_| "en_US.UTF-8".into())));
        env.push(("PATH".into(), "/usr/bin:/bin:/usr/sbin:/sbin".into()));
    }
    let remove_env = std::env::vars().map(|(key, _)| key).filter(|key| blocked(key)).collect();
    if !cfg!(windows) {
        env.push(("SHELL".into(), agents::login_shell()));
    }
    env.push(("TERM".into(), "xterm-256color".into()));
    env.push(("COLORTERM".into(), "truecolor".into()));
    env.push(("TERM_PROGRAM".into(), "Nucli".into()));
    env.push(("TERM_PROGRAM_VERSION".into(), env!("CARGO_PKG_VERSION").into()));
    env.push(("CLICOLOR".into(), "1".into()));
    env.push(("FORCE_HYPERLINK".into(), "1".into()));
    env.extend(extra);
    Launch { program, args, env, clear_env, remove_env, cwd }
}
