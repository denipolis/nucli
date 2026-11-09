use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gpui::prelude::FluentBuilder;
use gpui::{
    Animation, AnimationExt, AnyElement, App, AppContext, Bounds, Context, Div, Entity, FocusHandle, Global, Hsla,
    InteractiveElement, IntoElement, KeyDownEvent, MouseButton, ParentElement, PathPromptOptions, Render, SharedString,
    Stateful, StatefulInteractiveElement, Styled, Task, TitlebarOptions, Window, WindowBounds, WindowOptions, div,
    point, px, size,
};

use crate::agents::{self, AgentProfile, ShellProfile};
use crate::icons;
use crate::paths;
use crate::persist::StateFile;
use crate::settings::{self, Appearance, CursorShape, FONT, Settings};
use crate::settings_view;
use crate::theme::Theme;
use crate::ui;

pub struct Active;

impl Global for Active {}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Welcome,
    Appearance,
    Terminal,
    Agents,
    Shells,
    Ready,
}

const STEPS: [(Step, &str, &str); 5] = [
    (Step::Appearance, "Appearance", "Pick a theme. The interface, terminals and the file view all follow it."),
    (Step::Terminal, "Terminal", "Font, size, line height and cursor of terminals and the file view."),
    (Step::Agents, "Agents", "Agent CLIs found on this machine. Each one is a profile you can start in a tab."),
    (Step::Shells, "Shells", "Shells you can start in a tab. The first one is the default."),
    (Step::Ready, "Ready", "Everything here can be changed later in Settings."),
];

const PIXEL: f32 = 18.0;
const ROWS: usize = 4;
const COLUMN_MS: u128 = 50;
const ACTIONS_MS: u128 = 1100;
const BLINK_MS: u128 = 530;

struct Import {
    source: String,
    keybindings: Option<String>,
    state: Option<StateFile>,
}

pub struct Onboarding {
    focus: FocusHandle,
    step: Step,
    shown: Instant,
    detecting: bool,
    agents: Vec<(AgentProfile, bool)>,
    shells: Vec<(ShellProfile, bool)>,
    import: Option<Import>,
    legacy: bool,
    font_picker: Entity<crate::font_picker::FontPicker>,
    error: Option<String>,
    _ticker: Task<()>,
}

pub fn open(cx: &mut App) {
    cx.set_global(Active);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(980.0), px(680.0)), cx))),
        titlebar: Some(TitlebarOptions {
            title: Some("Welcome to Nucli".into()),
            appears_transparent: cfg!(target_os = "macos"),
            traffic_light_position: Some(point(px(10.0), px(9.0))),
        }),
        app_id: Some("uk.polishch.nucli".into()),
        window_decorations: Some(gpui::WindowDecorations::Server),
        window_min_size: Some(size(px(760.0), px(560.0))),
        ..Default::default()
    };
    if let Err(error) = cx.open_window(options, |window, cx| cx.new(|cx| Onboarding::new(window, cx))) {
        let text = format!("Nucli could not open a window: {error:#}");
        eprintln!("{text}");
        crate::notify::alert(&text);
        cx.quit();
    }
}

#[cfg(target_os = "macos")]
fn legacy_import() -> Option<(Settings, StateFile)> {
    if paths::DEV { None } else { crate::import::read() }
}

#[cfg(not(target_os = "macos"))]
fn legacy_import() -> Option<(Settings, StateFile)> {
    None
}

fn read_backup(folder: &Path) -> Result<(Settings, Import), String> {
    let text = std::fs::read_to_string(folder.join("settings.json"))
        .map_err(|_| format!("no settings.json in {}", paths::display(folder)))?;
    let settings = serde_json::from_str(&text).map_err(|error| format!("settings.json: {error}"))?;
    let keybindings = std::fs::read_to_string(folder.join("keybindings.json")).ok();
    let state = std::fs::read_to_string(folder.join("state.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<StateFile>(&text).ok());
    Ok((settings, Import { source: paths::display(folder), keybindings, state }))
}

fn merge<T: Clone>(list: &mut Vec<(T, bool)>, found: Vec<T>, key: fn(&T) -> &str) {
    for item in found {
        if !list.iter().any(|(existing, _)| key(existing) == key(&item)) {
            list.push((item, true));
        }
    }
}

fn position(step: Step) -> Option<usize> {
    STEPS.iter().position(|(item, ..)| *item == step)
}

impl Onboarding {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        ui::watch_appearance(window, cx);
        cx.observe_global::<Settings>(|_, cx| cx.notify()).detach();
        window.on_window_should_close(cx, |_, cx| {
            cx.quit();
            true
        });
        let focus = cx.focus_handle();
        window.focus(&focus);
        let ticker = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(40)).await;
                let alive = this.update(cx, |view, cx| {
                    let time = view.shown.elapsed().as_millis();
                    let welcome = view.step == Step::Welcome && (time < ACTIONS_MS + 600 || time % BLINK_MS < 40);
                    let blink = view.step == Step::Terminal && time % BLINK_MS < 40;
                    if welcome || blink || view.detecting {
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        });
        let mut view = Onboarding {
            focus,
            step: Step::Welcome,
            shown: Instant::now(),
            detecting: false,
            agents: Vec::new(),
            shells: Vec::new(),
            import: None,
            legacy: legacy_import().is_some(),
            font_picker: cx.new(|cx| crate::font_picker::FontPicker::new(window, cx)),
            error: None,
            _ticker: ticker,
        };
        view.detect(false, cx);
        view
    }

    fn detect(&mut self, again: bool, cx: &mut Context<Self>) {
        self.detecting = true;
        let found = cx.background_executor().spawn(async { (agents::detect_agents(), agents::detect_shells()) });
        cx.spawn(async move |this, cx| {
            let (found_agents, found_shells) = found.await;
            let _ = this.update(cx, |view, cx| {
                view.detecting = false;
                let fresh = again || view.import.is_none();
                if fresh || view.agents.is_empty() {
                    merge(&mut view.agents, found_agents, |profile: &AgentProfile| profile.name.as_str());
                }
                if fresh || view.shells.is_empty() {
                    merge(&mut view.shells, found_shells, |shell: &ShellProfile| shell.path.as_str());
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn go(&mut self, step: Step, cx: &mut Context<Self>) {
        self.step = step;
        self.error = None;
        cx.notify();
    }

    fn next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match position(self.step) {
            None => self.go(STEPS[0].0, cx),
            Some(index) if index + 1 < STEPS.len() => self.go(STEPS[index + 1].0, cx),
            Some(_) => self.start(window, cx),
        }
    }

    fn back(&mut self, cx: &mut Context<Self>) {
        match position(self.step) {
            Some(0) => self.go(Step::Welcome, cx),
            Some(index) => self.go(STEPS[index - 1].0, cx),
            None => {}
        }
    }

    fn apply_import(&mut self, settings: Settings, import: Import, cx: &mut Context<Self>) {
        if !settings.agents.is_empty() {
            self.agents = settings.agents.iter().cloned().map(|profile| (profile, true)).collect();
        }
        if !settings.shells.is_empty() {
            self.shells = settings.shells.iter().cloned().map(|shell| (shell, true)).collect();
        }
        cx.set_global(settings);
        self.import = Some(import);
        self.go(Step::Ready, cx);
    }

    fn import_backup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: true,
            multiple: false,
            prompt: Some("Import".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else { return };
            let Some(path) = paths.into_iter().next() else { return };
            let folder: PathBuf =
                if path.is_file() { path.parent().map(Path::to_path_buf).unwrap_or(path) } else { path };
            let _ = this.update(cx, |view, cx| match read_backup(&folder) {
                Ok((settings, import)) => view.apply_import(settings, import, cx),
                Err(error) => {
                    view.error = Some(error);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn import_legacy(&mut self, cx: &mut Context<Self>) {
        match legacy_import() {
            Some((settings, state)) => {
                let import = Import { source: "Nucli 1".into(), keybindings: None, state: Some(state) };
                self.apply_import(settings, import, cx);
            }
            None => {
                self.error = Some("Nucli 1 settings could not be read".into());
                cx.notify();
            }
        }
    }

    fn start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut settings = settings::get(cx).clone();
        settings.agents = self.agents.iter().filter(|(_, on)| *on).map(|(profile, _)| profile.clone()).collect();
        settings.shells = self.shells.iter().filter(|(_, on)| *on).map(|(shell, _)| shell.clone()).collect();
        cx.remove_global::<Active>();
        settings.write();
        cx.set_global(settings);
        if let Some(import) = self.import.take() {
            if let Some(keys) = import.keybindings {
                let _ = paths::write_atomic(&paths::keybindings_file(), keys.as_bytes());
            }
            if let Some(state) = import.state.filter(|_| !paths::state_file().exists()) {
                state.write();
            }
        }
        window.remove_window();
        cx.defer(|cx| {
            crate::apply_keys(cx);
            crate::launch(cx);
        });
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let modifiers = event.keystroke.modifiers;
        if modifiers.platform || modifiers.control || modifiers.alt {
            return;
        }
        match event.keystroke.key.as_str() {
            "enter" | "right" => self.next(window, cx),
            "escape" | "left" => self.back(cx),
            _ => return,
        }
        cx.stop_propagation();
    }
}

fn primary(id: &'static str, label: impl Into<SharedString>, theme: &Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .px(px(12.0))
        .bg(theme.accent)
        .text_color(theme.on_accent)
        .border_1()
        .border_color(theme.accent)
        .hover(|style| style.bg(theme.text).border_color(theme.text).text_color(theme.background))
        .child(label.into())
}

fn footer(chrome: bool, theme: &Theme) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_between()
        .gap(px(16.0))
        .px(px(16.0))
        .py(px(10.0))
        .border_t_1()
        .border_color(if chrome { theme.border } else { gpui::transparent_black() })
        .when(chrome, |this| this.bg(theme.surface))
}

fn spinner(time: u128) -> &'static str {
    icons::SPINNER[(time / 110) as usize % icons::SPINNER.len()]
}

fn logo_columns() -> Vec<[bool; ROWS]> {
    let lines: Vec<Vec<char>> = crate::overview::LOGO.lines().map(|line| line.chars().collect()).collect();
    let width = lines.iter().map(Vec::len).max().unwrap_or(0);
    (0..width)
        .map(|column| {
            std::array::from_fn(|row| {
                let cell = lines.get(row / 2).and_then(|line| line.get(column)).copied().unwrap_or(' ');
                if row % 2 == 0 { matches!(cell, '█' | '▀') } else { matches!(cell, '█' | '▄') }
            })
        })
        .collect()
}

fn logo(time: u128, theme: &Theme) -> Div {
    let columns = logo_columns();
    let revealed = (time.saturating_sub(150) / COLUMN_MS) as usize;
    let blink = revealed >= columns.len() && (time / BLINK_MS).is_multiple_of(2);
    let column = |cells: [Option<Hsla>; ROWS]| {
        div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .children(cells.map(|color| div().size(px(PIXEL)).when_some(color, Div::bg)))
    };
    let mut row = div().flex().gap(px(2.0));
    for (index, cells) in columns.iter().enumerate() {
        let age = time.saturating_sub(150 + index as u128 * COLUMN_MS);
        let color = if age < 200 { theme.text } else { theme.accent };
        row = row.child(column(match index.cmp(&revealed) {
            std::cmp::Ordering::Less => cells.map(|on| on.then_some(color)),
            std::cmp::Ordering::Equal => [Some(theme.accent); ROWS],
            std::cmp::Ordering::Greater => [None; ROWS],
        }));
    }
    let mut underline = [None; ROWS];
    underline[ROWS - 1] = blink.then_some(theme.accent);
    row.child(column([None; ROWS])).child(column(underline)).child(column(underline))
}

fn terminal_preview(current: &Settings, time: u128, theme: &Theme) -> Div {
    let size = current.font_size();
    let line = px((size * current.line_height()).round());
    let text = |color, content: &'static str| div().text_color(color).child(content);
    let visible = !current.cursor_blink || (time / BLINK_MS).is_multiple_of(2);
    let color = if visible { theme.accent } else { gpui::transparent_black() };
    let cursor = match current.cursor {
        CursorShape::Block => div().w(px(size * 0.6)).h(line).bg(color),
        CursorShape::Bar => div().w(px(2.0)).h(line).bg(color),
        CursorShape::Underline => {
            div().w(px(size * 0.6)).h(line).flex().items_end().child(div().w_full().h(px(2.0)).bg(color))
        }
    };
    div()
        .flex()
        .flex_col()
        .p(px(12.0))
        .bg(theme.background)
        .border_1()
        .border_color(theme.border)
        .font_family(current.font.clone())
        .text_size(px(size))
        .line_height(line)
        .child(div().flex().gap(px(8.0)).child(text(theme.blue, "~/projects/app")).child(text(theme.muted, "main")))
        .child(div().flex().gap(px(8.0)).child(text(theme.green, "❯")).child(text(theme.text, "claude")))
        .child(div().flex().gap(px(8.0)).child(text(theme.orange, "✻")).child(text(theme.text, "Reading src/main.rs")))
        .child(
            div()
                .flex()
                .gap(px(8.0))
                .pl(px(size * 1.2))
                .child(text(theme.muted, "Edited 2 files"))
                .child(text(theme.green, "+14"))
                .child(text(theme.red, "-3")),
        )
        .child(div().flex().items_center().gap(px(8.0)).child(text(theme.green, "❯")).child(cursor))
}

fn mini_window(preview: &Theme) -> Div {
    let bar = |width: f32, color: Hsla| div().w(px(width)).h(px(4.0)).bg(color);
    div()
        .flex_1()
        .flex()
        .h(px(64.0))
        .bg(preview.background)
        .child(
            div()
                .flex_none()
                .w(px(34.0))
                .h_full()
                .flex()
                .flex_col()
                .gap(px(5.0))
                .p(px(6.0))
                .bg(preview.surface)
                .border_r_1()
                .border_color(preview.border)
                .child(bar(18.0, preview.accent))
                .child(bar(14.0, preview.muted))
                .child(bar(20.0, preview.muted))
                .child(bar(12.0, preview.faint)),
        )
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .p(px(10.0))
                .child(bar(64.0, preview.text))
                .child(div().flex().gap(px(4.0)).child(bar(24.0, preview.green)).child(bar(36.0, preview.muted)))
                .child(div().flex().gap(px(4.0)).child(bar(18.0, preview.blue)).child(bar(44.0, preview.text)))
                .child(bar(10.0, preview.accent)),
        )
}

fn mode_tiles(current: &Settings, theme: &Theme) -> Div {
    let modes = [
        (Appearance::System, "System", "follows the system"),
        (Appearance::Light, "Light", "always light"),
        (Appearance::Dark, "Dark", "always dark"),
    ];
    let light = crate::theme::resolve(&current.theme, false);
    let dark = crate::theme::resolve(&current.theme, true);
    div().flex().gap(px(8.0)).children(modes.map(|(mode, label, hint)| {
        let selected = current.appearance == mode;
        let preview = match mode {
            Appearance::System => div().flex().child(mini_window(&light)).child(mini_window(&dark)),
            Appearance::Light => div().flex().child(mini_window(&light)),
            Appearance::Dark => div().flex().child(mini_window(&dark)),
        };
        div()
            .id(SharedString::from(format!("mode-{label}")))
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .p(px(8.0))
            .bg(theme.surface)
            .border_1()
            .border_color(if selected { theme.accent } else { theme.border })
            .hover(|style| style.border_color(theme.muted))
            .child(preview.border_1().border_color(theme.border))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .child(div().text_color(if selected { theme.accent } else { theme.text }).child(label))
                            .child(div().text_color(theme.faint).child(hint)),
                    )
                    .when(selected, |this| this.child(div().text_color(theme.accent).child(icons::CHECK))),
            )
            .on_click(move |_, _, cx| settings::update(cx, |settings| settings.appearance = mode))
    }))
}

fn section(title: &'static str, hint: &'static str, theme: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(10.0))
        .child(div().flex_none().text_color(theme.text).child(title))
        .child(div().flex_none().text_color(theme.faint).child(hint))
        .child(div().flex_1().h(px(1.0)).bg(theme.border))
}

fn tile(label: &'static str, theme: &Theme) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .p(px(20.0))
        .bg(theme.surface)
        .border_1()
        .border_color(theme.border)
        .child(div().text_color(theme.faint).child(label))
}

fn big(text: impl Into<SharedString>) -> Div {
    div().text_size(px(28.0)).line_height(px(36.0)).truncate().child(text.into())
}

impl Onboarding {
    fn welcome(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = ui::theme(cx);
        let time = self.shown.elapsed().as_millis();
        let fade = (time.saturating_sub(ACTIONS_MS) as f32 / 400.0).min(1.0);
        let link = |id: &'static str, question: &'static str, action: &'static str| {
            div().id(id).flex().gap(px(8.0)).child(div().text_color(theme.faint).child(question)).child(
                div().text_color(theme.muted).underline().hover(|style| style.text_color(theme.accent)).child(action),
            )
        };
        let bar = footer(false, &theme)
            .opacity(fade)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .when_some(self.error.clone(), |this, error| this.child(div().text_color(theme.red).child(error)))
                    .child(
                        link("import-backup", "moving from another machine?", "import your settings")
                            .on_click(cx.listener(|view, _, window, cx| view.import_backup(window, cx))),
                    )
                    .when(self.legacy, |this| {
                        this.child(
                            link("import-legacy", "used Nucli 1 before?", "import its settings")
                                .on_click(cx.listener(|view, _, _, cx| view.import_legacy(cx))),
                        )
                    }),
            )
            .child(
                primary("start-setup", "[enter] next", &theme)
                    .on_click(cx.listener(|view, _, window, cx| view.next(window, cx))),
            );
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(div().flex_1().flex().items_center().justify_center().child(logo(time, &theme)))
            .when(time >= ACTIONS_MS, |this| this.child(bar))
            .into_any_element()
    }

    fn agents_list(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = ui::theme(cx);
        let time = self.shown.elapsed().as_millis();
        let rows = self.agents.iter().enumerate().map(|(index, (profile, on))| {
            let (glyph, color) = icons::agent(&agents::command_name(&profile.executable));
            let detail = if profile.config_dir.is_empty() { String::new() } else { profile.config_dir.clone() };
            choice(("agent", index), *on, &theme)
                .child(div().flex_none().w(px(16.0)).text_color(color.color(theme.dark)).child(glyph))
                .child(div().flex_none().w(px(180.0)).truncate().child(profile.name.clone()))
                .child(
                    div().flex_none().w(px(120.0)).truncate().text_color(theme.muted).child(profile.executable.clone()),
                )
                .child(div().min_w_0().truncate().text_color(theme.faint).child(detail))
                .on_click(cx.listener(move |view, _, _, cx| {
                    if let Some((_, on)) = view.agents.get_mut(index) {
                        *on = !*on;
                    }
                    cx.notify();
                }))
                .into_any_element()
        });
        list(
            rows.collect(),
            self.detecting,
            [
                "looking for agent CLIs in your PATH",
                "No agent CLIs found. Install one (Claude Code, Codex, opencode, pi, …) and detect again.",
                "You can add agents by hand later.",
            ],
            time,
            &theme,
            cx,
        )
    }

    fn shells_list(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = ui::theme(cx);
        let time = self.shown.elapsed().as_millis();
        let default = self.shells.iter().position(|(_, on)| *on);
        let rows = self.shells.iter().enumerate().map(|(index, (shell, on))| {
            choice(("shell", index), *on, &theme)
                .child(div().flex_none().w(px(16.0)).text_color(theme.muted).child(icons::TERMINAL))
                .child(div().flex_none().w(px(180.0)).truncate().child(shell.name.clone()))
                .child(div().flex_1().min_w_0().truncate().text_color(theme.muted).child(shell.path.clone()))
                .when(default == Some(index), |this| {
                    this.child(div().flex_none().text_color(theme.faint).child("default"))
                })
                .on_click(cx.listener(move |view, _, _, cx| {
                    if let Some((_, on)) = view.shells.get_mut(index) {
                        *on = !*on;
                    }
                    cx.notify();
                }))
                .into_any_element()
        });
        list(
            rows.collect(),
            self.detecting,
            ["looking for shells", "No shells found.", "You can add shells by hand later."],
            time,
            &theme,
            cx,
        )
    }

    fn ready(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = ui::theme(cx);
        let current = settings::get(cx).clone();
        let theme_name = crate::theme::all()
            .into_iter()
            .find(|(id, _)| *id == current.theme)
            .map(|(_, name)| name.to_string())
            .unwrap_or(current.theme.clone());
        let agents: Vec<&AgentProfile> = self.agents.iter().filter(|(_, on)| *on).map(|(profile, _)| profile).collect();
        let shells: Vec<&ShellProfile> = self.shells.iter().filter(|(_, on)| *on).map(|(shell, _)| shell).collect();
        let swatches = [theme.accent, theme.red, theme.green, theme.yellow, theme.blue, theme.muted];
        let plural = |count: usize, word: &str| format!("{count} {word}{}", if count == 1 { "" } else { "s" });
        let more =
            |count: usize| (count > 4).then(|| div().text_color(theme.faint).child(format!("+{} more", count - 4)));
        let appearance = tile("theme", &theme)
            .child(big(theme_name))
            .child(div().flex().gap(px(4.0)).children(swatches.map(|color| div().size(px(22.0)).bg(color))))
            .child(div().text_color(theme.muted).child(format!("{:?} mode", current.appearance).to_lowercase()));
        let terminal = tile("terminal", &theme)
            .child(big(current.font.clone()).font_family(current.font.clone()))
            .child(div().font_family(current.font.clone()).text_color(theme.muted).child("fn main() => 0O1lI {}"))
            .child(div().text_color(theme.muted).child(format!(
                "{} pt, {} cursor{}",
                current.font_size(),
                format!("{:?}", current.cursor).to_lowercase(),
                if current.cursor_blink { ", blinking" } else { "" }
            )));
        let agent_tile = tile("agents", &theme)
            .child(big(plural(agents.len(), "agent")))
            .children(agents.iter().take(4).map(|profile| {
                let (glyph, color) = icons::agent(&agents::command_name(&profile.executable));
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(div().w(px(16.0)).text_color(color.color(theme.dark)).child(glyph))
                    .child(div().truncate().child(profile.name.clone()))
            }))
            .children(more(agents.len()))
            .when(agents.is_empty(), |this| {
                this.child(div().text_color(theme.muted).child("add one later in Settings"))
            });
        let shell_tile = tile("shells", &theme)
            .child(big(shells.first().map_or("none".to_string(), |shell| shell.name.clone())))
            .child(div().text_color(theme.muted).child(if shells.is_empty() {
                "add one later in Settings"
            } else {
                "default shell"
            }))
            .when(shells.len() > 1, |this| {
                this.child(
                    div()
                        .text_color(theme.faint)
                        .child(shells[1..].iter().map(|shell| shell.name.as_str()).collect::<Vec<_>>().join(", ")),
                )
            });
        let imported = self.import.as_ref().map(|import| {
            let mut parts = vec!["settings"];
            if import.keybindings.is_some() {
                parts.push("keybindings");
            }
            if import.state.is_some() {
                parts.push("workspaces and projects");
            }
            div()
                .flex()
                .gap(px(8.0))
                .px(px(12.0))
                .py(px(6.0))
                .border_1()
                .border_color(theme.border)
                .child(div().text_color(theme.green).child(icons::CHECK))
                .child(format!("imported {} from {}", parts.join(", "), import.source))
                .into_any_element()
        });
        imported
            .into_iter()
            .chain([div()
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(div().flex_1().flex().gap(px(8.0)).child(appearance).child(terminal))
                .child(div().flex_1().flex().gap(px(8.0)).child(agent_tile).child(shell_tile))
                .into_any_element()])
            .collect()
    }

    fn steps(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = ui::theme(cx);
        let current = settings::get(cx).clone();
        let (step, title, description) = STEPS[index];
        let body = match step {
            Step::Appearance => vec![
                section("Theme", "colors", &theme).into_any_element(),
                settings_view::theme_cards(&current, ui::system_dark(cx), &theme).into_any_element(),
                section("Mode", "light or dark", &theme).mt(px(12.0)).into_any_element(),
                mode_tiles(&current, &theme).into_any_element(),
            ],
            Step::Terminal => {
                let time = self.shown.elapsed().as_millis();
                let mut body = vec![terminal_preview(&current, time, &theme).into_any_element()];
                body.extend(settings_view::terminal(&current, self.font_picker.clone().into_any_element(), &theme));
                body
            }
            Step::Agents => self.agents_list(cx),
            Step::Shells => self.shells_list(cx),
            Step::Ready | Step::Welcome => self.ready(cx),
        };
        let chips = STEPS.iter().enumerate().map(|(position, (step, title, _))| {
            let step = *step;
            let active = position == index;
            div()
                .id(("step", position))
                .flex()
                .gap(px(6.0))
                .px(px(6.0))
                .when(active, |this| this.bg(theme.accent).text_color(theme.on_accent))
                .when(!active && position < index, |this| {
                    this.text_color(theme.muted).hover(|style| style.text_color(theme.text))
                })
                .when(position > index, |this| this.text_color(theme.faint).hover(|style| style.text_color(theme.text)))
                .child(if position < index { icons::CHECK.to_string() } else { format!("{}", position + 1) })
                .child(*title)
                .on_click(cx.listener(move |view, _, _, cx| view.go(step, cx)))
        });
        let last = index + 1 == STEPS.len();
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .id("onboarding-content")
                    .flex_1()
                    .min_h_0()
                    .when(step != Step::Ready, |this| this.overflow_y_scroll())
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(14.0))
                            .when(step == Step::Ready, |this| this.size_full())
                            .px(px(40.0))
                            .pt(px(if cfg!(target_os = "macos") { 56.0 } else { 32.0 }))
                            .pb(px(32.0))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .child(div().text_color(theme.faint).child(format!(
                                        "{:02} / {:02}",
                                        index + 1,
                                        STEPS.len()
                                    )))
                                    .child(div().text_size(px(22.0)).line_height(px(32.0)).child(title))
                                    .child(div().text_color(theme.muted).child(description)),
                            )
                            .child(div().flex_1().flex().flex_col().gap(px(12.0)).children(body).with_animation(
                                ("step-body", index),
                                Animation::new(Duration::from_millis(220)).with_easing(gpui::ease_out_quint()),
                                |this, delta| this.opacity(delta).relative().top(px((1.0 - delta) * 10.0)),
                            )),
                    ),
            )
            .child(
                footer(true, &theme).child(div().flex().gap(px(4.0)).children(chips)).child(
                    div()
                        .flex()
                        .flex_none()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            settings_view::button("back", "[esc] back", &theme)
                                .on_click(cx.listener(|view, _, _, cx| view.back(cx))),
                        )
                        .child(
                            primary("next", if last { "[enter] start" } else { "[enter] next" }, &theme)
                                .on_click(cx.listener(|view, _, window, cx| view.next(window, cx))),
                        ),
                ),
            )
            .into_any_element()
    }
}

fn choice(id: (&'static str, usize), on: bool, theme: &Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(10.0))
        .px(px(12.0))
        .py(px(6.0))
        .border_b_1()
        .border_color(theme.border)
        .when(!on, |this| this.text_color(theme.faint))
        .hover(|style| style.bg(theme.hover))
        .child(div().flex_none().text_color(if on { theme.accent } else { theme.muted }).child(if on {
            "[x]"
        } else {
            "[ ]"
        }))
}

fn list(
    rows: Vec<AnyElement>,
    detecting: bool,
    [looking, empty, later]: [&'static str; 3],
    time: u128,
    theme: &Theme,
    cx: &mut Context<Onboarding>,
) -> Vec<AnyElement> {
    let status = if detecting {
        Some(
            div()
                .flex()
                .gap(px(8.0))
                .text_color(theme.muted)
                .child(div().text_color(theme.accent).child(spinner(time)))
                .child(looking)
                .into_any_element(),
        )
    } else if rows.is_empty() {
        Some(div().text_color(theme.muted).child(empty).into_any_element())
    } else {
        None
    };
    let mut body = Vec::new();
    if !rows.is_empty() {
        body.push(settings_view::group(theme).children(rows).into_any_element());
    }
    body.extend(status);
    body.push(
        div()
            .flex()
            .items_center()
            .gap(px(12.0))
            .child(
                settings_view::button("detect", "detect again", theme)
                    .on_click(cx.listener(|view, _, _, cx| view.detect(true, cx))),
            )
            .child(div().text_color(theme.faint).child(later))
            .into_any_element(),
    );
    body
}

impl Render for Onboarding {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = ui::theme(cx);
        let ui_size = settings::get(cx).ui_font_size();
        let content = match position(self.step) {
            Some(index) => self.steps(index, cx),
            None => self.welcome(cx),
        };
        div()
            .id("onboarding")
            .key_context("Onboarding")
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .bg(theme.background)
            .text_color(theme.text)
            .font_family(FONT)
            .text_size(px(ui_size))
            .line_height(px((ui_size * 1.5).round()))
            .on_key_down(cx.listener(Self::key_down))
            .child(content)
            .when(cfg!(target_os = "macos"), |this| {
                this.child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .w_full()
                        .h(px(38.0))
                        .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move()),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_a_backup_folder() {
        let folder = std::env::temp_dir().join(format!("nucli-backup-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("settings.json"), r#"{"theme":"vitesse-dark"}"#).unwrap();
        std::fs::write(folder.join("keybindings.json"), "[]").unwrap();
        std::fs::write(folder.join("state.json"), r#"{"workspaces":[]}"#).unwrap();
        let (settings, import) = super::read_backup(&folder).unwrap();
        assert_eq!(settings.theme, "vitesse-dark");
        assert_eq!(import.keybindings.as_deref(), Some("[]"));
        assert!(import.state.is_some());
        std::fs::remove_file(folder.join("settings.json")).unwrap();
        assert!(super::read_backup(&folder).is_err_and(|error| error.starts_with("no settings.json")));
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
