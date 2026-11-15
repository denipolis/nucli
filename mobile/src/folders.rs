use gpui::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement, SharedString, StatefulInteractiveElement,
    Styled, div, px,
};
use nucli_core::icons;
use nucli_core::theme::Theme;
use nucli_core::uplink::protocol::Response;
use nucli_core::uplink::wire::Key;

use crate::home::Home;
use crate::ui;

fn parent(path: &str) -> Option<String> {
    let trimmed = path.trim_end_matches(['/', '\\']);
    let (parent, _) = trimmed.rsplit_once(['/', '\\'])?;
    Some(match parent {
        "" => "/".to_string(),
        drive if drive.ends_with(':') => format!("{drive}\\"),
        parent => parent.to_string(),
    })
}

fn join(path: &str, name: &str) -> String {
    let separator = if path.contains('\\') && !path.contains('/') { '\\' } else { '/' };
    format!("{}{separator}{name}", path.trim_end_matches(['/', '\\']))
}

impl Home {
    pub fn folders_screen(&mut self, key: Key, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let header = self.back_bar("add project", theme, cx);
        let (path, folders) = match &self.folders {
            Some(Response::Folders { path, folders }) => (Some(path.clone()), folders.clone()),
            _ => (None, Vec::new()),
        };
        let mut list = div().flex().flex_col();
        match &self.folders {
            None => list = list.child(self.loading(theme)),
            Some(Response::Failed { reason }) => list = list.child(ui::hint(reason.clone(), theme).py(px(24.0))),
            _ => {}
        }
        if let Some(path) = path.clone() {
            list = list.child(ui::hint(path.clone(), theme).text_color(theme.faint));
            if let Some(up) = parent(&path) {
                list = list.child(
                    ui::row("folder-up", theme)
                        .child(div().flex_none().w(px(18.0)).text_color(theme.muted).child(icons::CHEVRON_UP))
                        .child(div().text_color(theme.muted).child(".."))
                        .on_click(cx.listener(move |home, _, _, cx| home.browse(key, up.clone(), cx))),
                );
            }
            if folders.is_empty() {
                list = list.child(ui::hint("No folders here.", theme));
            }
            for name in folders {
                let target = join(&path, &name);
                list = list.child(
                    ui::row(SharedString::from(format!("folder:{name}")), theme)
                        .child(div().flex_none().w(px(18.0)).text_color(theme.accent).child(icons::FOLDER))
                        .child(div().flex_1().min_w_0().truncate().child(name))
                        .child(div().flex_none().text_color(theme.faint).child(icons::CHEVRON_RIGHT))
                        .on_click(cx.listener(move |home, _, _, cx| home.browse(key, target.clone(), cx))),
                );
            }
        }
        let mut footer = div()
            .flex()
            .flex_col()
            .flex_none()
            .gap(px(8.0))
            .px(px(16.0))
            .py(px(10.0))
            .border_t_1()
            .border_color(theme.border);
        if let Some(notice) = &self.notice {
            footer = footer.child(div().text_size(px(ui::SMALL)).text_color(theme.yellow).child(notice.clone()));
        }
        if let Some(path) = path {
            let name = path.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or(&path).to_string();
            footer = footer.child(
                ui::primary("add-folder", format!("{} add {name}", icons::ADD), theme)
                    .on_click(cx.listener(move |home, _, window, cx| home.add_project(key, path.clone(), window, cx))),
            );
        }
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(header)
            .child(div().id("folders").flex_1().min_h_0().overflow_y_scroll().child(list))
            .child(footer)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walks_folders_up_and_down() {
        assert_eq!(parent("/Users/me/code").as_deref(), Some("/Users/me"));
        assert_eq!(parent("/Users").as_deref(), Some("/"));
        assert_eq!(parent("/"), None);
        assert_eq!(parent("C:\\Users\\me").as_deref(), Some("C:\\Users"));
        assert_eq!(parent("C:\\Users").as_deref(), Some("C:\\"));
        assert_eq!(parent("C:\\"), None);
        assert_eq!(join("/Users/me", "code"), "/Users/me/code");
        assert_eq!(join("/", "Users"), "/Users");
        assert_eq!(join("C:\\Users", "me"), "C:\\Users\\me");
    }
}
