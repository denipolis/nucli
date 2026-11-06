use std::time::{Duration, Instant};

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Direction, Line, Point as GridPoint};
use alacritty_terminal::term::search::{RegexIter, RegexSearch};
use gpui::{
    AnyElement, AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::icons;
use crate::session::Session;
use crate::text_input::{InputEvent, TextInput};
use crate::ui;

const MAX_MATCHES: usize = 2000;

pub struct Find {
    pub input: Entity<TextInput>,
    pub matches: Vec<(GridPoint, GridPoint)>,
    pub current: usize,
    pub searched: Instant,
}

pub fn escape(query: &str) -> String {
    let mut out = String::with_capacity(query.len());
    for character in query.chars() {
        if "\\.+*?()|[]{}^$#&-~".contains(character) {
            out.push('\\');
        }
        out.push(character);
    }
    out
}

impl Session {
    pub fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(find) = &self.view.find {
            find.input.read(cx).focus(window);
            return;
        }
        let selected = self.terminal.selection_text().filter(|text| !text.contains('\n'));
        let input = cx.new(|cx| {
            let mut input = TextInput::new("find", cx);
            if let Some(text) = &selected {
                input.set_text(text, cx);
            }
            input
        });
        cx.subscribe_in(&input, window, |session, _, event, window, cx| match event {
            InputEvent::Changed => session.run_search(true, cx),
            InputEvent::Submit | InputEvent::Down => session.find_step(1, cx),
            InputEvent::Up => session.find_step(-1, cx),
            InputEvent::Cancel => session.close_find(window, cx),
            InputEvent::NextField => {}
        })
        .detach();
        input.read(cx).focus(window);
        self.view.find = Some(Find { input, matches: Vec::new(), current: 0, searched: Instant::now() });
        self.run_search(true, cx);
    }

    pub fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.view.find = None;
        window.focus(&self.focus);
        cx.notify();
    }

    pub fn run_search(&mut self, jump: bool, cx: &mut Context<Self>) {
        let Some(find) = self.view.find.as_mut() else { return };
        let query = find.input.read(cx).text.clone();
        find.searched = Instant::now();
        find.matches.clear();
        if !query.is_empty()
            && let Ok(mut regex) = RegexSearch::new(&escape(&query))
        {
            let term = self.terminal.term.lock();
            let grid = term.grid();
            let top = GridPoint::new(Line(-(grid.history_size() as i32)), Column(0));
            let bottom = GridPoint::new(Line(grid.screen_lines() as i32 - 1), Column(grid.columns() - 1));
            find.matches = RegexIter::new(top, bottom, Direction::Right, &*term, &mut regex)
                .take(MAX_MATCHES)
                .map(|found| (*found.start(), *found.end()))
                .collect();
        }
        if find.current >= find.matches.len() {
            find.current = find.matches.len().saturating_sub(1);
        }
        if jump {
            find.current = find.matches.len().saturating_sub(1);
            self.reveal_match();
        }
        cx.notify();
    }

    pub fn refresh_search(&mut self, cx: &mut Context<Self>) {
        let due = self.view.find.as_ref().is_some_and(|find| find.searched.elapsed() > Duration::from_millis(250));
        if due {
            self.run_search(false, cx);
        }
    }

    pub fn find_step(&mut self, delta: i32, cx: &mut Context<Self>) {
        let Some(find) = self.view.find.as_mut() else { return };
        let count = find.matches.len() as i32;
        if count == 0 {
            return;
        }
        find.current = (find.current as i32 + delta).rem_euclid(count) as usize;
        self.reveal_match();
        cx.notify();
    }

    fn reveal_match(&mut self) {
        let Some(find) = self.view.find.as_ref() else { return };
        if let Some((start, _)) = find.matches.get(find.current) {
            self.terminal.term.lock().scroll_to_point(*start);
        }
    }

    pub fn find_bar(&self, find: &Find, cx: &mut Context<Self>) -> AnyElement {
        let theme = ui::theme(cx);
        let label = if find.matches.is_empty() {
            "0/0".to_string()
        } else {
            format!(
                "{}/{}{}",
                find.current + 1,
                find.matches.len(),
                if find.matches.len() == MAX_MATCHES { "+" } else { "" }
            )
        };
        let button = |id: &'static str, glyph: &'static str| {
            div().id(id).px(px(4.0)).text_color(theme.muted).hover(|style| style.text_color(theme.accent)).child(glyph)
        };
        div()
            .absolute()
            .top(px(4.0))
            .right(px(12.0))
            .w(px(320.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .px(px(6.0))
            .bg(theme.elevated)
            .border_1()
            .border_color(theme.accent)
            .font_family(crate::settings::FONT)
            .text_size(px(crate::settings::get(cx).ui_font_size()))
            .child(div().flex_1().min_w_0().child(find.input.clone()))
            .child(div().flex_none().text_color(theme.muted).child(label))
            .child(
                button("find-previous", icons::ARROW_UP)
                    .on_click(cx.listener(|session, _, _, cx| session.find_step(-1, cx))),
            )
            .child(
                button("find-next", icons::ARROW_DOWN)
                    .on_click(cx.listener(|session, _, _, cx| session.find_step(1, cx))),
            )
            .child(
                button("find-close", icons::CLOSE)
                    .on_click(cx.listener(|session, _, window, cx| session.close_find(window, cx))),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_regex_syntax() {
        assert_eq!(escape("a.b(c)"), "a\\.b\\(c\\)");
        assert_eq!(escape("plain text"), "plain text");
    }
}
