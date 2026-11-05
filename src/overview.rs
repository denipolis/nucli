use std::path::Path;

use gpui::{
    AnyElement, Context, Hsla, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, div, px,
};

use crate::commands;
use crate::history::relative_age;
use crate::icons;
use crate::palette::Run;
use crate::paths;
use crate::settings;
use crate::store::Launcher;
use crate::ui;
use crate::window::WorkspaceView;

fn section(title: &str, cx: &Context<WorkspaceView>) -> AnyElement {
    let theme = ui::theme(cx);
    div()
        .mt(px(18.0))
        .mb(px(4.0))
        .flex()
        .gap(px(8.0))
        .items_center()
        .text_color(theme.faint)
        .child(ui::caps(title))
        .child(div().flex_1().h(px(1.0)).bg(theme.border))
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
fn row(
    id: SharedString,
    glyph: &'static str,
    color: Hsla,
    title: impl Into<SharedString>,
    subtitle: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    run: Run,
    cx: &mut Context<WorkspaceView>,
) -> AnyElement {
    let theme = ui::theme(cx);
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(10.0))
        .px(px(8.0))
        .py(px(3.0))
        .hover(|style| style.bg(theme.hover))
        .child(div().w(px(16.0)).text_color(color).child(glyph))
        .child(div().flex_none().max_w(px(380.0)).truncate().child(title.into()))
        .child(div().flex_1().min_w_0().truncate().text_color(theme.faint).child(subtitle.into()))
        .child(div().text_color(theme.faint).child(detail.into()))
        .on_click(cx.listener(move |view, _, window, cx| run(view, window, cx)))
        .into_any_element()
}

pub fn project(view: &mut WorkspaceView, path: &Path, cx: &mut Context<WorkspaceView>) -> AnyElement {
    let theme = ui::theme(cx);
    let Some(project) = view.state(cx).and_then(|state| state.project(path)).cloned() else {
        return empty(view, cx);
    };
    let store = view.store(cx);
    let tick = store.tick_fast;
    let last_launcher = store.last_launcher.clone();
    let shell = store.terminal_shell(cx);
    let conversations = store.conversations_for(&project.path).cloned();
    let tab_infos: Vec<crate::sidebar::TabInfo> = project
        .tabs
        .iter()
        .filter_map(|id| store.session(id))
        .map(|session| crate::sidebar::TabInfo::new(session.read(cx), &theme, tick))
        .collect();
    let folders: Vec<String> = project
        .tabs
        .iter()
        .filter_map(|id| store.session(id))
        .map(|session| {
            let session = session.read(cx);
            if session.is_running() { paths::display(session.working_dir()) } else { "exited".into() }
        })
        .collect();
    let open: Vec<String> = project
        .tabs
        .iter()
        .filter_map(|id| store.session(id))
        .filter_map(|session| session.read(cx).conversation.clone())
        .collect();
    let settings = settings::get(cx).clone();
    let tint = project.color.map(|color| color.color(theme.dark)).unwrap_or(theme.accent);
    let branch = crate::git::current_branch(&project.path)
        .map(|branch| format!("  {} {branch}", icons::BRANCH))
        .unwrap_or_default();
    let last = project.last_launcher.clone().or(last_launcher);
    let mut content =
        div().w(px(720.0)).max_w_full().flex().flex_col().pt(px(40.0)).pb(px(40.0)).child(
            div()
                .flex()
                .gap(px(12.0))
                .items_center()
                .child(div().text_size(px(26.0)).text_color(tint).child(icons::FOLDER))
                .child(
                    div().flex().flex_col().child(div().text_size(px(20.0)).child(project.display_name())).child(
                        div().text_color(theme.muted).child(format!("{}{branch}", paths::display(&project.path))),
                    ),
                ),
        );
    content = content.child(section("new", cx));
    let mut agents: Vec<_> = settings.agents.iter().filter(|profile| profile.is_launchable()).cloned().collect();
    agents.sort_by_key(|profile| Some(&profile.name) != last.as_ref());
    let picker = commands::key_for("new_tab_picker", cx);
    for (index, profile) in agents.into_iter().enumerate() {
        let detail = if index == 0 && !picker.is_empty() { format!("[{picker} enter]") } else { String::new() };
        let target = project.path.clone();
        let summary = profile.summary();
        let title = format!("new {} session", profile.name);
        let run: Run = std::rc::Rc::new(move |view, window, cx| {
            crate::actions::launch(view, &target, Launcher::Agent(profile.clone()), window, cx);
        });
        content = content.child(row(
            format!("new-agent-{index}").into(),
            icons::AGENT,
            theme.accent,
            title,
            summary,
            detail,
            run,
            cx,
        ));
    }
    for (index, shell) in settings.shells.iter().filter(|shell| shell.is_launchable()).cloned().enumerate() {
        let target = project.path.clone();
        let title = format!("new {} tab", shell.name);
        let path = shell.path.clone();
        let run: Run = std::rc::Rc::new(move |view, window, cx| {
            crate::actions::launch(view, &target, Launcher::Shell(shell.clone()), window, cx);
        });
        content = content.child(row(
            format!("new-shell-{index}").into(),
            icons::TERMINAL,
            theme.muted,
            title,
            path,
            "",
            run,
            cx,
        ));
    }
    let terminal_target = project.path.clone();
    content = content.child(row(
        "new-terminal".into(),
        icons::PANEL,
        theme.muted,
        "new terminal in the panel",
        shell,
        commands::key_for("new_terminal", cx),
        std::rc::Rc::new(move |view, window, cx| crate::actions::new_terminal_in(view, &terminal_target, window, cx)),
        cx,
    ));
    if !tab_infos.is_empty() {
        content = content.child(section("tabs", cx));
        for (info, folder) in tab_infos.into_iter().zip(folders) {
            let target = info.id.clone();
            let status = info.status.clone().map(|(glyph, _)| glyph).unwrap_or_default();
            content = content.child(row(
                SharedString::from(format!("open-{}", info.id)),
                info.glyph,
                theme.muted,
                info.title.clone(),
                folder,
                status,
                std::rc::Rc::new(move |view, window, cx| view.select_tab(&target, window, cx)),
                cx,
            ));
        }
    }
    content = content.child(section("conversations", cx));
    let loaded = conversations.is_some();
    match conversations {
        None => content = content.child(div().px(px(8.0)).text_color(theme.faint).child("loading…")),
        Some(list) if list.is_empty() => {
            content =
                content.child(div().px(px(8.0)).text_color(theme.faint).child("no conversations in this folder yet"))
        }
        Some(list) => {
            for (index, conversation) in list.into_iter().take(12).enumerate() {
                let detail = if open.contains(&conversation.id) { "open" } else { "" };
                let subtitle = format!("{} · {}", conversation.profile, relative_age(conversation.modified));
                let target = project.path.clone();
                let title = conversation.title.clone();
                content = content.child(row(
                    format!("conversation-{index}").into(),
                    icons::HISTORY,
                    theme.muted,
                    title,
                    subtitle,
                    detail,
                    std::rc::Rc::new(move |view, window, cx| {
                        crate::actions::launch(view, &target, Launcher::Resume(conversation.clone()), window, cx)
                    }),
                    cx,
                ));
            }
        }
    }
    if !project.is_scratch() {
        content = content.child(section("folder", cx));
        let target = project.path.clone();
        content = content.child(row(
            "reveal".into(),
            icons::REVEAL,
            theme.muted,
            crate::actions::reveal_label(),
            "",
            "",
            std::rc::Rc::new(move |_, _, cx| cx.reveal_path(&target)),
            cx,
        ));
    }
    if !loaded {
        let path = project.path.clone();
        cx.defer(move |cx| crate::store::store(cx).update(cx, |store, cx| store.refresh_conversations(path, cx)));
    }
    div()
        .id("overview")
        .size_full()
        .overflow_y_scroll()
        .flex()
        .justify_center()
        .px(px(24.0))
        .child(content)
        .into_any_element()
}

pub fn empty(_view: &mut WorkspaceView, cx: &mut Context<WorkspaceView>) -> AnyElement {
    let theme = ui::theme(cx);
    let lines = [
        ("new_tab", "new tab"),
        ("new_tab_picker", "choose agent"),
        ("toggle_terminal", "terminal"),
        ("open_project", "open project"),
        ("command_palette", "command palette"),
    ];
    let mut list = div().flex().flex_col().gap(px(4.0));
    for (name, label) in lines {
        let key = commands::key_for(name, cx);
        list = list.child(
            div()
                .flex()
                .gap(px(16.0))
                .child(div().w(px(120.0)).flex().justify_end().text_color(theme.accent).child(key))
                .child(div().text_color(theme.muted).child(label)),
        );
    }
    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(20.0))
        .child(div().text_color(theme.faint).child(LOGO))
        .child(list)
        .child(div().text_color(theme.faint).child("or drop a folder here"))
        .into_any_element()
}

const LOGO: &str = "█▄ █ █ █ █▀▀ █   █\n█ ▀█ █▄█ █▄▄ █▄▄ █";
