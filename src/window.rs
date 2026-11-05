use gpui::{Context, IntoElement, ParentElement, Render, Styled, Window, div, px};

use crate::icons;
use crate::settings::{self, FONT};
use crate::ui;

pub struct WorkspaceView {
    title: String,
}

impl WorkspaceView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        ui::watch_appearance(window, cx);
        cx.observe_global::<settings::Settings>(|_, cx| cx.notify()).detach();
        WorkspaceView { title: "Personal".into() }
    }
}

impl Render for WorkspaceView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = ui::theme(cx);
        let ui_size = settings::get(cx).ui_font_size();
        let row = px((ui_size * 2.0).round());
        let titlebar = div()
            .flex()
            .flex_row()
            .items_center()
            .h(row + px(6.0))
            .pl(px(if cfg!(target_os = "macos") { 76.0 } else { 8.0 }))
            .border_b_1()
            .border_color(theme.border)
            .bg(theme.surface)
            .child(div().px(px(6.0)).bg(theme.accent).text_color(theme.on_accent).child(ui::caps(&self.title)))
            .child(div().px(px(8.0)).text_color(theme.muted).child("no project"));
        let sidebar = div().w(px(260.0)).h_full().border_r_1().border_color(theme.border).bg(theme.surface).child(
            div()
                .px(px(8.0))
                .h(row)
                .flex()
                .items_center()
                .gap(px(16.0))
                .child(div().px(px(4.0)).bg(theme.accent).text_color(theme.on_accent).child("PROJECTS"))
                .child(div().text_color(theme.muted).child("AGENTS"))
                .child(div().text_color(theme.muted).child("HISTORY")),
        );
        let viewport = div()
            .flex_1()
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(theme.faint)
            .child(format!("{} new tab", icons::TERMINAL));
        let statusbar = div()
            .flex()
            .flex_row()
            .items_center()
            .h(row)
            .px(px(0.0))
            .border_t_1()
            .border_color(theme.border)
            .bg(theme.surface)
            .child(div().px(px(6.0)).bg(theme.accent).text_color(theme.on_accent).child("NUCLI"))
            .child(div().px(px(8.0)).text_color(theme.muted).child(format!("{} main", icons::BRANCH)));
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_color(theme.text)
            .font_family(FONT)
            .text_size(px(ui_size))
            .child(titlebar)
            .child(div().flex_1().flex().flex_row().overflow_hidden().child(sidebar).child(viewport))
            .child(statusbar)
    }
}
