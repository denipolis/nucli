use std::path::{Path, PathBuf};

use gpui::{AnyElement, App, Context, IntoElement, ParentElement, Styled, Window, div};

use crate::window::WorkspaceView;

#[derive(Default)]
pub struct Files;

impl Files {
    pub fn focus(&self, _window: &mut Window, _cx: &mut App) {}
}

pub fn render(_view: &mut WorkspaceView, _window: &mut Window, _cx: &mut Context<WorkspaceView>) -> AnyElement {
    div().size_full().child("file").into_any_element()
}

pub fn open_path(
    _view: &mut WorkspaceView,
    _path: &Path,
    _line: Option<usize>,
    _window: &mut Window,
    _cx: &mut Context<WorkspaceView>,
) {
}

pub fn open_config(_view: &mut WorkspaceView, _path: PathBuf, _window: &mut Window, _cx: &mut Context<WorkspaceView>) {}

pub fn close_active(_view: &mut WorkspaceView, _window: &mut Window, _cx: &mut Context<WorkspaceView>) {}

pub fn cursor_label(_view: &WorkspaceView, _cx: &App) -> Option<String> {
    None
}

pub fn open_file(
    _view: &mut WorkspaceView,
    _project: &Path,
    _relative: &str,
    _line: Option<usize>,
    _window: &mut Window,
    _cx: &mut Context<WorkspaceView>,
) {
}
