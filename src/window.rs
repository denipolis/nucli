use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, AppContext, Bounds, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    MouseButton, ParentElement, Pixels, Point, Render, StatefulInteractiveElement, Styled, Subscription,
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
    pub pane_bounds: Rc<RefCell<HashMap<String, Bounds<Pixels>>>>,
    pub dragging_titlebar: bool,
    pub shown_tab: Option<String>,
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
            appears_transparent: cfg!(target_os = "macos"),
            traffic_light_position: Some(point(px(10.0), px(9.0))),
        }),
        app_id: Some("uk.polishch.nucli".into()),
        window_decorations: Some(gpui::WindowDecorations::Server),
        window_min_size: Some(size(px(480.0), px(320.0))),
        ..Default::default()
    };
    let id = workspace.to_string();
    let opened = cx.open_window(options, |window, cx| cx.new(|cx| WorkspaceView::new(id, window, cx)));
    let handle = match opened {
        Ok(handle) => handle,
        Err(error) => {
            let text = format!("Nucli could not open a window: {error:#}");
            eprintln!("{text}");
            crate::notify::alert(&text);
            if cx.windows().is_empty() {
                cx.quit();
            }
            return None;
        }
    };
    store.update(cx, |store, cx| {
        store.windows.insert(workspace.to_string(), handle);
        store.last_workspace = Some(workspace.to_string());
        if reopen {
            store.changed(cx);
        }
    });
    let _ = handle.update(cx, |view, window, cx| view.focus_current(window, cx));
    store.update(cx, |store, cx| {
        if store.ui.sidebar_tab == crate::persist::SidebarTab::History && store.history.is_none() {
            store.refresh_history(cx);
        }
    });
    Some(handle)
}

impl WorkspaceView {
    pub fn new(workspace: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        ui::watch_appearance(window, cx);
        let store = store::store(cx);
        let filter = cx.new(|cx| TextInput::new("filter", cx));
        let subscriptions = vec![
            cx.observe_in(&store, window, |view, store, window, cx| {
                let gone = view.shown_tab.as_ref().is_some_and(|tab| !store.read(cx).sessions.contains_key(tab));
                view.shown_tab = view.state(cx).and_then(|state| state.selected_tab()).map(String::from);
                if view.palette.is_none() && view.rename.is_none() && (gone || window.focused(cx).is_none()) {
                    view.focus_current(window, cx);
                }
                cx.notify();
            }),
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
            pane_bounds: Default::default(),
            dragging_titlebar: false,
            shown_tab: None,
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
        self.state(cx).map(Workspace::visible_tabs).unwrap_or_default()
    }

    pub fn file_shown(&self, cx: &App) -> bool {
        self.state(cx).is_some_and(Workspace::file_shown)
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

    pub fn reveal_tab(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let panel = self
            .state(cx)
            .and_then(|state| state.project_of(id))
            .filter(|project| project.terminals.iter().any(|tab| tab == id))
            .map(|project| project.path.clone());
        let Some(path) = panel else {
            self.select_tab(id, window, cx);
            return;
        };
        let workspace = self.workspace.clone();
        let tab = id.to_string();
        self.select(Some(Selected::Project(path.clone())), window, cx);
        store::store(cx).update(cx, |store, cx| {
            if let Some(project) = store.workspace_mut(&workspace).and_then(|state| state.project_mut(&path)) {
                project.active_terminal = Some(tab.clone());
                project.terminal_shown = true;
            }
            store.changed(cx);
        });
        if let Some(session) = self.store(cx).session(id) {
            let focus = session.read(cx).focus.clone();
            window.focus(&focus);
        }
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

    fn titlebar(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
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
        let client_decorated = matches!(window.window_decorations(), gpui::Decorations::Client { .. });
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
            .window_control_area(gpui::WindowControlArea::Drag)
            .on_mouse_down(MouseButton::Left, cx.listener(|view, _, _, _| view.dragging_titlebar = true))
            .on_mouse_up(MouseButton::Left, cx.listener(|view, _, _, _| view.dragging_titlebar = false))
            .on_mouse_move(cx.listener(|view, event: &gpui::MouseMoveEvent, window, _| {
                if view.dragging_titlebar && event.pressed_button == Some(MouseButton::Left) {
                    view.dragging_titlebar = false;
                    window.start_window_move();
                }
            }))
            .on_click(|event, window, _| {
                if event.click_count() == 2 {
                    if cfg!(target_os = "macos") { window.titlebar_double_click() } else { window.zoom_window() }
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
            .when(client_decorated, |this| {
                let control = |id: &'static str, glyph: &'static str| {
                    div()
                        .id(id)
                        .px(px(6.0))
                        .text_color(theme.muted)
                        .hover(|style| style.text_color(theme.accent))
                        .child(glyph)
                };
                let id = self.workspace.clone();
                this.child(control("window-minimize", icons::REMOVE).on_click(|_, window, _| window.minimize_window()))
                    .child(control("window-zoom", icons::MAXIMIZE).on_click(|_, window, _| window.zoom_window()))
                    .child(control("window-close", icons::CLOSE).on_click(move |_, window, cx| {
                        crate::actions::close_workspace_window(&id, window, cx);
                    }))
            })
    }

    fn viewport(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(state) = self.state(cx).cloned() else { return div().into_any_element() };
        if self.file_shown(cx) {
            return crate::file_view::render(self, window, cx);
        }
        match &state.selected {
            Some(Selected::Tab(id)) => match crate::splits::group(&state.layouts, id) {
                Some(index) => crate::splits_view::render(self, &state.layouts[index], id, cx),
                None if self.store(cx).session(id).is_some() => crate::splits_view::single(self, id, cx),
                None => crate::overview::empty(self, cx),
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
        let terminal = crate::panel::terminal_panel(self, window, cx);
        let content = (terminal.is_none() || !state.window.terminal_maximized).then(|| self.viewport(window, cx));
        let viewport = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .h_full()
            .children(content.map(|content| div().flex_1().min_h_0().child(content)))
            .children(terminal);
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
                if event.keystroke.key == "escape"
                    && event.keystroke.modifiers == gpui::Modifiers::default()
                    && (view.switcher.take().is_some() || view.menu.take().is_some())
                {
                    cx.stop_propagation();
                    view.focus_current(window, cx);
                    cx.notify();
                }
            }))
            .child(self.titlebar(window, cx))
            .child(body)
            .child(crate::statusbar::render(self, cx));
        root = crate::actions::register(root, cx);
        if matches!(window.window_decorations(), gpui::Decorations::Client { .. }) {
            root = root.children(resize_edges());
        }
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

fn resize_edges() -> Vec<gpui::Stateful<gpui::Div>> {
    use gpui::{CursorStyle, ResizeEdge};
    let edge = |id: &'static str, edge: ResizeEdge, cursor: CursorStyle| {
        div().id(id).absolute().occlude().cursor(cursor).on_mouse_down(MouseButton::Left, move |_, window, cx| {
            window.start_window_resize(edge);
            cx.stop_propagation();
        })
    };
    vec![
        edge("resize-top", ResizeEdge::Top, CursorStyle::ResizeUpDown).top_0().left_0().w_full().h(px(3.0)),
        edge("resize-bottom", ResizeEdge::Bottom, CursorStyle::ResizeUpDown).bottom_0().left_0().w_full().h(px(3.0)),
        edge("resize-left", ResizeEdge::Left, CursorStyle::ResizeLeftRight).top_0().left_0().h_full().w(px(3.0)),
        edge("resize-right", ResizeEdge::Right, CursorStyle::ResizeLeftRight).top_0().right_0().h_full().w(px(3.0)),
    ]
}
