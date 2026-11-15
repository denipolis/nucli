use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Context, Focusable, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled,
    div, px,
};
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

    pub fn pair(&mut self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let time = self.shown.elapsed().as_millis();
        let link = self.link.read(cx);
        let (joining, note) = (link.joining, link.note.clone());
        let pairable: Vec<_> = link
            .pairable()
            .into_iter()
            .map(|nearby| {
                (nearby.key, nearby.name.clone(), nearby.addresses.first().map(|address| address.to_string()))
            })
            .collect();
        let target = self.target(cx);
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
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child("Looking for computers on this network that are adding a device…"),
                    ),
            );
        }
        for (key, name, address) in pairable {
            let on = target == Some(key);
            nearby = nearby.child(
                ui::row(
                    gpui::SharedString::from(format!(
                        "nearby:{}",
                        key.iter().take(6).map(|byte| format!("{byte:02x}")).collect::<String>()
                    )),
                    theme,
                )
                .child(ui::choice(on, theme))
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
                .child(div().text_size(px(ui::SMALL)).text_color(theme.green).child("adding a device"))
                .on_click(cx.listener(move |home, _, window, cx| {
                    home.chosen = Some(key);
                    window.focus(&home.code.focus_handle(cx), cx);
                    cx.notify();
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
            .child(ui::section("on your computer", theme))
            .child(
                div()
                    .px(px(16.0))
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_color(theme.muted)
                            .child("Open Nucli › Settings › Uplink, turn Uplink on and choose add device."),
                    )
                    .child(
                        div()
                            .text_size(px(ui::SMALL))
                            .text_color(theme.faint)
                            .child("It shows a QR code and a code. Both work for 5 minutes."),
                    ),
            )
            .child(ui::section("on this phone", theme))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .px(px(16.0))
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
            .child(ui::section("or pick it and type its code", theme))
            .child(nearby)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .px(px(16.0))
                    .pt(px(10.0))
                    .child(self.code.clone())
                    .when(target.is_none(), |this| this.child(self.address.clone()))
                    .child(ui::primary("pair-code", "[pair]", theme).on_click(cx.listener(|home, _, window, cx| {
                        window.blur(cx);
                        home.pair_with_code(cx);
                    }))),
            )
            .child(
                div()
                    .px(px(16.0))
                    .py(px(16.0))
                    .when(joining, |this| {
                        this.child(
                            div()
                                .flex()
                                .gap(px(8.0))
                                .text_color(theme.accent)
                                .child(ui::spinner(time))
                                .child("pairing…"),
                        )
                    })
                    .when_some(note.filter(|_| !joining), |this, note| {
                        let color = if note.starts_with("Paired") { theme.green } else { theme.red };
                        this.child(div().text_color(color).child(note))
                    }),
            );
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(header)
            .child(div().id("pair").flex_1().min_h_0().overflow_y_scroll().child(body))
            .into_any_element()
    }
}
