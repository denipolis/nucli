use gpui::prelude::FluentBuilder;
use gpui::{AnyElement, Context, IntoElement, ParentElement, StatefulInteractiveElement, Styled, div, px};
use nucli_core::icons;
use nucli_core::theme::Theme;

use crate::home::Home;
use crate::{scanner, ui};

impl Home {
    fn scan(&mut self, cx: &mut Context<Self>) {
        let (sender, receiver) = futures::channel::oneshot::channel::<String>();
        match scanner::scan(sender) {
            Ok(()) => {
                self.scanning = None;
                cx.spawn(async move |this, cx| {
                    if let Ok(text) = receiver.await {
                        let _ = this.update(cx, |home, cx| home.pair_with_link(&text, cx));
                    }
                })
                .detach();
            }
            Err(error) => self.scanning = Some(error),
        }
        cx.notify();
    }

    fn status(&self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let time = self.shown.elapsed().as_millis();
        let link = self.link.read(cx);
        let mut status = div().flex().flex_col().gap(px(10.0)).px(px(16.0)).py(px(16.0));
        if link.joining {
            let waiting = match &link.check {
                Some((name, sas)) => div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(div().text_color(theme.muted).child(format!("Allow this phone on {name}. Both show:")))
                    .child(div().text_size(px(36.0)).line_height(px(46.0)).text_color(theme.accent).child(sas.clone()))
                    .child(
                        div()
                            .text_size(px(ui::SMALL))
                            .text_color(theme.faint)
                            .child("If the numbers are not the same, cancel."),
                    ),
                None => {
                    div().flex().gap(px(8.0)).text_color(theme.accent).child(ui::spinner(time)).child("connecting…")
                }
            };
            status = status.child(waiting).child(
                ui::secondary("pair-cancel", "[cancel]", theme)
                    .on_click(cx.listener(|home, _, _, cx| home.link.update(cx, |link, cx| link.cancel(cx)))),
            );
        } else if let Some(note) = link.note.clone() {
            let color = if note.starts_with("Paired") { theme.green } else { theme.red };
            status = status.child(div().text_color(color).child(note));
        }
        status.into_any_element()
    }

    pub fn pair(&mut self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let time = self.shown.elapsed().as_millis();
        let link = self.link.read(cx);
        let joining = link.joining;
        let pairable: Vec<_> = link
            .pairable()
            .into_iter()
            .map(|nearby| {
                (nearby.key, nearby.name.clone(), nearby.addresses.first().map(|address| address.to_string()))
            })
            .collect();
        let mut nearby = div().flex().flex_col();
        if pairable.is_empty() {
            nearby = nearby.child(
                div()
                    .flex()
                    .gap(px(10.0))
                    .px(px(16.0))
                    .py(px(8.0))
                    .text_size(px(ui::SMALL))
                    .text_color(theme.faint)
                    .child(div().text_color(theme.accent).child(ui::spinner(time)))
                    .child(div().flex_1().min_w_0().child("Looking for computers on this network with Uplink on…")),
            );
        }
        for (key, name, address) in pairable {
            nearby = nearby.child(
                ui::row(
                    gpui::SharedString::from(format!(
                        "nearby:{}",
                        key.iter().take(6).map(|byte| format!("{byte:02x}")).collect::<String>()
                    )),
                    theme,
                )
                .child(div().flex_none().text_color(theme.accent).child(icons::DEVICE))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .child(div().truncate().child(if name.is_empty() { "computer".to_string() } else { name }))
                        .when_some(address, |this, address| {
                            this.child(div().truncate().text_size(px(ui::SMALL)).text_color(theme.faint).child(address))
                        }),
                )
                .child(div().text_size(px(ui::SMALL)).text_color(theme.accent).child("[connect]"))
                .on_click(cx.listener(move |home, _, window, cx| {
                    window.blur(cx);
                    home.link.update(cx, |link, cx| link.connect(key, cx));
                })),
            );
        }
        let header = ui::bar(theme)
            .child(
                ui::button("back", icons::CHEVRON_LEFT, theme)
                    .on_click(cx.listener(|home, _, window, cx| home.back(window, cx))),
            )
            .child(ui::title("add a computer"));
        let body = div()
            .flex()
            .flex_col()
            .child(ui::section("computers nearby", theme))
            .child(nearby)
            .child(
                div()
                    .px(px(16.0))
                    .pt(px(6.0))
                    .text_size(px(ui::SMALL))
                    .text_color(theme.faint)
                    .child("Tap one, then allow this phone on it. Uplink must be on there."),
            )
            .child(ui::section("or scan its code", theme))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .px(px(16.0))
                    .pb(px(10.0))
                    .child(
                        div()
                            .text_color(theme.muted)
                            .child("On the computer choose connect device in Nucli's Uplink sidebar or settings."),
                    )
                    .child(
                        ui::primary("scan", "[scan qr code]", theme)
                            .on_click(cx.listener(|home, _, _, cx| home.scan(cx))),
                    )
                    .child(ui::secondary("paste", "[paste pairing link]", theme).on_click(cx.listener(
                        |home, _, _, cx| {
                            let text = cx.read_from_clipboard().and_then(|item| item.text()).unwrap_or_default();
                            home.pair_with_link(&text, cx);
                        },
                    )))
                    .when_some(self.scanning.clone(), |this, error| {
                        this.child(div().text_size(px(ui::SMALL)).text_color(theme.red).child(error))
                    }),
            )
            .child(ui::section("not listed", theme))
            .child(div().flex().flex_col().gap(px(10.0)).px(px(16.0)).child(self.address.clone()).child(
                ui::secondary("connect", "[connect]", theme).on_click(cx.listener(|home, _, window, cx| {
                    window.blur(cx);
                    home.connect_address(cx);
                })),
            ))
            .when(!joining, |this| this.child(self.status(theme, cx)));
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(header)
            .when(joining, |this| this.child(self.status(theme, cx)))
            .child(ui::scroll("pair", &self.scroll).child(body))
            .into_any_element()
    }
}
