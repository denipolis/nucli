use gpui::prelude::FluentBuilder;
use gpui::{AppContext, Context, IntoElement, ParentElement, Render, Styled, Window, div, px};

use crate::icons;
use crate::settings::{self, FONT};
use crate::ui;

pub struct WorkspaceView {
    title: String,
    session: Option<gpui::Entity<crate::session::Session>>,
}

impl WorkspaceView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        ui::watch_appearance(window, cx);
        cx.observe_global::<settings::Settings>(|_, cx| cx.notify()).detach();
        let spec = crate::session::Spec {
            kind: crate::session::Kind::Shell,
            profile: None,
            shell: None,
            conversation: None,
            resume: false,
            cwd: crate::paths::home(),
            restored: None,
        };
        let size = crate::session::default_size(cx);
        let session =
            crate::session::Session::spawn(spec, size, cx).map_err(|error| eprintln!("spawn: {error:?}")).ok();
        if let Some(session) = &session {
            cx.observe(session, |_, _, cx| cx.notify()).detach();
            window.focus(&session.read(cx).focus.clone());
        }
        WorkspaceView { title: "Personal".into(), session }
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
            .children(self.session.clone())
            .when(self.session.is_none(), |this| this.child(format!("{} new tab", icons::TERMINAL)));
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
