use gpui::{AnyElement, Context, IntoElement, ParentElement, StatefulInteractiveElement, Styled, div, px};
use nucli_core::logo;
use nucli_core::theme::Theme;

use crate::home::Home;
use crate::ui;

const STEPS: [&str; 3] =
    ["Run Nucli on your computer.", "Turn on Uplink in its settings.", "Pair this phone with its QR code."];

impl Home {
    pub fn welcome(&mut self, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let time = self.shown.elapsed().as_millis();
        let fade = (time.saturating_sub(1100) as f32 / 400.0).min(1.0);
        let mut steps = div().flex().flex_col().gap(px(10.0)).px(px(24.0)).pb(px(24.0));
        for (index, step) in STEPS.iter().enumerate() {
            steps = steps.child(
                div()
                    .flex()
                    .gap(px(12.0))
                    .child(div().text_color(theme.accent).child(format!("{}", index + 1)))
                    .child(div().text_color(theme.muted).child(*step)),
            );
        }
        div()
            .flex_1()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(24.0))
                    .child(logo::pixels(time, 9.0, theme))
                    .child(div().opacity(fade).text_color(theme.muted).child("remote control for your agents")),
            )
            .child(
                div().opacity(fade).flex().flex_col().child(steps).child(
                    div().px(px(16.0)).pb(px(16.0)).child(
                        ui::primary("start", "[get started]", theme)
                            .on_click(cx.listener(|home, _, window, cx| home.start(window, cx))),
                    ),
                ),
            )
            .into_any_element()
    }
}
