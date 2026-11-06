use gpui::{
    AnyElement, App, Context, Div, InteractiveElement, IntoElement, ParentElement, Stateful,
    StatefulInteractiveElement, Styled, div, px,
};

use crate::icons;
use crate::paths;
use crate::settings::{self, SettingsError};
use crate::theme::Theme;
use crate::ui;
use crate::window::WorkspaceView;

fn frame(id: &'static str, theme: &Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .gap(px(4.0))
        .px(px(4.0))
        .bg(theme.elevated)
        .border_1()
        .border_color(theme.border)
        .text_color(theme.text)
}

fn chip(id: &'static str, glyph: &'static str, theme: &Theme) -> Stateful<Div> {
    frame(id, theme).child(div().flex_none().text_color(theme.muted).child(glyph))
}

fn file_chips(view: &WorkspaceView, theme: &Theme, cx: &App) -> Vec<AnyElement> {
    let Some(file) = crate::file_view::file_status(view, cx) else { return Vec::new() };
    let mut chips = Vec::new();
    if let Some(position) = file.position {
        chips.push(frame("status-position", theme).child(position).into_any_element());
    }
    if let Some(language) = file.language {
        let (glyph, color) = icons::file(&file.name);
        chips.push(
            frame("status-language", theme)
                .child(div().flex_none().text_color(color.color(theme.dark)).child(glyph))
                .child(language)
                .into_any_element(),
        );
    }
    if file.added + file.removed > 0 {
        chips.push(
            frame("status-file-changes", theme)
                .child(div().text_color(theme.green).child(format!("+{}", file.added)))
                .child(div().text_color(theme.red).child(format!("-{}", file.removed)))
                .into_any_element(),
        );
    }
    if let Some(commit) = file.commit {
        chips.push(
            chip("status-commit", icons::COMMIT, theme)
                .child(commit[..commit.len().min(7)].to_string())
                .into_any_element(),
        );
    }
    chips
}

pub fn render(view: &WorkspaceView, cx: &mut Context<WorkspaceView>) -> Option<AnyElement> {
    let error = cx
        .try_global::<SettingsError>()
        .and_then(|error| error.0.clone())
        .or_else(|| cx.try_global::<crate::commands::Keys>().and_then(|keys| keys.error.clone()));
    let file_shown = view.settings.is_none() && view.file_shown(cx);
    let session = if view.settings.is_none() && !file_shown { view.selected_session(cx) } else { None };
    let theme =
        session.as_ref().and_then(|session| session.read(cx).view.theme.clone()).unwrap_or_else(|| ui::theme(cx));
    let chips = if file_shown { file_chips(view, &theme, cx) } else { Vec::new() };
    if session.is_none() && chips.is_empty() && error.is_none() {
        return None;
    }
    let size = (settings::get(cx).ui_font_size() - 2.0).max(8.0);
    let mut row = div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(4.0))
        .pl(px(10.0))
        .pr(px(10.0))
        .pb(px(4.0))
        .text_size(px(size))
        .line_height(px((size * 1.5).round()))
        .bg(theme.background)
        .overflow_hidden()
        .children(chips);
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
        if let Some(name) = session.program().map(str::to_string) {
            row = row.child(chip("status-program", icons::PLAY, &theme).child(name));
        }
    }
    if let Some(error) = error {
        row = row.child(div().min_w_0().truncate().text_color(theme.red).child(error));
    }
    Some(row.into_any_element())
}
