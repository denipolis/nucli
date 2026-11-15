use std::time::{Duration, Instant};

use gpui::{
    AnyElement, AppContext, Autocapitalize, Context, Entity, Focusable, IntoElement, ParentElement, Render, Styled,
    Task, TextInputAction, Window, div, px,
};
use nucli_core::fonts::FAMILY;
use nucli_core::session::Activity;
use nucli_core::uplink::wire::Key;

use crate::field::{Field, FieldEvent};
use crate::link::Link;
use crate::mirror::Mirror;
use crate::{prefs, ui};

#[derive(Clone, PartialEq)]
pub enum Screen {
    Welcome,
    Main,
    Pair,
    Device(Key),
    Settings,
    Launch { key: Key, project: String },
    Terminal,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Board {
    Agents,
    Computers,
}

pub struct Home {
    pub link: Entity<Link>,
    pub screen: Screen,
    history: Vec<Screen>,
    pub board: Board,
    pub mirror: Option<Entity<Mirror>>,
    pub code: Entity<Field>,
    pub address: Entity<Field>,
    pub name: Entity<Field>,
    pub reach: Entity<Field>,
    pub forgetting: Option<Key>,
    pub shown: Instant,
    pub scanning: Option<String>,
    pub chosen: Option<Key>,
    ticker: Option<Task<()>>,
}

impl Home {
    pub fn new(link: Entity<Link>, window: &mut Window, cx: &mut Context<Self>) -> Home {
        cx.observe(&link, |home, link, cx| {
            let (paired, opened) = link.update(cx, |link, _| (link.paired.take(), link.opened.take()));
            if paired.is_some() && home.screen == Screen::Pair {
                home.history.clear();
                home.screen = Screen::Main;
                home.board = Board::Computers;
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
        let screen = if prefs.onboarded { Screen::Main } else { Screen::Welcome };
        name.update(cx, |field, cx| field.set(prefs.name.clone(), cx));
        Home {
            link,
            screen,
            history: Vec::new(),
            board: Board::Agents,
            mirror: None,
            code,
            address,
            name,
            reach,
            forgetting: None,
            shown: Instant::now(),
            scanning: None,
            chosen: None,
            ticker: None,
        }
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
        self.forgetting = None;
        self.history.push(std::mem::replace(&mut self.screen, screen));
        self.shown = Instant::now();
        cx.notify();
    }

    pub fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.blur(cx);
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
        self.screen = self.history.pop().unwrap_or(Screen::Main);
        if self.screen == Screen::Terminal && self.mirror.is_none() {
            self.screen = self.history.pop().unwrap_or(Screen::Main);
        }
        cx.notify();
    }

    pub fn start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        prefs::update(cx, |prefs| prefs.onboarded = true);
        self.history.clear();
        self.screen = Screen::Main;
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
        if self.screen != Screen::Terminal {
            if matches!(self.screen, Screen::Launch { .. }) {
                self.screen = self.history.pop().unwrap_or(Screen::Main);
            }
            self.history.push(std::mem::replace(&mut self.screen, Screen::Terminal));
        }
        cx.notify();
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
        match self.target(cx) {
            Some(key) => self.link.update(cx, |link, cx| link.join_nearby(key, &code, cx)),
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

    pub fn control(&mut self, line: &str, window: &mut Window, cx: &mut Context<Self>) {
        let (command, rest) = line.split_once(' ').unwrap_or((line, ""));
        match (command, self.mirror.clone()) {
            ("open", _) => {
                let found = self.link.read(cx).online.iter().find_map(|(key, online)| {
                    let tab = online.catalog.tabs.iter().find(|tab| tab.title.contains(rest))?;
                    Some((*key, tab.id.clone()))
                });
                if let Some((key, tab)) = found {
                    self.open(key, tab, cx);
                }
            }
            ("back", _) => self.back(window, cx),
            ("start", _) => self.start(window, cx),
            ("screen", _) => {
                let key = self.link.read(cx).devices.first().and_then(|device| device.public());
                let screen = match rest {
                    "pair" => Some(Screen::Pair),
                    "settings" => Some(Screen::Settings),
                    "device" => key.map(Screen::Device),
                    "launch" => self.link.read(cx).online.iter().find_map(|(key, online)| {
                        let project = online.catalog.projects.first()?;
                        Some(Screen::Launch { key: *key, project: project.path.clone() })
                    }),
                    _ => None,
                };
                if let Some(screen) = screen {
                    self.go(screen, window, cx);
                }
            }
            ("launch", _) => {
                if let Screen::Launch { key, project } = self.screen.clone() {
                    self.link.update(cx, |link, _| link.open_tab(key, &project, rest));
                }
            }
            ("board", _) => {
                self.board = if rest == "computers" { Board::Computers } else { Board::Agents };
                cx.notify();
            }
            ("pairlink", _) => self.pair_with_link(rest, cx),
            ("code", _) => {
                let (code, address) = rest.split_once(' ').unwrap_or((rest, ""));
                self.code.update(cx, |field, cx| field.set(code, cx));
                self.address.update(cx, |field, cx| field.set(address, cx));
                self.pair_with_code(cx);
            }
            ("focus", Some(mirror)) => mirror.update(cx, |mirror, cx| mirror.focus(window, cx)),
            ("type", Some(mirror)) => mirror.update(cx, |mirror, cx| mirror.type_text(rest, cx)),
            ("key", Some(mirror)) => mirror.update(cx, |mirror, cx| mirror.press(rest, cx)),
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
            Screen::Main | Screen::Terminal => self.any_working(cx),
            Screen::Pair => true,
            Screen::Launch { key, .. } => self.link.read(cx).is_opening(*key),
            _ => false,
        };
        self.animate(animated, cx);
        let body: AnyElement = match self.screen.clone() {
            Screen::Welcome => self.welcome(&theme, cx),
            Screen::Main => self.main(&theme, cx),
            Screen::Pair => self.pair(&theme, cx),
            Screen::Device(key) => self.device(key, &theme, cx),
            Screen::Settings => self.settings(&theme, cx),
            Screen::Launch { key, project } => self.launch(key, &project, &theme, cx),
            Screen::Terminal => match self.mirror.clone() {
                Some(mirror) => self.terminal(mirror, &theme, window, cx),
                None => self.main(&theme, cx),
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
