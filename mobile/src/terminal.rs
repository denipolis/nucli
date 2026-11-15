use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Context, Entity, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, Window, div, px,
};
use nucli_core::icons;
use nucli_core::theme::Theme;

use crate::board::{glyph, project_name, status};
use crate::home::Home;
use crate::mirror::Mirror;
use crate::ui;

const KEYS: [(&str, &str); 7] =
    [("escape", "esc"), ("tab", "tab"), ("up", "↑"), ("down", "↓"), ("left", "←"), ("right", "→"), ("enter", "⏎")];

impl Home {
    pub fn terminal(
        &mut self,
        mirror: Entity<Mirror>,
        theme: &Theme,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let time = self.shown.elapsed().as_millis();
        let (key, tab, ctrl, focused) = {
            let mirror = mirror.read(cx);
            (mirror.key, mirror.tab.clone(), mirror.ctrl, mirror.focused(window))
        };
        let link = self.link.read(cx);
        let device = link.device(&key).map(|device| device.name.clone()).unwrap_or_default();
        let online = link.online.get(&key);
        let remote = online.and_then(|online| online.catalog.tabs.iter().find(|item| item.id == tab)).cloned();
        let project = online.zip(remote.as_ref()).map(|(online, tab)| project_name(&online.catalog, &tab.project));
        let control = online.is_some_and(|online| online.catalog.control);
        let (state, color) = if online.is_none() {
            (SharedString::from("offline"), theme.yellow)
        } else if !link.is_watched(key, &tab) {
            ("closed".into(), theme.yellow)
        } else if !control {
            ("view only".into(), theme.faint)
        } else {
            remote.as_ref().map_or(("".into(), theme.faint), |remote| status(remote, time, theme))
        };
        let (glyph, glyph_color) = remote.as_ref().map_or((icons::TERMINAL, None), glyph);
        let detail = match project.filter(|project| !project.is_empty()) {
            Some(project) => format!("{device} · {project}"),
            None => device,
        };
        let header = ui::bar(theme)
            .child(
                ui::button("back", icons::CHEVRON_LEFT, theme)
                    .on_click(cx.listener(|home, _, window, cx| home.back(window, cx))),
            )
            .child(
                div()
                    .flex_none()
                    .text_color(glyph_color.map_or(theme.muted, |color| color.color(theme.dark)))
                    .child(glyph),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .px(px(6.0))
                    .child(
                        div().truncate().child(remote.as_ref().map(|remote| remote.title.clone()).unwrap_or_default()),
                    )
                    .child(div().truncate().text_size(px(ui::SMALL)).text_color(theme.faint).child(detail)),
            )
            .child(div().flex_none().pr(px(10.0)).text_size(px(ui::SMALL)).text_color(color).child(state));
        let mut keys = div()
            .id("keys")
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .px(px(4.0))
            .overflow_x_scroll()
            .border_t_1()
            .border_color(theme.border)
            .bg(theme.surface);
        let on_mirror = |action: fn(&mut Mirror, &mut Window, &mut Context<Mirror>)| {
            cx.listener(move |home: &mut Home, _, window, cx| {
                if let Some(mirror) = &home.mirror {
                    mirror.update(cx, |mirror, cx| action(mirror, window, cx));
                }
            })
        };
        keys = keys.child(
            ui::key("ctrl", "ctrl", theme)
                .when(ctrl, |button| button.bg(theme.accent).text_color(theme.on_accent))
                .on_click(on_mirror(|mirror, _, cx| {
                    mirror.ctrl = !mirror.ctrl;
                    cx.notify();
                })),
        );
        for (name, label) in KEYS {
            keys = keys.child(ui::key(name, label, theme).on_click(cx.listener(move |home, _, _, cx| {
                if let Some(mirror) = &home.mirror {
                    mirror.update(cx, |mirror, cx| mirror.press(name, cx));
                }
            })));
        }
        keys = keys
            .child(ui::key("paste", icons::PASTE, theme).on_click(on_mirror(|mirror, _, cx| mirror.paste(cx))))
            .child(
                ui::key("keyboard", icons::KEYBOARD, theme)
                    .when(focused, |button| button.bg(theme.accent).text_color(theme.on_accent))
                    .on_click(on_mirror(|mirror, window, cx| {
                        mirror.toggle_keyboard(window, cx);
                        cx.notify();
                    })),
            );
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(header)
            .child(div().flex_1().min_h_0().child(mirror))
            .when(control && online.is_some(), |this| this.child(keys))
            .into_any_element()
    }
}
