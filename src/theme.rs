use gpui::App;

pub use nucli_core::theme::*;

pub fn reload(cx: &mut App) {
    let themes = load(&crate::paths::themes_dir());
    if cx.try_global::<Themes>() != Some(&themes) {
        cx.set_global(themes);
    }
}
