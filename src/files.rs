use std::path::Path;

use gpui::{AnyElement, Context, IntoElement, ParentElement, Styled, Window, div, px};

use crate::ui;
use crate::window::WorkspaceView;

pub struct FilesPanel;

pub fn panel(
    _view: &mut WorkspaceView,
    _path: &Path,
    _window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) -> AnyElement {
    let theme = ui::theme(cx);
    div().p(px(10.0)).text_color(theme.faint).child("files").into_any_element()
}
