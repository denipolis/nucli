use std::path::PathBuf;
use std::time::{Duration, Instant};

use gpui::{
    AnimationExt, AnyElement, AppContext, Autocapitalize, Context, Entity, FocusHandle, Focusable, InteractiveElement,
    IntoElement, KeyDownEvent, Modifiers, ParentElement, Render, ScrollHandle, Styled, Task, TextInputAction, Window,
    div, point, px,
};
use nucli_core::fonts::FAMILY;
use nucli_core::session::Activity;
use nucli_core::uplink::protocol::{Request, Response};
use nucli_core::uplink::wire::Key;

use crate::field::{Field, FieldEvent};
use crate::link::Link;
use crate::menus::Clip;
use crate::mirror::Mirror;
use crate::sheet::Sheet;
use crate::{prefs, ui};

const TOAST: Duration = Duration::from_secs(4);

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
    pub slide: bool,
    pub fade: bool,
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
    pub notice: Option<String>,
    pub scroll: ScrollHandle,
    pub drawer_scroll: ScrollHandle,
    pub drawer_focus: FocusHandle,
    pub sheet: Option<Sheet>,
    pub sheet_focus: FocusHandle,
    pub input: Entity<Field>,
    pub message: Entity<Field>,
    pub clip: Option<Clip>,
    pub toast: Option<(String, Instant)>,
    focus: FocusHandle,
    ticker: Option<Task<()>>,
    moves: usize,
    shift: f32,
}

impl Home {
    pub fn new(link: Entity<Link>, window: &mut Window, cx: &mut Context<Self>) -> Home {
        cx.observe_in(&link, window, |home, link, window, cx| {
            let (paired, opened) = link.update(cx, |link, _| (link.paired.take(), link.opened.take()));
            if let Some(key) = paired
                && home.screen == Screen::Pair
            {
                home.history = vec![Screen::Computers];
                home.screen = Screen::Computer(key);
                home.moved(true);
                home.section = Section::Projects;
            }
            if let Some((key, tab)) = opened {
                home.open(key, tab, window, cx);
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
        let input = field("", Autocapitalize::None, TextInputAction::Done, cx);
        let message = field("commit message", Autocapitalize::Sentences, TextInputAction::Done, cx);
        cx.subscribe_in(&input, window, |home, _, _: &FieldEvent, window, cx| home.answer(window, cx)).detach();
        cx.subscribe(&message, |home, _, _: &FieldEvent, cx| home.commit_staged(cx)).detach();
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
            notice: None,
            scroll: ScrollHandle::new(),
            drawer_scroll: ScrollHandle::new(),
            drawer_focus: cx.focus_handle(),
            sheet: None,
            sheet_focus: cx.focus_handle(),
            input,
            message,
            clip: None,
            toast: None,
            focus: cx.focus_handle(),
            ticker: None,
            moves: 0,
            shift: 0.0,
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
        let slide = self.drawer.as_ref().is_none_or(|drawer| drawer.slide);
        self.drawer = Some(Drawer { panel, folder: folder.clone(), data: None, slide, fade: true });
        self.drawer_scroll.set_offset(point(px(0.0), px(0.0)));
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
        } else if let Some(drawer) = &mut self.drawer {
            drawer.slide = false;
            drawer.fade = false;
        }
        self.forgetting = None;
        self.notice = None;
        self.history.push(std::mem::replace(&mut self.screen, screen));
        self.moved(true);
        self.shown = Instant::now();
        self.load(cx);
        cx.notify();
    }

    pub fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.blur(cx);
        if self.sheet.take().is_some() {
            cx.notify();
            return;
        }
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
        self.notice = None;
        self.screen = self.history.pop().unwrap_or(Screen::Computers);
        if self.screen == Screen::Terminal && self.mirror.is_none() {
            self.screen = self.history.pop().unwrap_or(Screen::Computers);
        }
        self.moved(false);
        cx.notify();
    }

    pub fn start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        prefs::update(cx, |prefs| prefs.onboarded = true);
        self.history.clear();
        self.screen = Screen::Computers;
        self.go(Screen::Pair, window, cx);
    }

    pub fn open(&mut self, key: Key, tab: String, window: &mut Window, cx: &mut Context<Self>) {
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
        if window.last_input_was_keyboard() {
            mirror.update(cx, |mirror, cx| mirror.focus(window, cx));
        } else {
            window.blur(cx);
        }
        self.mirror = Some(mirror);
        if self.screen != Screen::Terminal {
            self.history.push(std::mem::replace(&mut self.screen, Screen::Terminal));
            self.moved(true);
        }
        cx.notify();
    }

    pub fn new_tab(&mut self, key: Key, project: &str, agent: bool, cx: &mut Context<Self>) {
        let link = self.link.read(cx);
        let launcher = link
            .online
            .get(&key)
            .and_then(|online| online.catalog.launchers.iter().find(|launcher| launcher.agent == agent))
            .map(|launcher| launcher.name.clone());
        if let Some(launcher) = launcher {
            self.link.update(cx, |link, _| link.open_tab(key, project, &launcher));
        }
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
                    home.moved(true);
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
            self.moved(true);
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

    pub fn commit_staged(&mut self, cx: &mut Context<Self>) {
        let message = self.message.read(cx).text.trim().to_string();
        let Screen::Project { key, project } = self.screen.clone() else { return };
        if message.is_empty() {
            self.say("Write a commit message first.");
            cx.notify();
            return;
        }
        self.send_then(
            key,
            Request::Act { project, action: nucli_core::uplink::protocol::Action::Commit { message } },
            |home, cx| home.message.update(cx, |field, cx| field.set("", cx)),
            cx,
        );
    }

    fn moved(&mut self, forward: bool) {
        self.scroll.set_offset(point(px(0.0), px(0.0)));
        self.moves += 1;
        self.shift = if forward { ui::SHIFT } else { -ui::SHIFT };
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let (key, modifiers) = (event.keystroke.key.as_str(), event.keystroke.modifiers);
        let plain = !modifiers.modified();
        match key {
            "down" | "tab" if plain => self.step(true, window, cx),
            "up" if plain => self.step(false, window, cx),
            "tab" if modifiers == Modifiers::shift() => self.step(false, window, cx),
            "escape" if plain && (!self.history.is_empty() || self.sheet.is_some()) => self.back(window, cx),
            "[" if modifiers == Modifiers::command() && (!self.history.is_empty() || self.sheet.is_some()) => {
                self.back(window, cx)
            }
            _ => return,
        }
        cx.stop_propagation();
    }

    fn step(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let scope = if self.sheet.is_some() {
            Some(self.sheet_focus.clone())
        } else if self.drawer.is_some() && matches!(self.screen, Screen::Project { .. }) {
            Some(self.drawer_focus.clone())
        } else {
            None
        };
        for _ in 0..256 {
            if forward {
                window.focus_next(cx);
            } else {
                window.focus_prev(cx);
            }
            if scope.as_ref().is_none_or(|scope| scope.contains_focused(window, cx)) {
                break;
            }
        }
        ui::reveal();
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
                    self.open(key, tab, window, cx);
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
            "download" => {
                if let Screen::Project { key, project } = self.screen.clone() {
                    self.download(key, &project, rest, cx);
                }
            }
            "upload" => {
                if let Screen::Project { key, project } = self.screen.clone() {
                    let done = self.link.update(cx, |link, cx| link.upload(key, &project, "", PathBuf::from(rest), cx));
                    cx.spawn(async move |this, cx| {
                        let outcome = done.await.unwrap_or_else(|_| Err("The transfer stopped.".into()));
                        let _ = this.update(cx, |home, cx| {
                            home.say(outcome.map_or_else(|reason| reason, |()| "uploaded".into()));
                            cx.notify();
                        });
                    })
                    .detach();
                }
            }
            "new" => {
                if let Screen::Project { key, project } = self.screen.clone() {
                    self.new_tab(key, &project, rest != "shell", cx);
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
        let shift = self.shift;
        let insets = window.insets().effective();
        let animated = match &self.screen {
            Screen::Welcome => self.shown.elapsed() < Duration::from_secs(3),
            Screen::Pair => true,
            Screen::Computer(_) if self.section == Section::History => self.conversations.is_none(),
            Screen::Folders(_) => self.folders.is_none(),
            Screen::File { .. } | Screen::Diff { .. } | Screen::Commit { .. } => self.view.is_none(),
            Screen::Project { .. } => {
                self.any_working(cx)
                    || self.notice.is_some()
                    || self.drawer.as_ref().is_some_and(|drawer| drawer.data.is_none())
            }
            _ => self.any_working(cx),
        };
        let moving = self.link.read(cx).transfers.values().next().map(|transfer| {
            let more = self.link.read(cx).transfers.len().saturating_sub(1);
            let verb = if transfer.upload { "uploading" } else { "downloading" };
            let share = if !transfer.upload && transfer.size > 0 {
                format!(" {}%", transfer.done * 100 / transfer.size)
            } else {
                String::new()
            };
            let others = if more > 0 { format!(" and {more} more") } else { String::new() };
            format!("{} {verb} {}{share}{others}", ui::spinner(self.shown.elapsed().as_millis()), transfer.name)
        });
        let toast =
            moving.or_else(|| self.toast.clone().filter(|(_, shown)| shown.elapsed() < TOAST).map(|(text, _)| text));
        self.animate(animated || toast.is_some(), cx);
        if window.focused(cx).is_none() {
            window.focus(&self.focus, cx);
        }
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
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .relative()
            .child(div().flex_1().min_h_0().flex().flex_col().child(body).with_animation(
                ("screen", self.moves),
                ui::motion(),
                move |this, delta| this.opacity(delta).relative().left(px((1.0 - delta) * shift)),
            ))
            .children(toast.map(|text| {
                div()
                    .absolute()
                    .left(insets.left + px(16.0))
                    .right(insets.right + px(16.0))
                    .bottom(insets.bottom + px(16.0))
                    .px(px(12.0))
                    .py(px(10.0))
                    .bg(theme.surface)
                    .border_1()
                    .border_color(theme.border)
                    .text_size(px(ui::SMALL))
                    .child(text)
                    .with_animation(("toast", self.moves), ui::motion(), |this, delta| this.opacity(delta))
            }))
            .children(self.sheet_view(&theme, cx).map(|sheet| {
                div()
                    .absolute()
                    .inset_0()
                    .pt(insets.top)
                    .pb(insets.bottom)
                    .pl(insets.left)
                    .pr(insets.right)
                    .flex()
                    .flex_col()
                    .child(div().flex_1().relative().child(sheet))
            }))
    }
}
