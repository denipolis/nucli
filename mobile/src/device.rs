use std::time::{SystemTime, UNIX_EPOCH};

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled, div, px,
};
use nucli_core::icons;
use nucli_core::theme::Theme;
use nucli_core::uplink::wire::{self, Key};

use crate::home::Home;
use crate::ui;

pub fn age(seconds: u64) -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs());
    let elapsed = now.saturating_sub(seconds);
    match elapsed {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", elapsed / 60),
        3600..86400 => format!("{} h ago", elapsed / 3600),
        _ => format!("{} days ago", elapsed / 86400),
    }
}

fn system(os: &str) -> &str {
    match os {
        "macos" => "macOS",
        "linux" => "Linux",
        "windows" => "Windows",
        "ios" => "iOS",
        "android" => "Android",
        "" => "unknown",
        other => other,
    }
}

impl Home {
    pub fn device(&mut self, key: Key, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let link = self.link.read(cx);
        let Some(device) = link.device(&key).cloned() else {
            return div().into_any_element();
        };
        let address = link.address(&key);
        let (state, color) = match &address {
            Some(address) => (format!("online · {address}"), theme.green),
            None => {
                (format!("{} · seen {}", link.device_error(&key).unwrap_or("offline"), age(device.seen)), theme.faint)
            }
        };
        let online = address.is_some();
        let tabs = link.online.get(&key).map_or(0, |online| online.catalog.tabs.len());
        let forgetting = self.forgetting == Some(key);
        let header = ui::bar(theme)
            .child(
                ui::button("back", icons::CHEVRON_LEFT, theme)
                    .on_click(cx.listener(|home, _, window, cx| home.back(window, cx))),
            )
            .child(div().flex_none().text_color(theme.accent).child(icons::os(&device.os)))
            .child(ui::title(device.name.clone()));
        let body = div()
            .flex()
            .flex_col()
            .child(ui::section("status", theme))
            .child(ui::label_value("state", div().text_color(color).child(state), theme))
            .child(ui::label_value("shared tabs", div().child(format!("{tabs}")), theme))
            .child(ui::label_value("system", div().child(system(&device.os).to_string()), theme))
            .child(ui::label_value("fingerprint", div().child(wire::fingerprint(&key)), theme))
            .child(ui::label_value("paired", div().child(age(device.paired)), theme))
            .child(ui::section("also reach it at", theme))
            .child(div().px(px(16.0)).child(self.reach.clone()))
            .child(ui::hint(
                "For networks where the phone does not find it by itself, such as a VPN. Nucli also remembers the addresses that worked.",
                theme,
            ))
            .when(!device.addresses.is_empty(), |this| {
                this.child(ui::hint(format!("worked before: {}", device.addresses.join(", ")), theme))
            })
            .child(ui::section("this computer", theme))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .px(px(16.0))
                    .pb(px(24.0))
                    .when(online, |this| {
                        this.child(ui::secondary("disconnect", "[disconnect]", theme).on_click(cx.listener(
                            move |home, _, _, cx| home.link.update(cx, |link, cx| link.disconnect(&key, cx)),
                        )))
                    })
                    .child(
                        div()
                            .id("forget")
                            .flex()
                            .items_center()
                            .justify_center()
                            .h(px(ui::ROW))
                            .border_1()
                            .border_color(theme.red)
                            .text_color(if forgetting { theme.background } else { theme.red })
                            .when(forgetting, |this| this.bg(theme.red))
                            .child(if forgetting { "[tap again to forget]" } else { "[forget]" })
                            .on_click(cx.listener(move |home, _, window, cx| {
                                if home.forgetting == Some(key) {
                                    home.link.update(cx, |link, cx| link.forget(&key, cx));
                                    home.back(window, cx);
                                } else {
                                    home.forgetting = Some(key);
                                    cx.notify();
                                }
                            })),
                    )
                    .child(div().text_size(px(ui::SMALL)).text_color(theme.faint).child(
                        "Forgetting removes the pairing on both sides when the computer is online. Pair again to reconnect.",
                    )),
            );
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(header)
            .child(div().id("device").flex_1().min_h_0().overflow_y_scroll().child(body))
            .into_any_element()
    }
}
