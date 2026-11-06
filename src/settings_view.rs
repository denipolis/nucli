use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, AppContext, Context, Div, Entity, FocusHandle, InteractiveElement, IntoElement, ParentElement,
    SharedString, Stateful, StatefulInteractiveElement, Styled, Subscription, Window, div, px,
};

use crate::agents::{AgentProfile, ShellProfile};
use crate::commands;
use crate::icons;
use crate::persist::Selected;
use crate::settings::{self, Appearance, CursorShape, FONT, Settings};
use crate::text_input::{InputEvent, TextInput};
use crate::theme::Theme;
use crate::ui;
use crate::window::WorkspaceView;

type Rows = Vec<Vec<Entity<TextInput>>>;

const AGENT_FIELDS: [(&str, &str); 4] =
    [("name", "name"), ("executable", "executable"), ("arguments", "--arguments"), ("config folder", "default")];
const SHELL_FIELDS: [(&str, &str); 2] = [("name", "name"), ("path", "/bin/sh")];

pub struct SettingsPage {
    pub focus: FocusHandle,
    pub selected: Option<Selected>,
    agents: Rows,
    shells: Rows,
    subscriptions: Vec<Subscription>,
}

fn agent_values(profile: &AgentProfile) -> Vec<String> {
    vec![profile.name.clone(), profile.executable.clone(), profile.arguments.clone(), profile.config_dir.clone()]
}

fn shell_values(shell: &ShellProfile) -> Vec<String> {
    vec![shell.name.clone(), shell.path.clone()]
}

fn texts(rows: &Rows, cx: &Context<WorkspaceView>) -> Vec<Vec<String>> {
    rows.iter().map(|row| row.iter().map(|input| input.read(cx).text.clone()).collect()).collect()
}

pub fn toggle(view: &mut WorkspaceView, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    if view.settings.is_some() {
        close(view, window, cx);
        return;
    }
    let focus = cx.focus_handle();
    window.focus(&focus);
    let selected = view.state(cx).and_then(|state| state.selected.clone());
    view.settings =
        Some(SettingsPage { focus, selected, agents: Vec::new(), shells: Vec::new(), subscriptions: Vec::new() });
    cx.notify();
}

pub fn close(view: &mut WorkspaceView, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    view.settings = None;
    view.focus_current(window, cx);
    cx.notify();
}

fn store_fields(view: &WorkspaceView, cx: &mut Context<WorkspaceView>) {
    let Some(page) = &view.settings else { return };
    let (agents, shells) = (texts(&page.agents, cx), texts(&page.shells, cx));
    settings::update(cx, |settings| {
        for (profile, row) in settings.agents.iter_mut().zip(agents) {
            if let [name, executable, arguments, config_dir] = row.as_slice() {
                profile.name.clone_from(name);
                profile.executable.clone_from(executable);
                profile.arguments.clone_from(arguments);
                profile.config_dir.clone_from(config_dir);
            }
        }
        for (shell, row) in settings.shells.iter_mut().zip(shells) {
            if let [name, path] = row.as_slice() {
                shell.name.clone_from(name);
                shell.path.clone_from(path);
            }
        }
    });
}

fn build_rows(
    values: Vec<Vec<String>>,
    fields: &[(&str, &str)],
    subscriptions: &mut Vec<Subscription>,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) -> Rows {
    values
        .into_iter()
        .map(|row| {
            row.iter()
                .zip(fields)
                .map(|(text, (_, placeholder))| {
                    let input = cx.new(|cx| {
                        let mut input = TextInput::new(placeholder, cx);
                        input.set_text(text, cx);
                        input
                    });
                    subscriptions.push(cx.subscribe_in(&input, window, |view, _, event, window, cx| match event {
                        InputEvent::Changed => store_fields(view, cx),
                        InputEvent::Submit | InputEvent::Cancel => view.focus_current(window, cx),
                        _ => {}
                    }));
                    input
                })
                .collect()
        })
        .collect()
}

fn sync_fields(view: &mut WorkspaceView, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let current = settings::get(cx);
    let agents: Vec<_> = current.agents.iter().map(agent_values).collect();
    let shells: Vec<_> = current.shells.iter().map(shell_values).collect();
    let Some(page) = &view.settings else { return };
    if texts(&page.agents, cx) == agents && texts(&page.shells, cx) == shells {
        return;
    }
    let mut subscriptions = Vec::new();
    let agents = build_rows(agents, &AGENT_FIELDS, &mut subscriptions, window, cx);
    let shells = build_rows(shells, &SHELL_FIELDS, &mut subscriptions, window, cx);
    if let Some(page) = &mut view.settings {
        (page.agents, page.shells, page.subscriptions) = (agents, shells, subscriptions);
    }
}

fn section(title: &'static str, theme: &Theme) -> Div {
    div().mt(px(14.0)).pb(px(2.0)).border_b_1().border_color(theme.border).text_color(theme.faint).child(title)
}

fn row(label: &'static str, control: impl IntoElement, theme: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(12.0))
        .child(div().flex_none().w(px(140.0)).text_color(theme.muted).child(label))
        .child(control)
}

fn button(id: impl Into<SharedString>, label: impl Into<SharedString>, theme: &Theme) -> Stateful<Div> {
    div()
        .id(id.into())
        .flex_none()
        .px(px(6.0))
        .border_1()
        .border_color(theme.border)
        .text_color(theme.muted)
        .hover(|style| style.text_color(theme.text).border_color(theme.muted))
        .child(label.into())
}

fn segments<T: Copy + PartialEq + 'static>(
    id: &'static str,
    options: &[(&'static str, T)],
    current: T,
    set: fn(&mut Settings, T),
    theme: &Theme,
) -> Div {
    div().flex().gap(px(4.0)).children(options.iter().map(|(label, value)| {
        let value = *value;
        let active = value == current;
        div()
            .id(SharedString::from(format!("{id}-{label}")))
            .px(px(6.0))
            .when(active, |this| this.bg(theme.accent).text_color(theme.on_accent))
            .when(!active, |this| this.text_color(theme.muted).hover(|style| style.text_color(theme.text)))
            .child(*label)
            .on_click(move |_, _, cx| settings::update(cx, |settings| set(settings, value)))
    }))
}

fn stepper(id: &'static str, value: String, step: f32, change: fn(&mut Settings, f32), theme: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(
            button(format!("{id}-less"), icons::REMOVE, theme)
                .on_click(move |_, _, cx| settings::update(cx, |settings| change(settings, -step))),
        )
        .child(div().min_w(px(32.0)).flex().justify_center().child(value))
        .child(
            button(format!("{id}-more"), icons::ADD, theme)
                .on_click(move |_, _, cx| settings::update(cx, |settings| change(settings, step))),
        )
}

fn checkbox(id: &'static str, on: bool, set: fn(&mut Settings, bool), theme: &Theme) -> Stateful<Div> {
    div()
        .id(id)
        .text_color(if on { theme.accent } else { theme.muted })
        .hover(|style| style.text_color(theme.text))
        .child(if on { "[x]" } else { "[ ]" })
        .on_click(move |_, _, cx| settings::update(cx, |settings| set(settings, !on)))
}

fn theme_card(id: &'static str, name: &'static str, selected: bool, system_dark: bool, theme: &Theme) -> Stateful<Div> {
    let preview = crate::theme::resolve(id, system_dark);
    let swatches = [preview.accent, preview.red, preview.green, preview.yellow, preview.blue, preview.muted];
    div()
        .id(SharedString::from(format!("theme-{id}")))
        .w(px(180.0))
        .p(px(8.0))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .bg(preview.background)
        .text_color(preview.text)
        .border_1()
        .border_color(if selected { theme.accent } else { theme.border })
        .hover(|style| style.border_color(theme.muted))
        .child(div().flex().justify_between().child(name).when(selected, |this| this.child(icons::CHECK)))
        .child(div().flex().gap(px(2.0)).children(swatches.map(|color| div().size(px(10.0)).bg(color))))
        .on_click(move |_, _, cx| settings::update(cx, |settings| settings.theme = id.to_string()))
}

fn field_rows(
    id: &'static str,
    fields: &[(&'static str, &str)],
    rows: &Rows,
    remove: fn(&mut Settings, usize),
    theme: &Theme,
) -> Vec<AnyElement> {
    let cell = || div().flex_1().min_w_0();
    let heading = div()
        .flex()
        .gap(px(6.0))
        .pr(px(28.0))
        .text_color(theme.faint)
        .children(fields.iter().map(|(label, _)| cell().child(*label)));
    std::iter::once(heading.into_any_element())
        .chain(rows.iter().enumerate().map(|(index, inputs)| {
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .children(
                    inputs
                        .iter()
                        .map(|input| cell().px(px(4.0)).border_1().border_color(theme.border).child(input.clone())),
                )
                .child(
                    div()
                        .id(SharedString::from(format!("{id}-remove-{index}")))
                        .flex_none()
                        .w(px(22.0))
                        .flex()
                        .justify_center()
                        .text_color(theme.faint)
                        .hover(|style| style.text_color(theme.red))
                        .child(icons::CLOSE)
                        .on_click(move |_, _, cx| settings::update(cx, |settings| remove(settings, index))),
                )
                .into_any_element()
        }))
        .collect()
}

pub fn render(view: &mut WorkspaceView, window: &mut Window, cx: &mut Context<WorkspaceView>) -> AnyElement {
    sync_fields(view, window, cx);
    let Some(page) = &view.settings else { return div().into_any_element() };
    let theme = ui::theme(cx);
    let current = settings::get(cx).clone();
    let system_dark = ui::system_dark(cx);
    let themes = crate::theme::all()
        .into_iter()
        .map(|(id, name)| theme_card(id, name, id == current.theme, system_dark, &theme).into_any_element());
    let appearance = [("SYSTEM", Appearance::System), ("LIGHT", Appearance::Light), ("DARK", Appearance::Dark)];
    let cursors = [("BLOCK", CursorShape::Block), ("BAR", CursorShape::Bar), ("UNDERLINE", CursorShape::Underline)];
    let content = div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .p(px(18.0))
        .pt(px(4.0))
        .max_w(px(960.0))
        .child(section("THEME", &theme))
        .child(div().flex().flex_wrap().gap(px(8.0)).children(themes))
        .child(row(
            "appearance",
            segments(
                "appearance",
                &appearance,
                current.appearance,
                |settings, value| settings.appearance = value,
                &theme,
            ),
            &theme,
        ))
        .child(section("TEXT", &theme))
        .child(row("font", div().text_color(theme.muted).child(FONT), &theme))
        .child(row(
            "terminal size",
            stepper(
                "font-size",
                format!("{}", current.font_size()),
                1.0,
                |settings, delta| settings.font_size = (settings.font_size + delta).clamp(6.0, 48.0),
                &theme,
            ),
            &theme,
        ))
        .child(row(
            "interface size",
            stepper(
                "ui-font-size",
                format!("{}", current.ui_font_size()),
                1.0,
                |settings, delta| settings.ui_font_size = (settings.ui_font_size + delta).clamp(8.0, 24.0),
                &theme,
            ),
            &theme,
        ))
        .child(row(
            "line height",
            stepper(
                "line-height",
                format!("{:.2}", current.line_height()),
                0.05,
                |settings, delta| {
                    settings.line_height = (((settings.line_height + delta) * 100.0).round() / 100.0).clamp(1.0, 2.0)
                },
                &theme,
            ),
            &theme,
        ))
        .child(row(
            "ligatures",
            checkbox("ligatures", current.ligatures, |settings, on| settings.ligatures = on, &theme),
            &theme,
        ))
        .child(section("TERMINAL", &theme))
        .child(row(
            "cursor",
            segments("cursor", &cursors, current.cursor, |settings, value| settings.cursor = value, &theme),
            &theme,
        ))
        .child(row(
            "cursor blinks",
            checkbox("cursor-blink", current.cursor_blink, |settings, on| settings.cursor_blink = on, &theme),
            &theme,
        ))
        .child(section("GENERAL", &theme))
        .child(row(
            "reopen tabs",
            checkbox("restore-tabs", current.restore_tabs, |settings, on| settings.restore_tabs = on, &theme),
            &theme,
        ))
        .child(row(
            "notifications",
            checkbox("notifications", current.notifications, |settings, on| settings.notifications = on, &theme),
            &theme,
        ))
        .child(section("AGENTS", &theme))
        .children(field_rows(
            "agent",
            &AGENT_FIELDS,
            &page.agents,
            |settings, index| {
                settings.agents.remove(index);
            },
            &theme,
        ))
        .child(
            div()
                .flex()
                .gap(px(6.0))
                .child(button("agent-add", "add", &theme).on_click(|_, _, cx| {
                    settings::update(cx, |settings| {
                        settings.agents.push(AgentProfile {
                            name: "agent".into(),
                            executable: "claude".into(),
                            ..Default::default()
                        })
                    })
                }))
                .child(
                    button("agent-detect", "detect", &theme)
                        .on_click(|_, window, cx| window.dispatch_action(Box::new(commands::DetectAgents), cx)),
                ),
        )
        .child(section("SHELLS", &theme))
        .children(field_rows(
            "shell",
            &SHELL_FIELDS,
            &page.shells,
            |settings, index| {
                settings.shells.remove(index);
            },
            &theme,
        ))
        .child(
            div()
                .flex()
                .gap(px(6.0))
                .child(button("shell-add", "add", &theme).on_click(|_, _, cx| {
                    settings::update(cx, |settings| {
                        settings.shells.push(ShellProfile { name: "shell".into(), path: "/bin/sh".into() })
                    })
                }))
                .child(
                    button("shell-detect", "detect", &theme)
                        .on_click(|_, window, cx| window.dispatch_action(Box::new(commands::DetectShells), cx)),
                ),
        )
        .child(section("FILES", &theme))
        .child(
            div()
                .flex()
                .gap(px(6.0))
                .child(
                    button("edit-settings", "settings.json", &theme)
                        .on_click(|_, window, cx| window.dispatch_action(Box::new(commands::OpenSettingsFile), cx)),
                )
                .child(
                    button("edit-keybindings", "keybindings.json", &theme)
                        .on_click(|_, window, cx| window.dispatch_action(Box::new(commands::OpenKeybindings), cx)),
                ),
        );
    div()
        .id("settings-page")
        .key_context("Settings")
        .track_focus(&page.focus)
        .size_full()
        .overflow_y_scroll()
        .bg(theme.background)
        .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
            if event.keystroke.key == "escape" {
                close(view, window, cx);
                cx.stop_propagation();
            }
        }))
        .child(content)
        .into_any_element()
}
