use gpui::{
    AnyElement, Context, Div, InteractiveElement, IntoElement, ParentElement, Stateful, StatefulInteractiveElement,
    Styled, div, px,
};

use crate::agents;
use crate::icons;
use crate::paths;
use crate::session::Session;
use crate::settings::{self, SettingsError};
use crate::theme::Theme;
use crate::ui;
use crate::window::WorkspaceView;

fn chip(id: &'static str, glyph: &'static str, theme: &Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .gap(px(6.0))
        .px(px(6.0))
        .bg(theme.elevated)
        .border_1()
        .border_color(theme.border)
        .text_color(theme.text)
        .child(div().flex_none().text_color(theme.muted).child(glyph))
}

fn program(session: &Session) -> Option<String> {
    let shell = session.shell.as_deref().map(agents::command_name);
    session.foreground.clone().filter(|name| Some(name) != shell.as_ref())
}

pub fn render(view: &WorkspaceView, cx: &mut Context<WorkspaceView>) -> Option<AnyElement> {
    let theme = ui::theme(cx);
    let error = cx
        .try_global::<SettingsError>()
        .and_then(|error| error.0.clone())
        .or_else(|| cx.try_global::<crate::commands::Keys>().and_then(|keys| keys.error.clone()));
    let session = if view.settings.is_none() && !view.file_shown(cx) { view.selected_session(cx) } else { None };
    if session.is_none() && error.is_none() {
        return None;
    }
    let mut row = div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(6.0))
        .px(px(6.0))
        .h(px((settings::get(cx).ui_font_size() * 2.4).round()))
        .bg(theme.background)
        .border_t_1()
        .border_color(theme.border)
        .overflow_hidden();
    if let Some(session) = session {
        let session = session.read(cx);
        let folder = session.working_dir().to_path_buf();
        let (reveal, copy) = (folder.clone(), folder.clone());
        row = row.child(
            chip("status-folder", icons::FOLDER, &theme)
                .flex_shrink()
                .min_w_0()
                .hover(|style| style.bg(theme.hover))
                .child(div().min_w_0().truncate().child(paths::display(&folder)))
                .on_click(cx.listener(move |view, event: &gpui::ClickEvent, _, cx| {
                    let (reveal, copy) = (reveal.clone(), copy.clone());
                    let items = vec![
                        crate::menu::action(crate::actions::reveal_label(), move |_, _, cx| cx.reveal_path(&reveal)),
                        crate::menu::action("Copy Path", move |_, _, cx| {
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(copy.to_string_lossy().into_owned()))
                        }),
                    ];
                    view.open_menu(event.position(), items, cx);
                })),
        );
        if let Some(branch) = session.branch.clone() {
            row = row.child(
                chip("status-branch", icons::BRANCH, &theme)
                    .hover(|style| style.bg(theme.hover))
                    .child(branch)
                    .on_click(cx.listener(|view, _, window, cx| crate::palettes::branches(view, window, cx))),
            );
        }
        let project = view.state(cx).and_then(|state| state.current_project()).map(|project| project.path.clone());
        let stat = project.as_ref().and_then(|path| view.store(cx).diffstats.get(path)).copied();
        if let Some(stat) = stat.filter(|stat| stat.files > 0) {
            row = row.child(
                chip("status-changes", "±", &theme)
                    .hover(|style| style.bg(theme.hover))
                    .child(stat.files.to_string())
                    .child(div().text_color(theme.green).child(format!("+{}", stat.added)))
                    .child(div().text_color(theme.red).child(format!("-{}", stat.removed)))
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(crate::commands::ShowChanges), cx)),
            );
        }
        if let Some(name) = program(session) {
            row = row.child(chip("status-program", icons::PLAY, &theme).child(name));
        }
    }
    if let Some(error) = error {
        row = row.child(div().min_w_0().truncate().text_color(theme.red).child(error));
    }
    Some(row.into_any_element())
}
