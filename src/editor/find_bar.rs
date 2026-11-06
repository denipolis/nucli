use std::collections::HashMap;
use std::ops::Range;

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, AppContext, Context, Div, Entity, InteractiveElement, IntoElement, ParentElement, ScrollStrategy,
    Stateful, StatefulInteractiveElement, Styled, Window, div, px,
};

use super::{Editor, FindState, Pos, Row};
use crate::find::{self, Hit, Query};
use crate::git::LineKind;
use crate::icons;
use crate::settings;
use crate::store;
use crate::text_input::{InputEvent, TextInput};
use crate::theme::Theme;
use crate::ui;

fn hits_by_row(rows: &[Row], hits: &[Hit]) -> HashMap<usize, Vec<(usize, Range<usize>)>> {
    let mut map: HashMap<usize, Vec<(usize, Range<usize>)>> = HashMap::new();
    for (position, hit) in hits.iter().enumerate() {
        for row in hit.start.row..=hit.end.row {
            let Some(line) = rows.get(row) else { continue };
            if hit.start.row != hit.end.row && line.kind == LineKind::Removed {
                continue;
            }
            let start = if row == hit.start.row { hit.start.col } else { 0 };
            let end = if row == hit.end.row { hit.end.col } else { line.text.len() };
            map.entry(row).or_default().push((position, start..end));
        }
    }
    map
}

fn shifted(pos: Pos, old_end: Pos, new_end: Pos) -> Pos {
    if pos.row == old_end.row && pos.col >= old_end.col {
        Pos { row: new_end.row, col: pos.col - old_end.col + new_end.col }
    } else if pos.row > old_end.row {
        Pos { row: pos.row + new_end.row - old_end.row, col: pos.col }
    } else {
        pos
    }
}

fn find_options(cx: &App) -> find::Options {
    store::store(cx).read(cx).ui.find
}

fn find_toggle(
    id: &'static str,
    label: &'static str,
    hint: &'static str,
    active: bool,
    theme: &Theme,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .px(px(4.0))
        .when(active, |this| this.bg(theme.accent).text_color(theme.on_accent))
        .when(!active, |this| this.text_color(theme.muted).hover(|style| style.text_color(theme.text)))
        .tooltip(ui::tooltip(hint))
        .child(label)
}

fn find_button(id: &'static str, glyph: &'static str, hint: &'static str, theme: &Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .px(px(4.0))
        .text_color(theme.muted)
        .hover(|style| style.text_color(theme.accent))
        .tooltip(ui::tooltip(hint))
        .child(glyph)
}

impl Editor {
    pub(super) fn find_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let find = self.find.as_ref()?;
        let theme = ui::theme(cx);
        let options = find_options(cx);
        let size = settings::get(cx).ui_font_size();
        let text = find.input.read(cx).text.clone();
        let (label, label_color) = if find.invalid {
            ("invalid".to_string(), theme.red)
        } else if let Some(notice) = &find.notice {
            (notice.clone(), theme.green)
        } else if text.is_empty() {
            (String::new(), theme.muted)
        } else if find.hits.is_empty() {
            ("no results".to_string(), theme.orange)
        } else {
            (format!("{} of {}", find.current + 1, find.hits.len()), theme.muted)
        };
        let field = |input: Entity<TextInput>, invalid: bool| {
            div()
                .flex_1()
                .min_w_0()
                .px(px(4.0))
                .bg(theme.background)
                .border_1()
                .border_color(if invalid { theme.red } else { theme.border })
                .child(input)
        };
        let editable = self.is_editable();
        let top = div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .when(editable, |this| {
                this.child(
                    find_button(
                        "find-expand",
                        if find.show_replace { icons::CHEVRON_DOWN } else { icons::CHEVRON_RIGHT },
                        "Toggle Replace",
                        &theme,
                    )
                    .on_click(cx.listener(|editor, _, window, cx| editor.toggle_replace(window, cx))),
                )
            })
            .child(field(find.input.clone(), find.invalid))
            .child(find_toggle("find-case", "Aa", "Match Case", options.case, &theme).on_click(
                cx.listener(|editor, _, _, cx| editor.change_options(|options| options.case = !options.case, cx)),
            ))
            .child(find_toggle("find-word", "ab", "Match Whole Word", options.word, &theme).on_click(
                cx.listener(|editor, _, _, cx| editor.change_options(|options| options.word = !options.word, cx)),
            ))
            .child(find_toggle("find-regex", ".*", "Use Regular Expression", options.regex, &theme).on_click(
                cx.listener(|editor, _, _, cx| editor.change_options(|options| options.regex = !options.regex, cx)),
            ))
            .child(
                find_toggle("find-scope", "≡", "Find in Selection", find.scope.is_some(), &theme)
                    .on_click(cx.listener(|editor, _, _, cx| editor.toggle_scope(cx))),
            )
            .child(div().flex_none().min_w(px(70.0)).flex().justify_center().text_color(label_color).child(label))
            .child(
                find_button("find-previous", icons::ARROW_UP, "Previous Match", &theme)
                    .on_click(cx.listener(|editor, _, _, cx| editor.find_step(-1, cx))),
            )
            .child(
                find_button("find-next", icons::ARROW_DOWN, "Next Match", &theme)
                    .on_click(cx.listener(|editor, _, _, cx| editor.find_step(1, cx))),
            )
            .child(
                find_button("find-close", icons::CLOSE, "Close", &theme)
                    .on_click(cx.listener(|editor, _, window, cx| editor.close_find(window, cx))),
            );
        let bottom = (find.show_replace && editable).then(|| {
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .pl(px(20.0))
                .child(field(find.replacement.clone(), false))
                .child(find_toggle("find-preserve", "AB", "Preserve Case", options.preserve, &theme).on_click(
                    cx.listener(|editor, _, _, cx| {
                        editor.change_options(|options| options.preserve = !options.preserve, cx)
                    }),
                ))
                .child(
                    find_button("find-replace", icons::REPLACE, "Replace", &theme)
                        .on_click(cx.listener(|editor, _, _, cx| editor.replace_current(cx))),
                )
                .child(
                    find_button("find-replace-all", icons::REPLACE_ALL, "Replace All", &theme)
                        .on_click(cx.listener(|editor, _, _, cx| editor.replace_all(cx))),
                )
        });
        Some(
            div()
                .id("find-bar")
                .absolute()
                .top(px(4.0))
                .right(px(12.0))
                .w(px(540.0))
                .max_w(gpui::relative(0.95))
                .flex()
                .flex_col()
                .gap(px(4.0))
                .p(px(6.0))
                .occlude()
                .text_size(px(size))
                .line_height(px((size * 1.5).round()))
                .bg(theme.elevated)
                .border_1()
                .border_color(theme.accent)
                .on_key_down(cx.listener(Self::find_key))
                .child(top)
                .children(bottom)
                .into_any_element(),
        )
    }

    fn find_key(&mut self, event: &gpui::KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let keys = &event.keystroke.modifiers;
        let primary = if cfg!(target_os = "macos") { keys.platform } else { keys.control };
        let chord = if cfg!(target_os = "macos") { keys.platform && keys.alt } else { keys.alt };
        if event.keystroke.key == "enter" && primary {
            self.replace_all(cx);
            cx.stop_propagation();
            return;
        }
        if !chord {
            return;
        }
        match event.keystroke.key.as_str() {
            "c" => self.change_options(|options| options.case = !options.case, cx),
            "w" => self.change_options(|options| options.word = !options.word, cx),
            "r" => self.change_options(|options| options.regex = !options.regex, cx),
            "p" => self.change_options(|options| options.preserve = !options.preserve, cx),
            "l" => self.toggle_scope(cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    pub(super) fn open_find(&mut self, replace: bool, window: &mut Window, cx: &mut Context<Self>) {
        let selection = self.selection();
        let selected = self
            .selected_text()
            .filter(|text| !text.contains('\n') && !text.is_empty() && self.focus.is_focused(window))
            .filter(|_| {
                !self.find.as_ref().is_some_and(|find| {
                    find.hits.get(find.current).is_some_and(|hit| Some((hit.start, hit.end)) == selection)
                })
            });
        let replace = replace && self.is_editable();
        if self.find.is_none() {
            self.find = Some(self.new_find(window, cx));
        }
        let Some(find) = self.find.as_mut() else { return };
        find.show_replace |= replace;
        let (input, replacement) = (find.input.clone(), find.replacement.clone());
        let text = selected.unwrap_or_else(|| input.read(cx).text.clone());
        input.update(cx, |input, cx| input.set_text(&text, cx));
        if replace && !text.is_empty() {
            let current = replacement.read(cx).text.clone();
            replacement.update(cx, |input, cx| input.set_text(&current, cx));
            replacement.read(cx).focus(window);
        } else {
            input.read(cx).focus(window);
        }
        self.search(true, cx);
    }

    fn toggle_replace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(find) = self.find.as_mut() else { return };
        find.show_replace = !find.show_replace;
        if find.show_replace {
            find.replacement.read(cx).focus(window);
        } else {
            find.input.read(cx).focus(window);
        }
        cx.notify();
    }

    fn new_find(&mut self, window: &mut Window, cx: &mut Context<Self>) -> FindState {
        let input = cx.new(|cx| TextInput::new("find", cx));
        let replacement = cx.new(|cx| TextInput::new("replace", cx));
        cx.subscribe_in(&input, window, |editor, _, event, window, cx| match event {
            InputEvent::Changed => editor.search(true, cx),
            InputEvent::Submit | InputEvent::Down => editor.find_step(1, cx),
            InputEvent::Up => editor.find_step(-1, cx),
            InputEvent::Cancel => editor.close_find(window, cx),
            InputEvent::NextField => editor.focus_find_field(true, window, cx),
        })
        .detach();
        cx.subscribe_in(&replacement, window, |editor, _, event, window, cx| match event {
            InputEvent::Changed => {}
            InputEvent::Submit => editor.replace_current(cx),
            InputEvent::Down => editor.find_step(1, cx),
            InputEvent::Up => editor.find_step(-1, cx),
            InputEvent::Cancel => editor.close_find(window, cx),
            InputEvent::NextField => editor.focus_find_field(false, window, cx),
        })
        .detach();
        FindState {
            input,
            replacement,
            show_replace: false,
            hits: Vec::new(),
            by_row: HashMap::new(),
            current: 0,
            invalid: false,
            scope: None,
            notice: None,
        }
    }

    fn focus_find_field(&mut self, to_replacement: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(find) = self.find.as_ref() else { return };
        if to_replacement && find.show_replace {
            find.replacement.read(cx).focus(window);
        } else {
            find.input.read(cx).focus(window);
        }
    }

    pub(super) fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.find = None;
        window.focus(&self.focus);
        cx.notify();
    }

    fn change_options(&mut self, change: impl FnOnce(&mut find::Options), cx: &mut Context<Self>) {
        store::store(cx).update(cx, |store, cx| {
            change(&mut store.ui.find);
            store.changed(cx);
        });
        self.search(true, cx);
    }

    fn toggle_scope(&mut self, cx: &mut Context<Self>) {
        let selection = self.selection();
        let Some(find) = self.find.as_mut() else { return };
        find.scope = if find.scope.is_some() { None } else { selection };
        self.search(false, cx);
    }

    pub(super) fn search(&mut self, reveal: bool, cx: &mut Context<Self>) {
        let options = find_options(cx);
        let start = self.selection().map_or(self.cursor, |(start, _)| start);
        let Some(find) = self.find.as_mut() else { return };
        let text = find.input.read(cx).text.clone();
        let mut hits = Vec::new();
        find.invalid = false;
        find.notice = None;
        if !text.is_empty() {
            match Query::new(&text, options) {
                Ok(query) => query.walk(&self.rows, |hit, _| {
                    if find.scope.is_none_or(|(from, to)| hit.start >= from && hit.end <= to) {
                        hits.push(hit);
                    }
                }),
                Err(_) => find.invalid = true,
            }
        }
        hits.sort_by_key(|hit| hit.start);
        find.by_row = hits_by_row(&self.rows, &hits);
        find.hits = hits;
        find.current = if reveal {
            find.hits.iter().position(|hit| hit.start >= start).unwrap_or(0)
        } else {
            find.current.min(find.hits.len().saturating_sub(1))
        };
        if reveal {
            self.reveal_found(cx);
        }
        cx.notify();
    }

    pub(super) fn find_step(&mut self, delta: i32, cx: &mut Context<Self>) {
        let Some(find) = self.find.as_mut() else { return };
        let count = find.hits.len() as i32;
        find.notice = None;
        if count > 0 {
            find.current = (find.current as i32 + delta).rem_euclid(count) as usize;
            self.reveal_found(cx);
        }
    }

    fn reveal_found(&mut self, cx: &mut Context<Self>) {
        if let Some(hit) = self.find.as_ref().and_then(|find| find.hits.get(find.current)).copied() {
            self.anchor = Some(hit.start);
            self.cursor = hit.end;
            self.scroll.scroll_to_item(hit.start.row, ScrollStrategy::Center);
        }
        cx.notify();
    }

    fn replacements(&self, cx: &App) -> Vec<(Hit, String)> {
        let Some(find) = self.find.as_ref() else { return Vec::new() };
        let options = find_options(cx);
        let template = find.replacement.read(cx).text.clone();
        let Ok(query) = Query::new(&find.input.read(cx).text, options) else { return Vec::new() };
        let mut found = Vec::new();
        query.walk(&self.rows, |hit, captures| {
            let writable = self.rows[hit.start.row].kind != LineKind::Removed;
            if writable && find.scope.is_none_or(|(from, to)| hit.start >= from && hit.end <= to) {
                found.push((hit, find::expand(&template, captures, options)));
            }
        });
        found
    }

    fn replace_current(&mut self, cx: &mut Context<Self>) {
        if !self.is_editable() {
            return;
        }
        let Some(target) = self.find.as_ref().and_then(|find| find.hits.get(find.current)).copied() else { return };
        let Some((_, text)) = self.replacements(cx).into_iter().find(|(hit, _)| *hit == target) else {
            self.find_step(1, cx);
            return;
        };
        self.anchor = Some(target.start);
        self.cursor = target.end;
        self.snapshot(false);
        self.delete_selection();
        self.anchor = None;
        self.cursor = target.start;
        self.insert_at_cursor(&text);
        let after = self.cursor;
        if let Some((_, end)) = self.find.as_mut().and_then(|find| find.scope.as_mut()) {
            *end = shifted(*end, target.end, after);
        }
        self.after_edit(cx);
        if let Some(find) = self.find.as_mut() {
            find.current = find.hits.iter().position(|hit| hit.start >= after).unwrap_or(0);
        }
        self.reveal_found(cx);
    }

    fn replace_all(&mut self, cx: &mut Context<Self>) {
        if !self.is_editable() {
            return;
        }
        let found = self.replacements(cx);
        if found.is_empty() {
            return;
        }
        self.snapshot(false);
        self.tokens.resize(self.rows.len(), Vec::new());
        for (hit, text) in found.iter().rev() {
            self.anchor = Some(hit.start);
            self.cursor = hit.end;
            self.delete_selection();
            self.anchor = None;
            self.cursor = hit.start;
            self.insert_at_cursor(text);
        }
        if let Some(find) = self.find.as_mut() {
            find.scope = None;
        }
        self.after_edit(cx);
        if let Some(find) = self.find.as_mut() {
            find.notice = Some(format!("{} replaced", found.len()));
        }
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(items: &[(LineKind, &str)]) -> Vec<Row> {
        items.iter().map(|(kind, text)| Row { kind: *kind, text: text.to_string(), old: None, new: None }).collect()
    }

    #[test]
    fn hits_cover_each_row() {
        let rows = rows(&[(LineKind::Context, "abc"), (LineKind::Removed, "old"), (LineKind::Added, "xyz")]);
        let hit = Hit { start: Pos { row: 0, col: 1 }, end: Pos { row: 2, col: 2 } };
        let map = hits_by_row(&rows, &[hit]);
        assert_eq!(map.get(&0), Some(&vec![(0, 1..3)]));
        assert_eq!(map.get(&1), None);
        assert_eq!(map.get(&2), Some(&vec![(0, 0..2)]));
    }

    #[test]
    fn shifts_positions_after_an_edit() {
        let pos = |row, col| Pos { row, col };
        assert_eq!(shifted(pos(1, 9), pos(1, 5), pos(1, 2)), pos(1, 6));
        assert_eq!(shifted(pos(3, 4), pos(1, 5), pos(2, 0)), pos(4, 4));
        assert_eq!(shifted(pos(0, 4), pos(1, 5), pos(2, 0)), pos(0, 4));
    }
}
