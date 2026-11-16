use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement, SharedString, StatefulInteractiveElement,
    Styled, div, px,
};
use nucli_core::icons;
use nucli_core::theme::{self, Theme};

use crate::home::{Home, Screen};
use crate::prefs::{self, Appearance, FONT_SIZES};
use crate::ui;

const APPEARANCES: [(Appearance, &str); 3] =
    [(Appearance::System, "system"), (Appearance::Light, "light"), (Appearance::Dark, "dark")];

impl Home {
    pub fn settings(&mut self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let current = prefs::get(cx).clone();
        let link = self.link.read(cx);
        let fingerprint = link.fingerprint();
        let devices: Vec<_> = link
            .devices
            .iter()
            .filter_map(|device| Some((device.public()?, device.name.clone(), device.os.clone())))
            .map(|(key, name, os)| (key, name, os, link.online.contains_key(&key)))
            .collect();
        let header = ui::bar(theme)
            .child(
                ui::button("back", icons::CHEVRON_LEFT, theme)
                    .on_click(cx.listener(|home, _, window, cx| home.back(window, cx))),
            )
            .child(ui::title("settings"));
        let mut body = div()
            .flex()
            .flex_col()
            .child(ui::section("this phone", theme))
            .child(div().px(px(16.0)).child(self.name.clone()))
            .child(ui::hint("Computers show this name for the phone.", theme))
            .child(ui::label_value("fingerprint", div().child(fingerprint), theme))
            .child(ui::section("appearance", theme));
        let mut modes = div().flex().gap(px(4.0)).px(px(16.0)).py(px(8.0));
        for (appearance, label) in APPEARANCES {
            let on = current.appearance == appearance;
            modes = modes.child(
                div()
                    .id(label)
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .h(px(40.0))
                    .border_1()
                    .border_color(if on { theme.accent } else { theme.border })
                    .text_color(if on { theme.on_accent } else { theme.muted })
                    .when(on, |this| this.bg(theme.accent))
                    .child(label)
                    .on_click(cx.listener(move |_, _, _, cx| prefs::update(cx, |prefs| prefs.appearance = appearance))),
            );
        }
        body = body.child(modes).child(ui::section("theme", theme));
        for (id, name) in theme::all(&[]) {
            let on = current.theme == id.as_ref();
            let chosen = id.to_string();
            body = body.child(
                ui::row(SharedString::from(format!("theme:{id}")), theme)
                    .child(ui::choice(on, theme))
                    .child(div().flex_1().child(name))
                    .child(div().text_size(px(ui::SMALL)).text_color(theme.faint).child(if theme::is_automatic(&id) {
                        "light and dark"
                    } else {
                        ""
                    }))
                    .on_click(cx.listener(move |_, _, _, cx| {
                        let chosen = chosen.clone();
                        prefs::update(cx, |prefs| prefs.theme = chosen)
                    })),
            );
        }
        let size = current.font_size;
        let step = |delta: isize| {
            let index = FONT_SIZES.iter().position(|value| *value >= size).unwrap_or(3) as isize;
            FONT_SIZES[(index + delta).clamp(0, FONT_SIZES.len() as isize - 1) as usize]
        };
        let (smaller, bigger) = (step(-1), step(1));
        body = body
            .child(ui::section("terminal", theme))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(16.0))
                    .min_h(px(ui::ROW))
                    .child(div().flex_1().text_color(theme.muted).child("font size"))
                    .child(
                        ui::button("smaller", "[-]", theme)
                            .on_click(cx.listener(move |_, _, _, cx| prefs::update(cx, |prefs| prefs.font_size = smaller))),
                    )
                    .child(div().w(px(32.0)).flex().justify_center().child(format!("{size}")))
                    .child(
                        ui::button("bigger", "[+]", theme)
                            .on_click(cx.listener(move |_, _, _, cx| prefs::update(cx, |prefs| prefs.font_size = bigger))),
                    ),
            )
            .child(ui::hint(
                "Used once the phone types into a terminal. Until then a terminal keeps its computer's size and fits the screen.",
                theme,
            ))
            .child(ui::section("computers", theme));
        for (key, name, os, online) in devices {
            body = body.child(
                ui::row(SharedString::from(format!("settings-device:{name}:{}", key[0])), theme)
                    .child(div().flex_none().w(px(18.0)).text_color(theme.accent).child(icons::os(&os)))
                    .child(div().flex_1().min_w_0().truncate().child(name))
                    .child(
                        div()
                            .text_size(px(ui::SMALL))
                            .text_color(if online { theme.green } else { theme.faint })
                            .child(if online { "online" } else { "offline" }),
                    )
                    .child(div().text_color(theme.faint).child(icons::CHEVRON_RIGHT))
                    .on_click(cx.listener(move |home, _, window, cx| home.go(Screen::Device(key), window, cx))),
            );
        }
        body = body
            .child(
                ui::row("settings-add", theme)
                    .child(div().w(px(18.0)).text_color(theme.accent).child(icons::ADD))
                    .child(div().text_color(theme.accent).child("add a computer"))
                    .on_click(cx.listener(|home, _, window, cx| home.go(Screen::Pair, window, cx))),
            )
            .child(ui::section("about", theme))
            .child(ui::label_value(
                "version",
                div().child(format!("Nucli Uplink {}", env!("CARGO_PKG_VERSION"))),
                theme,
            ))
            .child(div().h(px(24.0)));
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(header)
            .child(ui::scroll("settings", &self.scroll).child(body))
            .into_any_element()
    }
}
