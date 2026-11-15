use std::time::{Duration, Instant};

use gpui::{
    AnyElement, AppContext, Autocapitalize, Context, Entity, Focusable, IntoElement, ParentElement, Render, Styled,
    Task, TextInputAction, Window, div, px,
};
use nucli_core::fonts::FAMILY;
use nucli_core::session::Activity;
use nucli_core::uplink::protocol::{Request, Response};
use nucli_core::uplink::wire::Key;

use crate::field::{Field, FieldEvent};
use crate::link::Link;
use crate::mirror::Mirror;
use crate::{prefs, ui};

#[derive(Clone, PartialEq)]
pub enum Screen {
    Welcome,
    Computers,
    Computer(Key),
    Project { key: Key, project: String },
    Folders(Key),
    File { key: Key, project: String, path: String },
    Diff { key: Key, project: String, path: String, commit: Option<String> },
    Commit { key: Key, project: String, hash: String },
    Pair,
    Device(Key),
    Settings,
    Terminal,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Section {
    Projects,
    Agents,
    History,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Panel {
    Files,
    Changes,
    Log,
}

pub struct Drawer {
    pub panel: Panel,
    pub folder: String,
    pub data: Option<Response>,
}

pub struct Home {
    pub link: Entity<Link>,
    pub screen: Screen,
    history: Vec<Screen>,
    pub section: Section,
    pub mirror: Option<Entity<Mirror>>,
    pub code: Entity<Field>,
    pub address: Entity<Field>,
    pub name: Entity<Field>,
    pub reach: Entity<Field>,
    pub forgetting: Option<Key>,
    pub shown: Instant,
    pub scanning: Option<String>,
    pub chosen: Option<Key>,
    pub conversations: Option<Response>,
    pub folders: Option<Response>,
    pub view: Option<Response>,
    pub drawer: Option<Drawer>,
    pub sheet: Option<bool>,
    pub notice: Option<String>,
    ticker: Option<Task<()>>,
}

impl Home {
    pub fn new(link: Entity<Link>, window: &mut Window, cx: &mut Context<Self>) -> Home {
        cx.observe(&link, |home, link, cx| {
            let (paired, opened) = link.update(cx, |link, _| (link.paired.take(), link.opened.take()));
            if let Some(key) = paired
                && home.screen == Screen::Pair
            {
                home.history = vec![Screen::Computers];
                home.screen = Screen::Computer(key);
                home.section = Section::Projects;
            }
            if let Some((key, tab)) = opened {
                home.open(key, tab, cx);
            }
            cx.notify();
        })
        .detach();
        cx.observe_window_appearance(window, |_, window, _| window.refresh()).detach();
        let field = |placeholder, capitalize, action, cx: &mut Context<Self>| {
            cx.new(|cx| Field::new(placeholder, capitalize, action, cx))
        };
        let code = field("K7QM-4XRP", Autocapitalize::Characters, TextInputAction::Next, cx);
        let address = field("computer name or address", Autocapitalize::None, TextInputAction::Go, cx);
        let name = field("this phone's name", Autocapitalize::Words, TextInputAction::Done, cx);
        let reach = field("names or addresses, comma separated", Autocapitalize::None, TextInputAction::Done, cx);
        cx.subscribe_in(&code, window, |home, _, _: &FieldEvent, window, cx| {
            window.focus(&home.address.focus_handle(cx), cx)
        })
        .detach();
        cx.subscribe(&address, |home, _, _: &FieldEvent, cx| home.pair_with_code(cx)).detach();
        cx.subscribe(&name, |home, field, _: &FieldEvent, cx| {
            let text = field.read(cx).text.trim().to_string();
            if !text.is_empty() {
                prefs::update(cx, |prefs| prefs.name = text.clone());
                home.link.update(cx, |link, _| link.rename(&text));
            }
        })
        .detach();
        cx.subscribe(&reach, |home, field, _: &FieldEvent, cx| {
            if let Screen::Device(key) = home.screen {
                let text = field.read(cx).text.clone();
                home.link.update(cx, |link, cx| link.set_manual(&key, &text, cx));
            }
        })
        .detach();
        let prefs = prefs::get(cx).clone();
        let screen = if prefs.onboarded { Screen::Computers } else { Screen::Welcome };
        name.update(cx, |field, cx| field.set(prefs.name.clone(), cx));
        Home {
            link,
            screen,
            history: Vec::new(),
            section: Section::Projects,
            mirror: None,
            code,
            address,
            name,
            reach,
            forgetting: None,
            shown: Instant::now(),
            scanning: None,
            chosen: None,
            conversations: None,
            folders: None,
            view: None,
            drawer: None,
            sheet: None,
            notice: None,
            ticker: None,
        }
    }

    pub fn fetch(
        &mut self,
        key: Key,
        request: Request,
        apply: impl FnOnce(&mut Home, Response) + 'static,
        cx: &mut Context<Self>,
    ) {
        let receiver = self.link.update(cx, |link, cx| link.ask(key, request, cx));
        cx.spawn(async move |this, cx| {
            let response = receiver.await.unwrap_or(Response::Failed { reason: "No answer.".into() });
            let _ = this.update(cx, |home, cx| {
                apply(home, response);
                cx.notify();
            });
        })
        .detach();
    }

    fn load(&mut self, cx: &mut Context<Self>) {
        let screen = self.screen.clone();
        let current = move |home: &Home| home.screen == screen;
        match self.screen.clone() {
            Screen::Computer(key) if self.section == Section::History => {
                self.conversations = None;
                self.fetch(
                    key,
                    Request::History,
                    move |home, response| {
                        if current(home) {
                            home.conversations = Some(response);
                        }
                    },
                    cx,
                );
            }
            Screen::Folders(key) => {
                self.folders = None;
                self.fetch(
                    key,
                    Request::Folders { path: String::new() },
                    |home, response| home.folders = Some(response),
                    cx,
                );
            }
            Screen::File { key, project, path } => {
                self.view = None;
                self.fetch(
                    key,
                    Request::File { project, path },
                    move |home, response| {
                        if current(home) {
                            home.view = Some(response);
                        }
                    },
                    cx,
                );
            }
            Screen::Diff { key, project, path, commit } => {
                self.view = None;
                self.fetch(
                    key,
                    Request::Diff { project, path, commit },
                    move |home, response| {
                        if current(home) {
                            home.view = Some(response);
                        }
                    },
                    cx,
                );
            }
            Screen::Commit { key, project, hash } => {
                self.view = None;
                self.fetch(
                    key,
                    Request::Commit { project, hash },
                    move |home, response| {
                        if current(home) {
                            home.view = Some(response);
                        }
                    },
                    cx,
                );
            }
            _ => {}
        }
    }

    pub fn set_section(&mut self, section: Section, cx: &mut Context<Self>) {
        self.section = section;
        self.load(cx);
        cx.notify();
    }

    pub fn browse(&mut self, key: Key, path: String, cx: &mut Context<Self>) {
        self.folders = None;
        self.fetch(key, Request::Folders { path }, |home, response| home.folders = Some(response), cx);
        cx.notify();
    }

    pub fn open_drawer(&mut self, key: Key, project: String, panel: Panel, folder: String, cx: &mut Context<Self>) {
        self.drawer = Some(Drawer { panel, folder: folder.clone(), data: None });
        let request = match panel {
            Panel::Files => Request::Files { project, folder: folder.clone() },
            Panel::Changes => Request::Changes { project },
            Panel::Log => Request::Log { project },
        };
        self.fetch(
            key,
            request,
            move |home, response| {
                if let Some(drawer) = &mut home.drawer
                    && drawer.panel == panel
                    && drawer.folder == folder
                {
                    drawer.data = Some(response);
                }
            },
            cx,
        );
        cx.notify();
    }

    pub fn go(&mut self, screen: Screen, window: &mut Window, cx: &mut Context<Self>) {
        window.blur(cx);
        if let Screen::Device(key) = &screen {
            let manual = self.link.read(cx).device(key).map(|device| device.manual.join(", ")).unwrap_or_default();
            self.reach.update(cx, |field, cx| field.set(manual, cx));
        }
        if screen == Screen::Pair {
            self.link.update(cx, |link, _| link.note = None);
            self.scanning = None;
        }
        if matches!(screen, Screen::Computer(_)) {
            self.section = Section::Projects;
        }
        if !matches!(screen, Screen::File { .. } | Screen::Diff { .. } | Screen::Commit { .. }) {
            self.drawer = None;
        }
        self.forgetting = None;
        self.sheet = None;
        self.notice = None;
        self.history.push(std::mem::replace(&mut self.screen, screen));
        self.shown = Instant::now();
        self.load(cx);
        cx.notify();
    }

    pub fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.blur(cx);
        if self.drawer.is_some() && matches!(self.screen, Screen::Project { .. }) {
            self.drawer = None;
            cx.notify();
            return;
        }
        if self.screen == Screen::Terminal
            && let Some(mirror) = self.mirror.take()
        {
            let (key, tab) = {
                let mirror = mirror.read(cx);
                (mirror.key, mirror.tab.clone())
            };
            self.link.update(cx, |link, _| link.unwatch(key, &tab));
        }
        self.forgetting = None;
        self.sheet = None;
        self.notice = None;
        self.screen = self.history.pop().unwrap_or(Screen::Computers);
        if self.screen == Screen::Terminal && self.mirror.is_none() {
            self.screen = self.history.pop().unwrap_or(Screen::Computers);
        }
        cx.notify();
    }

    pub fn start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        prefs::update(cx, |prefs| prefs.onboarded = true);
        self.history.clear();
        self.screen = Screen::Computers;
        self.go(Screen::Pair, window, cx);
    }

    pub fn open(&mut self, key: Key, tab: String, cx: &mut Context<Self>) {
        if let Some(previous) = self.mirror.take() {
            let (old_key, old_tab) = {
                let previous = previous.read(cx);
                (previous.key, previous.tab.clone())
            };
            self.link.update(cx, |link, _| link.unwatch(old_key, &old_tab));
        }
        let link = self.link.clone();
        let mirror = cx.new(|cx| Mirror::new(link, key, tab, cx));
        cx.observe(&mirror, |_, _, cx| cx.notify()).detach();
        self.mirror = Some(mirror);
        self.sheet = None;
        if self.screen != Screen::Terminal {
            self.history.push(std::mem::replace(&mut self.screen, Screen::Terminal));
        }
        cx.notify();
    }

    pub fn launch(&mut self, key: Key, project: &str, launcher: &str, cx: &mut Context<Self>) {
        self.sheet = None;
        self.notice = None;
        self.shown = Instant::now();
        self.link.update(cx, |link, _| link.open_tab(key, project, launcher));
        cx.notify();
    }

    pub fn resume(
        &mut self,
        key: Key,
        conversation: String,
        project: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.go(Screen::Project { key, project }, window, cx);
        self.notice = Some("resuming…".into());
        self.fetch(
            key,
            Request::Resume { conversation },
            |home, response| {
                home.notice = match response {
                    Response::Failed { reason } => Some(reason),
                    _ => None,
                };
            },
            cx,
        );
    }

    pub fn add_project(&mut self, key: Key, path: String, window: &mut Window, cx: &mut Context<Self>) {
        let project = path.clone();
        self.fetch(
            key,
            Request::AddProject { path },
            move |home, response| match response {
                Response::Failed { reason } => home.notice = Some(reason),
                _ => {
                    home.screen = home.history.pop().unwrap_or(Screen::Computers);
                    home.history.push(home.screen.clone());
                    home.screen = Screen::Project { key, project };
                }
            },
            cx,
        );
        window.blur(cx);
    }

    pub fn target(&self, cx: &Context<Self>) -> Option<Key> {
        let link = self.link.read(cx);
        let pairable = link.pairable();
        match self.chosen.filter(|chosen| pairable.iter().any(|nearby| nearby.key == *chosen)) {
            Some(chosen) => Some(chosen),
            None if self.address.read(cx).text.trim().is_empty() && pairable.len() == 1 => Some(pairable[0].key),
            None => None,
        }
    }

    pub fn pair_with_code(&mut self, cx: &mut Context<Self>) {
        let code = self.code.read(cx).text.clone();
        let address = self.address.read(cx).text.clone();
        let choose = address.trim().is_empty() && !self.link.read(cx).pairable().is_empty();
        match self.target(cx) {
            Some(key) => self.link.update(cx, |link, cx| link.join_nearby(key, &code, cx)),
            None if choose => self.link.update(cx, |link, cx| {
                link.note = Some("Choose the computer above, or type its name or address.".into());
                cx.notify();
            }),
            None => self.link.update(cx, |link, cx| link.join_code(&code, &address, cx)),
        }
    }

    pub fn open_url(&mut self, url: &str, cx: &mut Context<Self>) {
        if self.screen != Screen::Pair {
            self.history.push(std::mem::replace(&mut self.screen, Screen::Pair));
        }
        prefs::update(cx, |prefs| prefs.onboarded = true);
        self.pair_with_link(url, cx);
    }

    pub fn choose(&mut self, key: Key, cx: &mut Context<Self>) {
        self.chosen = Some(key);
        self.link.update(cx, |link, _| link.note = None);
        cx.notify();
    }

    pub fn pair_with_link(&mut self, text: &str, cx: &mut Context<Self>) {
        self.link.update(cx, |link, cx| link.pair(text, cx));
    }

    fn animate(&mut self, needed: bool, cx: &mut Context<Self>) {
        if !needed {
            self.ticker = None;
        } else if self.ticker.is_none() {
            self.ticker = Some(cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(Duration::from_millis(ui::SPINNER_MS as u64)).await;
                    if this.update(cx, |_, cx| cx.notify()).is_err() {
                        break;
                    }
                }
            }));
        }
    }

    fn computer_named(&self, name: &str, cx: &Context<Self>) -> Option<Key> {
        self.link
            .read(cx)
            .devices
            .iter()
            .find(|device| name.is_empty() || device.name == name)
            .and_then(|device| device.public())
    }

    pub fn control(&mut self, line: &str, window: &mut Window, cx: &mut Context<Self>) {
        let (command, rest) = line.split_once(' ').unwrap_or((line, ""));
        let mirror = self.mirror.clone();
        match command {
            "open" => {
                let found = self.link.read(cx).online.iter().find_map(|(key, online)| {
                    let tab = online.catalog.tabs.iter().find(|tab| tab.title.contains(rest))?;
                    Some((*key, tab.id.clone()))
                });
                if let Some((key, tab)) = found {
                    self.open(key, tab, cx);
                }
            }
            "back" => self.back(window, cx),
            "start" => self.start(window, cx),
            "computer" => {
                if let Some(key) = self.computer_named(rest, cx) {
                    self.go(Screen::Computer(key), window, cx);
                }
            }
            "section" => {
                let section = match rest {
                    "agents" => Section::Agents,
                    "history" => Section::History,
                    _ => Section::Projects,
                };
                self.set_section(section, cx);
            }
            "project" => {
                if let Screen::Computer(key) = self.screen.clone() {
                    let path = self.link.read(cx).online.get(&key).and_then(|online| {
                        online
                            .catalog
                            .projects
                            .iter()
                            .find(|project| project.name == rest)
                            .map(|project| project.path.clone())
                    });
                    if let Some(project) = path {
                        self.go(Screen::Project { key, project }, window, cx);
                    }
                }
            }
            "drawer" => {
                if let Screen::Project { key, project } = self.screen.clone() {
                    let panel = match rest {
                        "changes" => Panel::Changes,
                        "history" => Panel::Log,
                        _ => Panel::Files,
                    };
                    self.open_drawer(key, project, panel, String::new(), cx);
                }
            }
            "file" | "diff" | "commit" => {
                if let Screen::Project { key, project } = self.screen.clone() {
                    let (project, path) = (project, rest.to_string());
                    let screen = match command {
                        "file" => Screen::File { key, project, path },
                        "diff" => Screen::Diff { key, project, path, commit: None },
                        _ => Screen::Commit { key, project, hash: path },
                    };
                    self.go(screen, window, cx);
                }
            }
            "browse" | "add" => {
                if let Screen::Folders(key) = self.screen {
                    if command == "browse" {
                        self.browse(key, rest.to_string(), cx);
                    } else {
                        self.add_project(key, rest.to_string(), window, cx);
                    }
                }
            }
            "resume" => {
                if let Screen::Computer(key) = self.screen
                    && let Some(Response::History { conversations }) = &self.conversations
                    && let Some(item) = conversations.iter().find(|item| item.title.contains(rest))
                {
                    let (id, project) = (item.id.clone(), item.project.clone());
                    self.resume(key, id, project, window, cx);
                }
            }
            "sheet" => {
                self.sheet = Some(rest != "shell");
                cx.notify();
            }
            "screen" => {
                let (what, name) = rest.split_once(' ').unwrap_or((rest, ""));
                let key = self.computer_named(name, cx);
                let screen = match what {
                    "pair" => Some(Screen::Pair),
                    "settings" => Some(Screen::Settings),
                    "device" => key.map(Screen::Device),
                    "folders" => key.map(Screen::Folders),
                    _ => None,
                };
                if let Some(screen) = screen {
                    self.go(screen, window, cx);
                }
            }
            "choose" => {
                let key = self.link.read(cx).nearby.iter().find(|nearby| nearby.name == rest).map(|nearby| nearby.key);
                if let Some(key) = key {
                    self.choose(key, cx);
                }
            }
            "pairlink" => self.pair_with_link(rest, cx),
            "code" => {
                let (code, address) = rest.split_once(' ').unwrap_or((rest, ""));
                self.code.update(cx, |field, cx| field.set(code, cx));
                self.address.update(cx, |field, cx| field.set(address, cx));
                self.pair_with_code(cx);
            }
            "launch" => {
                if let Screen::Project { key, project } = self.screen.clone() {
                    self.launch(key, &project, rest, cx);
                }
            }
            "focus" => {
                if let Some(mirror) = mirror {
                    mirror.update(cx, |mirror, cx| mirror.focus(window, cx));
                }
            }
            "type" => {
                if let Some(mirror) = mirror {
                    mirror.update(cx, |mirror, cx| mirror.type_text(rest, cx));
                }
            }
            "key" => {
                if let Some(mirror) = mirror {
                    mirror.update(cx, |mirror, cx| mirror.press(rest, cx));
                }
            }
            _ => {}
        }
    }

    pub fn any_working(&self, cx: &Context<Self>) -> bool {
        self.link
            .read(cx)
            .online
            .values()
            .flat_map(|online| &online.catalog.tabs)
            .any(|tab| tab.running && tab.activity == Activity::Working)
    }
}

impl Render for Home {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = ui::theme(window, cx);
        let insets = window.insets().effective();
        let animated = match &self.screen {
            Screen::Welcome => self.shown.elapsed() < Duration::from_secs(3),
            Screen::Pair => true,
            Screen::Computer(_) if self.section == Section::History => self.conversations.is_none(),
            Screen::Folders(_) => self.folders.is_none(),
            Screen::File { .. } | Screen::Diff { .. } | Screen::Commit { .. } => self.view.is_none(),
            Screen::Project { key, .. } => {
                self.any_working(cx)
                    || self.notice.is_some()
                    || self.link.read(cx).is_opening(*key)
                    || self.drawer.as_ref().is_some_and(|drawer| drawer.data.is_none())
            }
            _ => self.any_working(cx),
        };
        self.animate(animated, cx);
        let body: AnyElement = match self.screen.clone() {
            Screen::Welcome => self.welcome(&theme, cx),
            Screen::Computers => self.computers(&theme, cx),
            Screen::Computer(key) => self.computer(key, &theme, cx),
            Screen::Project { key, project } => self.project(key, &project, &theme, cx),
            Screen::Folders(key) => self.folders_screen(key, &theme, cx),
            Screen::File { path, .. } => self.file(&path, &theme, cx),
            Screen::Diff { path, commit, .. } => self.diff(&path, commit, &theme, cx),
            Screen::Commit { key, project, hash } => self.commit(key, &project, &hash, &theme, cx),
            Screen::Pair => self.pair(&theme, cx),
            Screen::Device(key) => self.device(key, &theme, cx),
            Screen::Settings => self.settings(&theme, cx),
            Screen::Terminal => match self.mirror.clone() {
                Some(mirror) => self.terminal(mirror, &theme, window, cx),
                None => self.computers(&theme, cx),
            },
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .pt(insets.top)
            .pb(insets.bottom)
            .pl(insets.left)
            .pr(insets.right)
            .bg(theme.background)
            .text_color(theme.text)
            .font_family(FAMILY)
            .text_size(px(ui::TEXT))
            .child(body)
    }
}
