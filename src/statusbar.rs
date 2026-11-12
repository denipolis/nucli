use gpui::{
    AnyElement, App, Context, Div, Entity, InteractiveElement, IntoElement, ParentElement, Stateful,
    StatefulInteractiveElement, Styled, div, px,
};

use crate::editor::Editor;
use crate::icons;
use crate::paths;
use crate::session::Session;
use crate::settings::{self, SettingsError, StatusChips};
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

pub fn every_pane(cx: &App) -> bool {
    settings::get(cx).status_chips == StatusChips::EveryPane
}

fn row(theme: &Theme, cx: &App) -> Div {
    let size = (settings::get(cx).ui_font_size() - 2.0).max(8.0);
    div()
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
}

fn file_chips(editor: &Entity<Editor>, theme: &Theme, cx: &App) -> Vec<AnyElement> {
    let file = crate::file_view::file_status(editor, cx);
    let mut chips = Vec::new();
    if let Some(position) = file.position {
        chips.push(frame("status-position", theme).child(position).into_any_element());
    }
    if let Some(language) = file.language {
        let (glyph, color) = icons::file(&file.icon_file);
        chips.push(
            frame("status-language", theme)
                .hover(|style| style.bg(theme.hover))
                .child(div().flex_none().text_color(color.color(theme.dark)).child(glyph))
                .child(language)
                .on_click(|_, window, cx| window.dispatch_action(Box::new(crate::commands::SelectLanguage), cx))
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

fn session_chips(
    view: &WorkspaceView,
    session: &Entity<Session>,
    theme: &Theme,
    cx: &mut Context<WorkspaceView>,
) -> Vec<AnyElement> {
    let session = session.read(cx);
    let folder = session.working_dir().to_path_buf();
    let (reveal, copy) = (folder.clone(), folder.clone());
    let mut chips = vec![
        chip("status-folder", icons::FOLDER, theme)
            .flex_shrink()
            .min_w_0()
            .hover(|style| style.bg(theme.hover))
            .child(div().min_w_0().truncate().child(paths::display(&folder)))
            .on_click(cx.listener(move |view, event: &gpui::ClickEvent, _, cx| {
                let reveal = reveal.clone();
                let items = vec![
                    crate::menu::action(crate::actions::reveal_label(), move |_, _, cx| cx.reveal_path(&reveal)),
                    crate::menu::copy("Copy Path", copy.to_string_lossy()),
                ];
                view.open_menu(event.position(), items, cx);
            }))
            .into_any_element(),
    ];
    if let Some(branch) = session.branch.clone() {
        chips.push(
            chip("status-branch", icons::BRANCH, theme)
                .hover(|style| style.bg(theme.hover))
                .child(branch)
                .on_click(cx.listener(|view, _, window, cx| crate::palettes::branches(view, window, cx)))
                .into_any_element(),
        );
    }
    let project = view.state(cx).and_then(|state| state.project_of(&session.id)).map(|project| project.path.clone());
    let stat = project.as_ref().and_then(|path| view.store(cx).diffstats.get(path)).copied();
    if let Some(stat) = stat.filter(|stat| stat.files > 0) {
        chips.push(
            chip("status-changes", "±", theme)
                .hover(|style| style.bg(theme.hover))
                .child(stat.files.to_string())
                .child(div().text_color(theme.green).child(format!("+{}", stat.added)))
                .child(div().text_color(theme.red).child(format!("-{}", stat.removed)))
                .on_click(|_, window, cx| window.dispatch_action(Box::new(crate::commands::ShowChanges), cx))
                .into_any_element(),
        );
    }
    if let Some(name) = session.program().map(str::to_string) {
        chips.push(chip("status-program", icons::PLAY, theme).child(name).into_any_element());
    }
    chips
}

fn session_theme(session: &Entity<Session>, cx: &App) -> std::rc::Rc<Theme> {
    session.read(cx).view.theme.clone().unwrap_or_else(|| ui::theme(cx))
}

pub fn session_row(view: &WorkspaceView, session: &Entity<Session>, cx: &mut Context<WorkspaceView>) -> AnyElement {
    let theme = session_theme(session, cx);
    let chips = session_chips(view, session, &theme, cx);
    row(&theme, cx).children(chips).into_any_element()
}

pub fn file_row(editor: &Entity<Editor>, cx: &App) -> Option<AnyElement> {
    let theme = ui::theme(cx);
    let chips = file_chips(editor, &theme, cx);
    (!chips.is_empty()).then(|| row(&theme, cx).children(chips).into_any_element())
}

pub fn render(view: &WorkspaceView, cx: &mut Context<WorkspaceView>) -> Option<AnyElement> {
    let error = cx
        .try_global::<SettingsError>()
        .and_then(|error| error.0.clone())
        .or_else(|| cx.try_global::<crate::commands::Keys>().and_then(|keys| keys.error.clone()))
        .or_else(|| cx.try_global::<crate::theme::Themes>().and_then(|themes| themes.error.clone()));
    let focused = !every_pane(cx) && view.settings.is_none();
    let file_shown = focused && view.file_shown(cx);
    let session = if focused && !file_shown { view.selected_session(cx) } else { None };
    let theme = session.as_ref().map(|session| session_theme(session, cx)).unwrap_or_else(|| ui::theme(cx));
    let mut chips = match (&session, file_shown) {
        (Some(session), _) => session_chips(view, session, &theme, cx),
        (None, true) => {
            crate::file_view::active_editor(view, cx).map(|editor| file_chips(&editor, &theme, cx)).unwrap_or_default()
        }
        _ => Vec::new(),
    };
    if let Some(error) = error {
        chips.push(div().min_w_0().truncate().text_color(theme.red).child(error).into_any_element());
    }
    (session.is_some() || !chips.is_empty()).then(|| row(&theme, cx).children(chips).into_any_element())
}
