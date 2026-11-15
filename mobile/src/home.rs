use gpui::{
    AnyElement, AppContext, Context, Div, Entity, InteractiveElement, IntoElement, ParentElement, Render, SharedString,
    Stateful, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};
use nucli_core::fonts::FAMILY;
use nucli_core::icons;
use nucli_core::session::{Activity, Kind};
use nucli_core::theme::Theme;
use nucli_core::uplink::protocol::RemoteTab;
use nucli_core::uplink::wire::{self, Key};

use crate::link::Link;
use crate::mirror::Mirror;

const KEYS: [(&str, &str); 7] =
    [("escape", "esc"), ("tab", "tab"), ("up", "↑"), ("down", "↓"), ("left", "←"), ("right", "→"), ("enter", "⏎")];

pub struct Home {
    link: Entity<Link>,
    mirror: Option<Entity<Mirror>>,
}

impl Home {
    pub fn new(link: Entity<Link>, cx: &mut Context<Self>) -> Home {
        cx.observe(&link, |_, _, cx| cx.notify()).detach();
        Home { link, mirror: None }
    }

    fn open(&mut self, key: Key, tab: String, cx: &mut Context<Self>) {
        let link = self.link.clone();
        let mirror = cx.new(|cx| Mirror::new(link, key, tab, cx));
        cx.observe(&mirror, |_, _, cx| cx.notify()).detach();
        self.mirror = Some(mirror);
        cx.notify();
    }

    fn back(&mut self, cx: &mut Context<Self>) {
        if let Some(mirror) = self.mirror.take() {
            let (key, tab) = {
                let mirror = mirror.read(cx);
                (mirror.key, mirror.tab.clone())
            };
            self.link.update(cx, |link, _| link.unwatch(key, &tab));
        }
        cx.notify();
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
            ("back", _) => self.back(cx),
            ("focus", Some(mirror)) => mirror.update(cx, |mirror, cx| mirror.focus(window, cx)),
            ("type", Some(mirror)) => mirror.update(cx, |mirror, cx| mirror.type_text(rest, cx)),
            ("key", Some(mirror)) => mirror.update(cx, |mirror, cx| mirror.press(rest, cx)),
            _ => {}
        }
    }

    fn devices(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let link = self.link.read(cx);
        let header = bar(theme).child(div().flex_1().child("nucli")).child(
            button("pair", "[paste pairing link]", theme).on_click(cx.listener(|home, _, _, cx| {
                let text = cx.read_from_clipboard().and_then(|item| item.text()).unwrap_or_default();
                home.link.update(cx, |link, cx| link.pair(&text, cx));
            })),
        );
        let mut list = div().id("devices").flex_1().min_h_0().overflow_y_scroll().flex().flex_col();
        if let Some(note) = &link.note {
            list = list.child(div().px(px(16.0)).py(px(10.0)).text_color(theme.muted).child(note.clone()));
        }
        if link.joining {
            list = list.child(div().px(px(16.0)).py(px(10.0)).text_color(theme.muted).child("pairing…"));
        }
        if link.devices.is_empty() {
            list = list.child(
                div()
                    .px(px(16.0))
                    .py(px(10.0))
                    .text_color(theme.faint)
                    .child("On your computer open Settings › Uplink, choose add device and copy pairing link. Then tap [paste pairing link]."),
            );
        }
        for device in &link.devices {
            let Some(key) = device.public() else { continue };
            let online = link.online.get(&key);
            let state = match online {
                Some(_) => "online".to_string(),
                None => link.device_error(&key).unwrap_or("offline").to_string(),
            };
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .px(px(16.0))
                    .pt(px(16.0))
                    .pb(px(6.0))
                    .child(div().text_color(theme.accent).child(icons::os(&device.os)))
                    .child(div().flex_1().child(device.name.clone()))
                    .child(div().text_color(if online.is_some() { theme.green } else { theme.faint }).child(state)),
            );
            let Some(online) = online else { continue };
            if online.catalog.tabs.is_empty() {
                list = list.child(div().px(px(42.0)).py(px(6.0)).text_color(theme.faint).child("no shared tabs"));
            }
            for tab in &online.catalog.tabs {
                let project = online
                    .catalog
                    .projects
                    .iter()
                    .find(|project| project.path == tab.project)
                    .map(|project| project.name.clone())
                    .unwrap_or_default();
                let id = SharedString::from(format!("{}:{}", wire::encode_key(&key), tab.id));
                let tab_id = tab.id.clone();
                list = list.child(
                    div()
                        .id(id)
                        .flex()
                        .items_center()
                        .gap(px(10.0))
                        .px(px(16.0))
                        .py(px(10.0))
                        .border_b_1()
                        .border_color(theme.border)
                        .child(div().w(px(16.0)).text_color(glyph_color(tab, theme)).child(glyph(tab)))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .flex_1()
                                .min_w_0()
                                .child(div().truncate().child(tab.title.clone()))
                                .child(div().truncate().text_color(theme.faint).text_size(px(12.0)).child(project)),
                        )
                        .child(status(tab, theme))
                        .on_click(cx.listener(move |home, _, _, cx| home.open(key, tab_id.clone(), cx))),
                );
            }
        }
        div().flex_1().min_h_0().flex().flex_col().child(header).child(list).into_any_element()
    }

    fn terminal(&self, mirror: Entity<Mirror>, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let (key, tab, ctrl) = {
            let mirror = mirror.read(cx);
            (mirror.key, mirror.tab.clone(), mirror.ctrl)
        };
        let link = self.link.read(cx);
        let device = link
            .devices
            .iter()
            .find(|device| device.public().as_ref() == Some(&key))
            .map(|device| device.name.clone())
            .unwrap_or_default();
        let online = link.online.get(&key);
        let title = online
            .and_then(|online| online.catalog.tabs.iter().find(|item| item.id == tab))
            .map(|item| item.title.clone())
            .unwrap_or_default();
        let state = if online.is_none() {
            "offline"
        } else if !link.is_watched(key, &tab) {
            "closed"
        } else {
            ""
        };
        let header = bar(theme)
            .child(button("back", "‹ back", theme).on_click(cx.listener(|home, _, _, cx| home.back(cx))))
            .child(div().flex_1().min_w_0().truncate().child(format!("{device} › {title}")))
            .child(div().text_color(theme.yellow).child(state));
        let mut keys =
            div().flex().items_center().justify_between().px(px(6.0)).border_t_1().border_color(theme.border);
        keys = keys.child(
            button("ctrl", "ctrl", theme)
                .when(ctrl, |button| button.bg(theme.accent).text_color(theme.on_accent))
                .on_click(cx.listener(|home, _, _, cx| {
                    if let Some(mirror) = &home.mirror {
                        mirror.update(cx, |mirror, cx| {
                            mirror.ctrl = !mirror.ctrl;
                            cx.notify();
                        });
                    }
                })),
        );
        for (name, label) in KEYS {
            keys = keys.child(button(name, label, theme).on_click(cx.listener(move |home, _, _, cx| {
                if let Some(mirror) = &home.mirror {
                    mirror.update(cx, |mirror, cx| mirror.press(name, cx));
                }
            })));
        }
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(header)
            .child(div().flex_1().min_h_0().child(mirror))
            .child(keys)
            .into_any_element()
    }
}

impl Render for Home {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = crate::theme(window);
        let insets = window.insets().effective();
        let body = match self.mirror.clone() {
            Some(mirror) => self.terminal(mirror, &theme, cx),
            None => self.devices(&theme, cx),
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
            .text_size(px(15.0))
            .child(body)
    }
}

fn bar(theme: &Theme) -> Div {
    div().flex().items_center().gap(px(8.0)).h(px(44.0)).px(px(10.0)).border_b_1().border_color(theme.border)
}

fn button(id: &'static str, label: &'static str, theme: &Theme) -> Stateful<Div> {
    div().id(id).px(px(10.0)).py(px(8.0)).text_color(theme.accent).child(label)
}

fn glyph(tab: &RemoteTab) -> &'static str {
    match (tab.kind, &tab.agent) {
        (Kind::Agent, Some(agent)) => icons::agent(agent).0,
        (Kind::Agent, None) => icons::AGENT,
        _ => icons::TERMINAL,
    }
}

fn glyph_color(tab: &RemoteTab, theme: &Theme) -> gpui::Hsla {
    match (tab.kind, &tab.agent) {
        (Kind::Agent, Some(agent)) => icons::agent(agent).1.color(theme.dark),
        _ => tab.color.map_or(theme.muted, |color| color.color(theme.dark)),
    }
}

fn status(tab: &RemoteTab, theme: &Theme) -> Div {
    let (text, color) = if !tab.running {
        ("exited", theme.faint)
    } else if tab.activity == Activity::Waiting {
        ("waiting", theme.yellow)
    } else if tab.activity == Activity::Working {
        (icons::SPINNER[0], theme.accent)
    } else if tab.attention {
        ("done", theme.green)
    } else {
        ("", theme.faint)
    };
    div().text_color(color).text_size(px(12.0)).child(text)
}
