use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement, SharedString, StatefulInteractiveElement,
    Styled, div, px,
};
use nucli_core::icons;
use nucli_core::theme::Theme;
use nucli_core::uplink::wire::Key;

use crate::board::project_name;
use crate::home::Home;
use crate::ui;

impl Home {
    pub fn launch(&mut self, key: Key, project: &str, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let time = self.shown.elapsed().as_millis();
        let link = self.link.read(cx);
        let device = link.device(&key).map(|device| device.name.clone()).unwrap_or_default();
        let catalog = link.online.get(&key).map(|online| online.catalog.clone()).unwrap_or_default();
        let name = project_name(&catalog, project);
        let header = ui::bar(theme)
            .child(
                ui::button("back", icons::CHEVRON_LEFT, theme)
                    .on_click(cx.listener(|home, _, window, cx| home.back(window, cx))),
            )
            .child(ui::title(format!("new tab in {name}")));
        let mut body = div()
            .flex()
            .flex_col()
            .child(
                div()
                    .px(px(16.0))
                    .pt(px(12.0))
                    .text_color(theme.muted)
                    .child(format!("Starts on {device}, in {name}.")),
            )
            .child(ui::hint(project.to_string(), theme).text_color(theme.faint));
        if !link.online.contains_key(&key) {
            body = body.child(ui::hint("The computer is offline.", theme));
        }
        for (agents, title) in [(true, "agents"), (false, "shells")] {
            let launchers: Vec<_> = catalog.launchers.iter().filter(|launcher| launcher.agent == agents).collect();
            if launchers.is_empty() {
                continue;
            }
            body = body.child(ui::section(title, theme));
            for launcher in launchers {
                let (glyph, color) = if agents {
                    let (glyph, color) = icons::agent(&launcher.command);
                    (glyph, color.color(theme.dark))
                } else {
                    (icons::TERMINAL, theme.muted)
                };
                let (project, launcher_name) = (project.to_string(), launcher.name.clone());
                body = body.child(
                    ui::row(SharedString::from(format!("launcher:{}", launcher.name)), theme)
                        .child(div().flex_none().w(px(18.0)).text_color(color).child(glyph))
                        .child(div().flex_1().min_w_0().truncate().child(launcher.name.clone()))
                        .child(
                            div()
                                .text_size(px(ui::SMALL))
                                .text_color(theme.faint)
                                .truncate()
                                .child(launcher.command.clone()),
                        )
                        .on_click(cx.listener(move |home, _, _, cx| {
                            home.shown = std::time::Instant::now();
                            home.link.update(cx, |link, _| link.open_tab(key, &project, &launcher_name));
                            cx.notify();
                        })),
                );
            }
        }
        let starting = time < 5000 && self.link.read(cx).is_opening(key);
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(header)
            .child(div().id("launch").flex_1().min_h_0().overflow_y_scroll().child(body))
            .when(starting, |this| {
                this.child(
                    div()
                        .flex()
                        .gap(px(8.0))
                        .px(px(16.0))
                        .py(px(12.0))
                        .border_t_1()
                        .border_color(theme.border)
                        .text_color(theme.accent)
                        .child(ui::spinner(time))
                        .child("starting…"),
                )
            })
            .into_any_element()
    }
}
