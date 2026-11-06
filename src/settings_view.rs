use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, AppContext, Context, Div, Entity, FocusHandle, InteractiveElement, IntoElement, ParentElement,
    SharedString, Stateful, StatefulInteractiveElement, Styled, Subscription, Window, div, px,
};

use crate::agents::{self, AgentProfile, ShellProfile};
use crate::commands::{self, COMMANDS, Keys};
use crate::icons;
use crate::persist::{Selected, SettingsSection};
use crate::settings::{self, Appearance, CursorShape, FONT, Settings};
use crate::store;
use crate::text_input::{InputEvent, TextInput};
use crate::theme::Theme;
use crate::ui;
use crate::window::WorkspaceView;

type Rows = Vec<Vec<Entity<TextInput>>>;

const SECTIONS: [(SettingsSection, &str, &str, &str); 6] = [
    (
        SettingsSection::General,
        icons::SETTINGS,
        "General",
        "Startup, notifications and the files behind these settings.",
    ),
    (SettingsSection::Appearance, icons::THEME, "Appearance", "Theme, light or dark, and the size of the interface."),
    (SettingsSection::Terminal, icons::TERMINAL, "Terminal", "Font and cursor of terminals and the file view."),
    (SettingsSection::Agents, icons::AGENT, "Agents", "The agent CLIs you can start in a tab."),
    (SettingsSection::Shells, icons::COMMAND, "Shells", "The shells you can start in a tab."),
    (SettingsSection::Keybindings, icons::KEYBOARD, "Keybindings", "Click a key, then press the new one."),
];

const AGENT_FIELDS: [(&str, &str); 4] =
    [("Name", "name"), ("Executable", "executable"), ("Arguments", "--arguments"), ("Config folder", "default")];
const SHELL_FIELDS: [(&str, &str); 2] = [("Name", "name"), ("Path", "/bin/sh")];
const THEME_COLUMNS: usize = 3;

pub struct SettingsPage {
    pub focus: FocusHandle,
    pub selected: Option<Selected>,
    filter: Entity<TextInput>,
    recording: Option<(&'static str, Subscription)>,
    agents: Rows,
    shells: Rows,
    subscriptions: Vec<Subscription>,
    _filter_subscription: Subscription,
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

fn set_section(section: SettingsSection, cx: &mut App) {
    store::store(cx).update(cx, |store, cx| {
        store.ui.settings_section = section;
        store.changed(cx);
    });
}

pub fn show(
    view: &mut WorkspaceView,
    section: Option<SettingsSection>,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) {
    if let Some(section) = section {
        set_section(section, cx);
    }
    if view.settings.is_none() {
        let focus = cx.focus_handle();
        let selected = view.state(cx).and_then(|state| state.selected.clone());
        let filter = cx.new(|cx| TextInput::new("filter commands", cx));
        let filter_subscription = cx.subscribe_in(&filter, window, |view, _, event, window, cx| match event {
            InputEvent::Submit | InputEvent::Cancel => view.focus_current(window, cx),
            _ => cx.notify(),
        });
        view.settings = Some(SettingsPage {
            focus,
            selected,
            filter,
            recording: None,
            agents: Vec::new(),
            shells: Vec::new(),
            subscriptions: Vec::new(),
            _filter_subscription: filter_subscription,
        });
    }
    view.focus_current(window, cx);
    cx.notify();
}

pub fn toggle(view: &mut WorkspaceView, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    if view.settings.is_some() {
        close(view, window, cx);
    } else {
        show(view, None, window, cx);
    }
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

fn start_recording(view: &mut WorkspaceView, name: &'static str, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let this = cx.entity().downgrade();
    let subscription = cx.intercept_keystrokes(move |event, _, cx| {
        let key = event.keystroke.unparse();
        let escape = event.keystroke.key == "escape";
        let _ = this.update(cx, |view, cx| finish_recording(view, (!escape).then_some(key.as_str()), cx));
        cx.stop_propagation();
    });
    if let Some(page) = &mut view.settings {
        page.recording = Some((name, subscription));
    }
    view.focus_current(window, cx);
    cx.notify();
}

fn finish_recording(view: &mut WorkspaceView, key: Option<&str>, cx: &mut Context<WorkspaceView>) {
    let Some((name, _)) = view.settings.as_mut().and_then(|page| page.recording.take()) else { return };
    if key.is_some() {
        commands::rebind(name, key);
        crate::apply_keys(cx);
    }
    cx.notify();
}

fn group(theme: &Theme) -> Div {
    div().flex().flex_col().border_1().border_color(theme.border).bg(theme.surface)
}

fn row(label: &'static str, hint: &'static str, control: impl IntoElement, theme: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(16.0))
        .px(px(12.0))
        .py(px(8.0))
        .border_b_1()
        .border_color(theme.border)
        .child(
            div()
                .flex()
                .flex_col()
                .min_w_0()
                .child(label)
                .when(!hint.is_empty(), |this| this.child(div().text_color(theme.faint).child(hint))),
        )
        .child(div().flex_none().child(control))
}

fn button(id: impl Into<SharedString>, label: impl Into<SharedString>, theme: &Theme) -> Stateful<Div> {
    div()
        .id(id.into())
        .flex_none()
        .px(px(8.0))
        .border_1()
        .border_color(theme.border)
        .bg(theme.elevated)
        .text_color(theme.text)
        .hover(|style| style.bg(theme.hover).border_color(theme.muted))
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
            .px(px(8.0))
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
        .flex_1()
        .min_w_0()
        .p(px(8.0))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .bg(preview.background)
        .text_color(preview.text)
        .border_1()
        .border_color(if selected { theme.accent } else { theme.border })
        .hover(|style| style.border_color(theme.muted))
        .child(
            div()
                .flex()
                .justify_between()
                .child(div().min_w_0().truncate().child(name))
                .when(selected, |this| this.child(div().flex_none().child(icons::CHECK))),
        )
        .child(div().flex().gap(px(2.0)).children(swatches.map(|color| div().size(px(10.0)).bg(color))))
        .on_click(move |_, _, cx| settings::update(cx, |settings| settings.theme = id.to_string()))
}

fn table(
    id: &'static str,
    fields: &[(&'static str, &str)],
    rows: &Rows,
    glyphs: Vec<Option<(&'static str, gpui::Hsla)>>,
    remove: fn(&mut Settings, usize),
    theme: &Theme,
) -> Div {
    let cell = || div().flex_1().min_w_0();
    let heading = div()
        .flex()
        .gap(px(6.0))
        .pl(px(32.0))
        .pr(px(34.0))
        .py(px(4.0))
        .border_b_1()
        .border_color(theme.border)
        .text_color(theme.faint)
        .children(fields.iter().map(|(label, _)| cell().child(*label)));
    group(theme).child(heading).children(rows.iter().enumerate().map(|(index, inputs)| {
        let glyph = glyphs.get(index).copied().flatten();
        div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .pl(px(10.0))
            .pr(px(6.0))
            .py(px(4.0))
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex_none()
                    .w(px(16.0))
                    .when_some(glyph, |this, (glyph, color)| this.text_color(color).child(glyph)),
            )
            .children(inputs.iter().map(|input| {
                cell().px(px(4.0)).bg(theme.background).border_1().border_color(theme.border).child(input.clone())
            }))
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
    }))
}

fn general(current: &Settings, theme: &Theme) -> Vec<AnyElement> {
    vec![
        group(theme)
            .child(row(
                "Reopen tabs",
                "Reopen tabs and agent conversations from the last session",
                checkbox("restore-tabs", current.restore_tabs, |settings, on| settings.restore_tabs = on, theme),
                theme,
            ))
            .child(row(
                "Notifications",
                "Notify when an agent finishes or waits in the background",
                checkbox("notifications", current.notifications, |settings, on| settings.notifications = on, theme),
                theme,
            ))
            .child(row(
                "Settings file",
                "Everything on these pages is stored here",
                button("edit-settings", "settings.json", theme)
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(commands::OpenSettingsFile), cx)),
                theme,
            ))
            .into_any_element(),
    ]
}

fn appearance(current: &Settings, system_dark: bool, theme: &Theme) -> Vec<AnyElement> {
    let modes = [("System", Appearance::System), ("Light", Appearance::Light), ("Dark", Appearance::Dark)];
    let themes = crate::theme::all();
    let cards = themes.chunks(THEME_COLUMNS).map(|chunk| {
        let spacers = THEME_COLUMNS - chunk.len();
        div()
            .flex()
            .gap(px(8.0))
            .children(chunk.iter().map(|(id, name)| theme_card(id, name, *id == current.theme, system_dark, theme)))
            .children(
                (0..spacers).map(|_| div().flex_1().p(px(8.0)).border_1().border_color(gpui::transparent_black())),
            )
    });
    vec![
        div().flex().flex_col().gap(px(8.0)).children(cards).into_any_element(),
        group(theme)
            .child(row(
                "Mode",
                "Light or dark side of a theme",
                segments(
                    "appearance",
                    &modes,
                    current.appearance,
                    |settings, value| settings.appearance = value,
                    theme,
                ),
                theme,
            ))
            .child(row(
                "Interface size",
                "Text in the sidebar, panel and menus",
                stepper(
                    "ui-font-size",
                    format!("{}", current.ui_font_size()),
                    1.0,
                    |settings, delta| settings.ui_font_size = (settings.ui_font_size + delta).clamp(8.0, 24.0),
                    theme,
                ),
                theme,
            ))
            .into_any_element(),
    ]
}

fn terminal(current: &Settings, theme: &Theme) -> Vec<AnyElement> {
    let cursors = [("Block", CursorShape::Block), ("Bar", CursorShape::Bar), ("Underline", CursorShape::Underline)];
    vec![
        group(theme)
            .child(row("Font", "Bundled with Nucli", div().text_color(theme.muted).child(FONT), theme))
            .child(row(
                "Font size",
                "",
                stepper(
                    "font-size",
                    format!("{}", current.font_size()),
                    1.0,
                    |settings, delta| settings.font_size = (settings.font_size + delta).clamp(6.0, 48.0),
                    theme,
                ),
                theme,
            ))
            .child(row(
                "Line height",
                "",
                stepper(
                    "line-height",
                    format!("{:.2}", current.line_height()),
                    0.05,
                    |settings, delta| {
                        settings.line_height =
                            (((settings.line_height + delta) * 100.0).round() / 100.0).clamp(1.0, 2.0)
                    },
                    theme,
                ),
                theme,
            ))
            .child(row(
                "Ligatures",
                "",
                checkbox("ligatures", current.ligatures, |settings, on| settings.ligatures = on, theme),
                theme,
            ))
            .into_any_element(),
        group(theme)
            .child(row(
                "Cursor",
                "",
                segments("cursor", &cursors, current.cursor, |settings, value| settings.cursor = value, theme),
                theme,
            ))
            .child(row(
                "Cursor blinks",
                "",
                checkbox("cursor-blink", current.cursor_blink, |settings, on| settings.cursor_blink = on, theme),
                theme,
            ))
            .into_any_element(),
    ]
}

fn agents_page(page: &SettingsPage, current: &Settings, theme: &Theme) -> Vec<AnyElement> {
    let glyphs = current
        .agents
        .iter()
        .map(|profile| {
            let (glyph, color) = icons::agent(&agents::command_name(&profile.executable));
            Some((glyph, color.color(theme.dark)))
        })
        .collect();
    vec![
        table(
            "agent",
            &AGENT_FIELDS,
            &page.agents,
            glyphs,
            |settings, index| {
                settings.agents.remove(index);
            },
            theme,
        )
        .into_any_element(),
        div()
            .flex()
            .gap(px(6.0))
            .child(button("agent-add", "Add", theme).on_click(|_, _, cx| {
                settings::update(cx, |settings| {
                    settings.agents.push(AgentProfile {
                        name: "agent".into(),
                        executable: "claude".into(),
                        ..Default::default()
                    })
                })
            }))
            .child(
                button("agent-detect", "Detect", theme)
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(commands::DetectAgents), cx)),
            )
            .into_any_element(),
    ]
}

fn shells_page(page: &SettingsPage, current: &Settings, theme: &Theme) -> Vec<AnyElement> {
    let glyphs = current.shells.iter().map(|_| Some((icons::TERMINAL, theme.muted))).collect();
    vec![
        table(
            "shell",
            &SHELL_FIELDS,
            &page.shells,
            glyphs,
            |settings, index| {
                settings.shells.remove(index);
            },
            theme,
        )
        .into_any_element(),
        div()
            .flex()
            .gap(px(6.0))
            .child(button("shell-add", "Add", theme).on_click(|_, _, cx| {
                settings::update(cx, |settings| {
                    settings.shells.push(ShellProfile { name: "shell".into(), path: "/bin/sh".into() })
                })
            }))
            .child(
                button("shell-detect", "Detect", theme)
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(commands::DetectShells), cx)),
            )
            .into_any_element(),
    ]
}

fn keybindings(view: &WorkspaceView, theme: &Theme, cx: &mut Context<WorkspaceView>) -> Vec<AnyElement> {
    let Some(page) = &view.settings else { return Vec::new() };
    let filter = page.filter.clone();
    let recording = page.recording.as_ref().map(|(name, _)| *name);
    let query = filter.read(cx).text.trim().to_lowercase();
    let bound = cx.try_global::<Keys>().map(|keys| keys.by_name.clone()).unwrap_or_default();
    let mut rows = Vec::new();
    for command in COMMANDS {
        let key = bound.get(command.name).cloned().unwrap_or_default();
        let shown = commands::display(&key);
        let hit = [command.title, command.name, shown.as_str()].iter().any(|text| text.to_lowercase().contains(&query));
        if !query.is_empty() && !hit {
            continue;
        }
        let name = command.name;
        let active = recording == Some(name);
        let conflict = !key.is_empty()
            && COMMANDS.iter().any(|other| {
                other.name != name && other.context == command.context && bound.get(other.name) == Some(&key)
            });
        let changed = key != commands::default_key(command);
        let color = if conflict { theme.red } else { theme.text };
        rows.push(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .px(px(12.0))
                .py(px(3.0))
                .border_b_1()
                .border_color(theme.border)
                .child(div().min_w_0().truncate().child(command.title))
                .child(
                    div()
                        .flex()
                        .flex_none()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            div()
                                .id(SharedString::from(format!("key-reset-{name}")))
                                .w(px(16.0))
                                .text_color(theme.faint)
                                .hover(|style| style.text_color(theme.accent))
                                .when(changed && !active, |this| {
                                    this.child(icons::DISCARD).on_click(cx.listener(move |_, _, _, cx| {
                                        commands::rebind(name, None);
                                        crate::apply_keys(cx);
                                        cx.notify();
                                    }))
                                }),
                        )
                        .child(
                            div()
                                .id(SharedString::from(format!("key-{name}")))
                                .min_w(px(90.0))
                                .flex()
                                .justify_center()
                                .px(px(6.0))
                                .border_1()
                                .border_color(if active { theme.accent } else { theme.border })
                                .when(active, |this| this.bg(theme.accent).text_color(theme.on_accent))
                                .when(!active, |this| {
                                    this.bg(theme.elevated)
                                        .text_color(if key.is_empty() { theme.faint } else { color })
                                        .hover(|style| style.border_color(theme.muted))
                                })
                                .child(if active {
                                    "press keys".to_string()
                                } else if key.is_empty() {
                                    "none".to_string()
                                } else {
                                    shown.clone()
                                })
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    if view.settings.as_ref().is_some_and(|page| page.recording.is_none()) {
                                        start_recording(view, name, window, cx);
                                    }
                                })),
                        ),
                )
                .into_any_element(),
        );
    }
    vec![
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .px(px(6.0))
                    .bg(theme.background)
                    .border_1()
                    .border_color(theme.border)
                    .child(filter),
            )
            .child(
                button("edit-keybindings", "keybindings.json", theme)
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(commands::OpenKeybindingsFile), cx)),
            )
            .into_any_element(),
        group(theme).children(rows).into_any_element(),
    ]
}

pub fn render(view: &mut WorkspaceView, window: &mut Window, cx: &mut Context<WorkspaceView>) -> AnyElement {
    sync_fields(view, window, cx);
    let theme = ui::theme(cx);
    let current = settings::get(cx).clone();
    let system_dark = ui::system_dark(cx);
    let section = view.store(cx).ui.settings_section;
    let (title, description) = SECTIONS
        .iter()
        .find(|(id, ..)| *id == section)
        .map(|(_, _, title, description)| (*title, *description))
        .unwrap_or_default();
    let nav = div()
        .flex()
        .flex_col()
        .flex_none()
        .w(px(170.0))
        .h_full()
        .py(px(8.0))
        .bg(theme.surface)
        .border_r_1()
        .border_color(theme.border)
        .children(SECTIONS.iter().map(|(id, glyph, label, _)| {
            let id = *id;
            let active = id == section;
            div()
                .id(SharedString::from(format!("settings-{label}")))
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(12.0))
                .py(px(3.0))
                .when(active, |this| this.bg(theme.accent).text_color(theme.on_accent))
                .when(!active, |this| {
                    this.text_color(theme.muted).hover(|style| style.bg(theme.hover).text_color(theme.text))
                })
                .child(div().flex_none().w(px(16.0)).child(*glyph))
                .child(*label)
                .on_click(move |_, _, cx| set_section(id, cx))
        }));
    let body = match (section, &view.settings) {
        (SettingsSection::General, _) => general(&current, &theme),
        (SettingsSection::Appearance, _) => appearance(&current, system_dark, &theme),
        (SettingsSection::Terminal, _) => terminal(&current, &theme),
        (SettingsSection::Agents, Some(page)) => agents_page(page, &current, &theme),
        (SettingsSection::Shells, Some(page)) => shells_page(page, &current, &theme),
        (SettingsSection::Keybindings, _) => keybindings(view, &theme, cx),
        _ => Vec::new(),
    };
    let Some(page) = &view.settings else { return div().into_any_element() };
    div()
        .id("settings-page")
        .key_context("Settings")
        .track_focus(&page.focus)
        .flex()
        .size_full()
        .bg(theme.background)
        .on_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
            if event.keystroke.key == "escape" {
                close(view, window, cx);
                cx.stop_propagation();
            }
        }))
        .child(nav)
        .child(
            div().id("settings-content").flex_1().min_w_0().h_full().overflow_y_scroll().child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .max_w(px(820.0))
                    .p(px(20.0))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(div().text_size(px(20.0)).line_height(px(28.0)).child(title))
                            .child(div().text_color(theme.faint).child(description)),
                    )
                    .children(body),
            ),
        )
        .into_any_element()
}
