use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled, div, px,
};

use crate::icons;
use crate::paths;
use crate::persist::Selected;
use crate::session::Kind;
use crate::settings::SettingsError;
use crate::ui;
use crate::window::WorkspaceView;

pub fn render(view: &mut WorkspaceView, cx: &mut Context<WorkspaceView>) -> AnyElement {
    let theme = ui::theme(cx);
    let state = view.state(cx).cloned();
    let store = view.store(cx);
    let session = view.selected_session(cx);
    let file = view.file_shown(cx);
    let (mode, folder, branch, program) =
        match (&session, file, state.as_ref().and_then(|state| state.selected.clone())) {
            (_, true, _) => ("FILE", None, None, None),
            (Some(session), _, _) => {
                let session = session.read(cx);
                let mode =
                    if session.kind == Kind::Agent || session.running_agent.is_some() { "AGENT" } else { "SHELL" };
                (mode, Some(session.working_dir().to_path_buf()), session.branch.clone(), session.foreground.clone())
            }
            (None, _, Some(Selected::Project(path))) => (
                "PROJECT",
                (!path.as_os_str().is_empty()).then(|| path.clone()),
                crate::git::current_branch(&path),
                None,
            ),
            _ => ("NUCLI", None, None, None),
        };
    let project = state.as_ref().and_then(|state| state.current_project()).map(|project| project.path.clone());
    let stat = project.as_ref().and_then(|path| store.diffstats.get(path)).copied();
    let attention = store.sessions.values().filter(|session| session.read(cx).attention).count();
    let error = cx
        .try_global::<SettingsError>()
        .and_then(|error| error.0.clone())
        .or_else(|| cx.try_global::<crate::commands::Keys>().and_then(|keys| keys.error.clone()));
    let mut left = div()
        .flex()
        .items_center()
        .gap(px(12.0))
        .min_w_0()
        .child(div().px(px(6.0)).bg(theme.accent).text_color(theme.on_accent).child(mode));
    if let Some(folder) = folder {
        let (reveal, copy) = (folder.clone(), folder.clone());
        left = left.child(
            div()
                .id("status-folder")
                .flex_none()
                .text_color(theme.muted)
                .hover(|style| style.text_color(theme.text))
                .child(format!("{} {}", icons::FOLDER, paths::display(&folder)))
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
    }
    if let Some(branch) = branch {
        left = left.child(
            div()
                .id("status-branch")
                .flex_none()
                .text_color(theme.muted)
                .hover(|style| style.text_color(theme.text))
                .child(format!("{} {branch}", icons::BRANCH))
                .on_click(cx.listener(|view, _, window, cx| crate::palettes::branches(view, window, cx))),
        );
    }
    if let Some(program) = program {
        left = left.child(div().flex_none().text_color(theme.faint).child(format!("{} {program}", icons::TERMINAL)));
    }
    if let Some(error) = error {
        left = left.child(div().truncate().text_color(theme.red).child(error));
    }
    let mut right = div().flex().items_center().gap(px(12.0)).flex_none();
    if let Some(position) = crate::file_view::cursor_label(view, cx) {
        right = right.child(div().text_color(theme.muted).child(position));
    }
    if let Some(stat) = stat.filter(|stat| stat.files > 0) {
        right = right.child(
            div()
                .id("status-changes")
                .flex()
                .gap(px(6.0))
                .hover(|style| style.bg(theme.hover))
                .child(div().text_color(theme.muted).child(format!(
                    "{} {}",
                    stat.files,
                    if stat.files == 1 { "file" } else { "files" }
                )))
                .child(div().text_color(theme.green).child(format!("+{}", stat.added)))
                .child(div().text_color(theme.red).child(format!("-{}", stat.removed)))
                .on_click(|_, window, cx| window.dispatch_action(Box::new(crate::commands::ShowChanges), cx)),
        );
    }
    if attention > 0 {
        right = right.child(
            div().px(px(6.0)).bg(theme.green).text_color(theme.on_accent).child(format!("{} {attention}", icons::BELL)),
        );
    }
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .pr(px(8.0))
        .h(px((crate::settings::get(cx).ui_font_size() * 1.9).round()))
        .bg(theme.surface)
        .border_t_1()
        .border_color(theme.border)
        .child(left)
        .child(right)
        .into_any_element()
}
