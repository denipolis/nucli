use std::path::{Path, PathBuf};

use gpui::{
    AnyElement, App, AppContext, Bounds, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    MouseButton, ParentElement, Pixels, Point, Render, SharedString, StatefulInteractiveElement, Styled, Subscription,
    TitlebarOptions, Window, WindowBounds, WindowHandle, WindowOptions, deferred, div, point, px, size,
};

use crate::icons;
use crate::menu::{self, Menu};
use crate::palette::{Palette, PaletteEvent};
use crate::persist::{Selected, Side};
use crate::session::Session;
use crate::settings::{self, FONT};
use crate::store::{self, Store, Workspace};
use crate::text_input::{InputEvent, TextInput};
use crate::ui;

pub enum Rename {
    Project(PathBuf),
    Tab(String),
}

pub struct WorkspaceView {
    pub workspace: String,
    pub focus: FocusHandle,
    pub filter: Entity<TextInput>,
    pub rename: Option<(Rename, Entity<TextInput>)>,
    pub menu: Option<Menu>,
    pub palette: Option<Entity<Palette>>,
    pub prompt_text: String,
    pub switcher: Option<crate::switcher::Switcher>,
    pub panel: crate::panel::PanelState,
    pub files: crate::file_view::Files,
    pub resizing: Option<crate::resize::Drag>,
    _subscriptions: Vec<Subscription>,
}

pub fn open(workspace: &str, cx: &mut App) -> Option<WindowHandle<WorkspaceView>> {
    let store = store::store(cx);
    if let Some(handle) = store.read(cx).windows.get(workspace).copied() {
        let _ = handle.update(cx, |_, window, _| window.activate_window());
        return Some(handle);
    }
    let state = store.read(cx).workspace(workspace)?.clone();
    let reopen = state.closed;
    store.update(cx, |store, cx| {
        if let Some(target) = store.workspace_mut(workspace) {
            target.closed = false;
        }
        store.restore(workspace, cx);
    });
    let bounds = state
        .window
        .bounds
        .map(|[x, y, w, h]| Bounds::new(point(px(x), px(y)), size(px(w), px(h))))
        .unwrap_or_else(|| Bounds::centered(None, size(px(1280.0), px(800.0)), cx));
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some(state.name.clone().into()),
            appears_transparent: true,
            traffic_light_position: Some(point(px(10.0), px(9.0))),
        }),
        app_id: Some("uk.polishch.nucli".into()),
        window_min_size: Some(size(px(480.0), px(320.0))),
        ..Default::default()
    };
    let id = workspace.to_string();
    let handle = cx.open_window(options, |window, cx| cx.new(|cx| WorkspaceView::new(id, window, cx))).ok()?;
    store.update(cx, |store, cx| {
        store.windows.insert(workspace.to_string(), handle);
        store.last_workspace = Some(workspace.to_string());
        if reopen {
            store.changed(cx);
        }
    });
    let _ = handle.update(cx, |view, window, cx| view.focus_current(window, cx));
    Some(handle)
}

impl WorkspaceView {
    pub fn new(workspace: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        ui::watch_appearance(window, cx);
        let store = store::store(cx);
        let filter = cx.new(|cx| TextInput::new("filter", cx));
        let subscriptions = vec![
            cx.observe(&store, |_, _, cx| cx.notify()),
            cx.observe_global::<settings::Settings>(|_, cx| cx.notify()),
            cx.subscribe_in(&filter, window, |view, _, event, window, cx| match event {
                InputEvent::Cancel => {
                    view.filter.update(cx, |input, cx| input.clear(cx));
                    view.focus_current(window, cx);
                }
                InputEvent::Submit => view.select_first_match(window, cx),
                _ => cx.notify(),
            }),
            cx.observe_window_bounds(window, |view, window, cx| {
                let bounds = window.bounds();
                let rect = [
                    f32::from(bounds.origin.x),
                    f32::from(bounds.origin.y),
                    f32::from(bounds.size.width),
                    f32::from(bounds.size.height),
                ];
                let id = view.workspace.clone();
                store::store(cx).update(cx, |store, cx| {
                    if let Some(target) = store.workspace_mut(&id) {
                        target.window.bounds = Some(rect);
                        store.schedule_save(cx);
                    }
                });
            }),
            cx.observe_window_activation(window, |view, window, cx| {
                if window.is_window_active() {
                    let id = view.workspace.clone();
                    let shown = view.visible_tabs(cx);
                    store::store(cx).update(cx, |store, cx| {
                        store.last_workspace = Some(id);
                        for tab in shown {
                            if let Some(session) = store.sessions.get(&tab) {
                                session.update(cx, |session, cx| {
                                    if session.attention {
                                        session.attention = false;
                                        cx.notify();
                                    }
                                });
                            }
                        }
                        store.update_badge(cx);
                    });
                }
            }),
        ];
        let id = workspace.clone();
        window.on_window_should_close(cx, move |window, cx| {
            crate::actions::close_workspace_window(&id, window, cx);
            false
        });
        WorkspaceView {
            workspace,
            focus: cx.focus_handle(),
            filter,
            rename: None,
            menu: None,
            palette: None,
            prompt_text: String::new(),
            switcher: None,
            panel: Default::default(),
            files: Default::default(),
            resizing: None,
            _subscriptions: subscriptions,
        }
    }

    pub fn state<'a>(&self, cx: &'a App) -> Option<&'a Workspace> {
        store::store(cx).read(cx).workspace(&self.workspace)
    }

    pub fn store<'a>(&self, cx: &'a App) -> &'a Store {
        cx.global::<store::StoreHandle>().0.read(cx)
    }

    pub fn current_project_path(&self, cx: &App) -> Option<PathBuf> {
        self.state(cx)?.current_project().map(|project| project.path.clone())
    }

    pub fn selected_session(&self, cx: &App) -> Option<Entity<Session>> {
        let id = self.state(cx)?.selected_tab()?.to_string();
        self.store(cx).session(&id).cloned()
    }

    pub fn visible_tabs(&self, cx: &App) -> Vec<String> {
        let Some(state) = self.state(cx) else { return Vec::new() };
        let mut tabs: Vec<String> = match state.selected_tab() {
            Some(id) => match crate::splits::group(&state.layouts, id) {
                Some(index) => crate::splits::tabs(&state.layouts[index]),
                None => vec![id.to_string()],
            },
            None => Vec::new(),
        };
        if let Some(project) = state.current_project()
            && project.terminal_shown
            && let Some(terminal) = &project.active_terminal
        {
            tabs.push(terminal.clone());
        }
        tabs
    }

    pub fn shows(&self, id: &str, cx: &App) -> bool {
        self.visible_tabs(cx).iter().any(|tab| tab == id) && !self.file_shown(cx)
    }

    pub fn file_shown(&self, cx: &App) -> bool {
        self.state(cx).and_then(|state| state.current_project()).is_some_and(|project| {
            project.file_shown
                && project.active_file.is_some()
                && matches!(self.state(cx).and_then(|s| s.selected.clone()), Some(Selected::Project(_)))
        })
    }

    pub fn focus_current(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(session) = self.selected_session(cx) {
            let focus = session.read(cx).focus.clone();
            window.focus(&focus);
        } else if self.file_shown(cx) {
            crate::file_view::focus_active(self, window, cx);
        } else {
            window.focus(&self.focus);
        }
    }

    pub fn select(&mut self, selected: Option<Selected>, window: &mut Window, cx: &mut Context<Self>) {
        let id = self.workspace.clone();
        store::store(cx).update(cx, |store, cx| store.select(&id, selected, cx));
        if let Some(path) = self.current_project_path(cx) {
            let path = path.clone();
            store::store(cx).update(cx, |store, cx| store.refresh_conversations(path, cx));
        }
        self.focus_current(window, cx);
        cx.notify();
    }

    pub fn select_tab(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.select(Some(Selected::Tab(id.to_string())), window, cx);
    }

    pub fn open_menu(&mut self, position: Point<Pixels>, items: Vec<menu::Item>, cx: &mut Context<Self>) {
        self.menu = Some(Menu { position, items });
        cx.notify();
    }

    pub fn show_palette(&mut self, palette: Entity<Palette>, window: &mut Window, cx: &mut Context<Self>) {
        cx.subscribe_in(&palette, window, |view, _, event, window, cx| match event {
            PaletteEvent::Confirm(item, alternate) => {
                let run =
                    if *alternate { item.alternate.clone().unwrap_or(item.run.clone()) } else { item.run.clone() };
                view.palette = None;
                view.focus_current(window, cx);
                run(view, window, cx);
                cx.notify();
            }
            PaletteEvent::Preview(item) => {
                if let Some(preview) = item.preview.clone() {
                    preview(view, window, cx);
                }
            }
            PaletteEvent::Text(text) => view.prompt_text = text.clone(),
            PaletteEvent::Dismiss => view.dismiss_palette(window, cx),
        })
        .detach();
        let focus = palette.read(cx).input.read(cx).focus_handle(cx);
        window.focus(&focus);
        self.palette = Some(palette);
        cx.notify();
    }

    pub fn dismiss_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.take().is_some() {
            crate::actions::end_theme_preview(cx);
            self.focus_current(window, cx);
            cx.notify();
        }
    }

    fn select_first_match(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let query = self.filter.read(cx).text.clone();
        if let Some(target) = crate::sidebar::first_match(self, &query, cx) {
            self.filter.update(cx, |input, cx| input.clear(cx));
            self.select(Some(target), window, cx);
        }
    }

    pub fn open_path(&mut self, path: &Path, line: Option<usize>, window: &mut Window, cx: &mut Context<Self>) {
        crate::file_view::open_path(self, path, line, window, cx);
    }

    fn titlebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = ui::theme(cx);
        let state = self.state(cx);
        let name = state.map(|state| state.name.clone()).unwrap_or_default();
        let location = match state.and_then(|state| state.current_project()) {
            Some(project) if !project.is_scratch() => {
                let branch = crate::git::current_branch(&project.path)
                    .map(|branch| format!("  {} {branch}", icons::BRANCH))
                    .unwrap_or_default();
                format!("{}  {}{branch}", project.display_name(), crate::paths::display(&project.path))
            }
            _ => "no project".to_string(),
        };
        let height = px((settings::get(cx).ui_font_size() * 2.4).round());
        let toggle = |id: &'static str, glyph: &'static str, active: bool, command: &'static str| {
            div()
                .id(id)
                .px(px(6.0))
                .text_color(if active { theme.text } else { theme.faint })
                .hover(|style| style.text_color(theme.accent))
                .child(glyph)
                .on_click(move |_, window, cx| {
                    if let Some(command) = crate::commands::find(command) {
                        window.dispatch_action((command.action)(), cx);
                    }
                })
        };
        let sidebar_shown = state.is_some_and(|state| !state.window.sidebar_hidden);
        let panel_shown = !self.store(cx).ui.panel_hidden;
        let terminal_shown =
            state.and_then(|state| state.current_project()).is_some_and(|project| project.terminal_shown);
        div()
            .id("titlebar")
            .flex()
            .flex_none()
            .items_center()
            .h(height)
            .pl(px(if cfg!(target_os = "macos") { 78.0 } else { 8.0 }))
            .pr(px(6.0))
            .gap(px(10.0))
            .bg(theme.surface)
            .border_b_1()
            .border_color(theme.border)
            .on_click(|event, window, _| {
                if event.click_count() == 2 {
                    window.zoom_window();
                }
            })
            .child(
                div()
                    .id("workspace-name")
                    .px(px(6.0))
                    .bg(theme.accent)
                    .text_color(theme.on_accent)
                    .child(ui::caps(&name))
                    .on_click(cx.listener(|view, event: &gpui::ClickEvent, window, cx| {
                        let position = event.position();
                        let items = crate::actions::workspace_menu(view, cx);
                        view.open_menu(position, items, cx);
                        let _ = window;
                    })),
            )
            .child(
                div()
                    .id("location")
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(theme.muted)
                    .hover(|style| style.text_color(theme.text))
                    .child(location)
                    .on_click(cx.listener(|view, _, window, cx| crate::actions::command_palette(view, window, cx))),
            )
            .child(toggle("toggle-sidebar", icons::SIDEBAR, sidebar_shown, "toggle_sidebar"))
            .child(toggle("toggle-panel", icons::LAYOUT, panel_shown, "toggle_panel"))
            .child(toggle("toggle-terminal", icons::PANEL, terminal_shown, "toggle_terminal"))
    }

    fn viewport(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(state) = self.state(cx).cloned() else { return div().into_any_element() };
        if self.file_shown(cx) {
            return crate::file_view::render(self, window, cx);
        }
        match &state.selected {
            Some(Selected::Tab(id)) => match crate::splits::group(&state.layouts, id) {
                Some(index) => crate::splits_view::render(self, &state.layouts[index], id, cx),
                None => match self.store(cx).session(id) {
                    Some(session) => div().size_full().child(session.clone()).into_any_element(),
                    None => crate::overview::empty(self, cx),
                },
            },
            Some(Selected::Project(path)) => crate::overview::project(self, path, cx),
            None => crate::overview::empty(self, cx),
        }
    }
}

impl Focusable for WorkspaceView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for WorkspaceView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = ui::theme(cx);
        let ui_size = settings::get(cx).ui_font_size();
        let Some(state) = self.state(cx).cloned() else { return div().into_any_element() };
        let ui_state = self.store(cx).ui.clone();
        let viewport = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(div().flex_1().min_h_0().child(self.viewport(window, cx)))
            .children(crate::panel::terminal_panel(self, window, cx));
        let panel = (!ui_state.panel_hidden).then(|| crate::panel::render(self, window, cx));
        let mut body = div().flex().flex_row().flex_1().min_h_0().overflow_hidden();
        if !state.window.sidebar_hidden {
            body = body
                .child(crate::sidebar::render(self, window, cx))
                .child(crate::resize::handle_horizontal(crate::resize::Drag::Sidebar, cx));
        }
        let panel_handle = panel.is_some().then(|| crate::resize::handle_horizontal(crate::resize::Drag::Panel, cx));
        match ui_state.panel_side {
            Side::Left => body = body.children(panel).children(panel_handle).child(viewport),
            Side::Right => body = body.child(viewport).children(panel_handle).children(panel),
        }
        let mut root = div()
            .id("workspace")
            .key_context("Workspace")
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.background)
            .text_color(theme.text)
            .font_family(FONT)
            .text_size(px(ui_size))
            .line_height(px((ui_size * 1.5).round()))
            .on_drop(cx.listener(|view, paths: &gpui::ExternalPaths, window, cx| {
                crate::actions::drop_folders(view, paths, window, cx)
            }))
            .on_mouse_move(cx.listener(crate::resize::mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(|view, _, _, cx| crate::resize::mouse_up(view, cx)))
            .on_modifiers_changed(cx.listener(crate::switcher::modifiers_changed))
            .capture_key_down(cx.listener(|view, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" && event.keystroke.modifiers == gpui::Modifiers::default() {
                    if view.switcher.take().is_some() || view.menu.take().is_some() {
                        cx.stop_propagation();
                        view.focus_current(window, cx);
                        cx.notify();
                    }
                }
            }))
            .child(self.titlebar(cx))
            .child(body)
            .child(crate::statusbar::render(self, cx));
        root = crate::actions::register(root, cx);
        if let Some(menu) = &self.menu {
            root = root.child(menu::render(menu, cx));
        }
        if let Some(palette) = &self.palette {
            root = root.child(
                deferred(
                    div()
                        .id("palette-backdrop")
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .flex()
                        .justify_center()
                        .pt(px(60.0))
                        .occlude()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|view, _, window, cx| view.dismiss_palette(window, cx)),
                        )
                        .child(
                            div()
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .child(palette.clone()),
                        ),
                )
                .with_priority(1),
            );
        }
        if let Some(switcher) = &self.switcher {
            root = root.child(crate::switcher::render(self, switcher, cx));
        }
        root.into_any_element()
    }
}

pub fn label(text: &str) -> SharedString {
    SharedString::from(text.to_string())
}
