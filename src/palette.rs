use std::rc::Rc;

use gpui::{
    AnyElement, App, AppContext, Context, Entity, EventEmitter, FocusHandle, Focusable, Hsla, InteractiveElement,
    IntoElement, MouseButton, ParentElement, Render, SharedString, StatefulInteractiveElement, Styled, Subscription,
    UniformListScrollHandle, Window, div, px, uniform_list,
};

use crate::fuzzy;
use crate::text_input::{InputEvent, TextInput};
use crate::ui;
use crate::window::WorkspaceView;

pub type Run = Rc<dyn Fn(&mut WorkspaceView, &mut Window, &mut Context<WorkspaceView>)>;

#[derive(Clone)]
pub struct Item {
    pub title: SharedString,
    pub subtitle: SharedString,
    pub glyph: &'static str,
    pub color: Option<Hsla>,
    pub detail: SharedString,
    pub section: SharedString,
    pub rank: i32,
    pub path: bool,
    pub digit: Option<usize>,
    pub hidden_when_empty: bool,
    pub run: Run,
    pub alternate: Option<Run>,
    pub preview: Option<Run>,
}

impl Item {
    pub fn new(
        title: impl Into<SharedString>,
        run: impl Fn(&mut WorkspaceView, &mut Window, &mut Context<WorkspaceView>) + 'static,
    ) -> Item {
        Item {
            title: title.into(),
            subtitle: SharedString::default(),
            glyph: "",
            color: None,
            detail: SharedString::default(),
            section: SharedString::default(),
            rank: 0,
            path: false,
            digit: None,
            hidden_when_empty: false,
            run: Rc::new(run),
            alternate: None,
            preview: None,
        }
    }

    pub fn subtitle(mut self, text: impl Into<SharedString>) -> Item {
        self.subtitle = text.into();
        self
    }

    pub fn glyph(mut self, glyph: &'static str, color: Option<Hsla>) -> Item {
        self.glyph = glyph;
        self.color = color;
        self
    }

    pub fn detail(mut self, text: impl Into<SharedString>) -> Item {
        self.detail = text.into();
        self
    }

    pub fn section(mut self, text: impl Into<SharedString>) -> Item {
        self.section = text.into();
        self
    }

    pub fn rank(mut self, rank: i32) -> Item {
        self.rank = rank;
        self
    }
}

pub enum PaletteEvent {
    Confirm(Item, bool),
    Preview(Item),
    Dismiss,
    Text(String),
}

enum Row {
    Header(SharedString),
    Item { index: usize, positions: Vec<usize> },
}

pub struct Palette {
    pub input: Entity<TextInput>,
    pub title: SharedString,
    pub items: Vec<Item>,
    rows: Vec<Row>,
    selected: usize,
    pub prompt: bool,
    pub digits: bool,
    scroll: UniformListScrollHandle,
    _subscription: Subscription,
}

impl EventEmitter<PaletteEvent> for Palette {}

impl Focusable for Palette {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.read(cx).focus_handle(cx)
    }
}

impl Palette {
    pub fn new(title: &str, placeholder: &str, items: Vec<Item>, initial: usize, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| TextInput::new(placeholder, cx));
        let subscription = cx.subscribe(&input, |palette: &mut Palette, _, event, cx| match event {
            InputEvent::Changed => {
                let text = palette.input.read(cx).text.clone();
                if palette.digits
                    && let Ok(digit) = text.parse::<usize>()
                    && let Some(item) = palette.items.iter().find(|item| item.digit == Some(digit))
                {
                    cx.emit(PaletteEvent::Confirm(item.clone(), false));
                    return;
                }
                if palette.prompt {
                    cx.emit(PaletteEvent::Text(text));
                }
                palette.filter(cx);
            }
            InputEvent::Submit => palette.confirm(false, cx),
            InputEvent::Cancel => cx.emit(PaletteEvent::Dismiss),
            InputEvent::Up => palette.move_by(-1, cx),
            InputEvent::Down => palette.move_by(1, cx),
            InputEvent::Blur => {}
        });
        let mut palette = Palette {
            input,
            title: title.to_string().into(),
            items,
            rows: Vec::new(),
            selected: 0,
            prompt: false,
            digits: false,
            scroll: UniformListScrollHandle::new(),
            _subscription: subscription,
        };
        palette.filter(cx);
        if let Some(row) =
            palette.rows.iter().position(|row| matches!(row, Row::Item { index, .. } if *index == initial))
        {
            palette.selected = row;
        }
        palette
    }

    pub fn set_items(&mut self, items: Vec<Item>, cx: &mut Context<Self>) {
        let current = self.current().map(|item| item.title.clone());
        self.items = items;
        self.filter(cx);
        if let Some(title) = current
            && let Some(row) = self
                .rows
                .iter()
                .position(|row| matches!(row, Row::Item { index, .. } if self.items[*index].title == title))
        {
            self.selected = row;
        }
    }

    fn current(&self) -> Option<&Item> {
        match self.rows.get(self.selected)? {
            Row::Item { index, .. } => self.items.get(*index),
            Row::Header(_) => None,
        }
    }

    fn filter(&mut self, cx: &mut Context<Self>) {
        let query = self.input.read(cx).text.trim().to_string();
        self.rows.clear();
        if query.is_empty() {
            let mut section = SharedString::default();
            for (index, item) in self.items.iter().enumerate() {
                if item.hidden_when_empty {
                    continue;
                }
                if item.section != section && !item.section.is_empty() {
                    self.rows.push(Row::Header(item.section.clone()));
                }
                section = item.section.clone();
                self.rows.push(Row::Item { index, positions: Vec::new() });
            }
        } else if !self.prompt {
            let mut ranked: Vec<(i32, usize, Vec<usize>)> = self
                .items
                .iter()
                .enumerate()
                .filter_map(|(index, item)| {
                    let found = if item.path {
                        fuzzy::score_path(&query, &item.title)
                    } else {
                        fuzzy::score(&query, &item.title)
                    };
                    found.map(|(score, positions)| (score + item.rank, index, positions))
                })
                .collect();
            ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
            self.rows =
                ranked.into_iter().take(300).map(|(_, index, positions)| Row::Item { index, positions }).collect();
        }
        self.selected = self.rows.iter().position(|row| matches!(row, Row::Item { .. })).unwrap_or(0);
        self.scroll.scroll_to_item(self.selected, gpui::ScrollStrategy::Top);
        self.preview(cx);
        cx.notify();
    }

    fn preview(&mut self, cx: &mut Context<Self>) {
        if let Some(item) = self.current().filter(|item| item.preview.is_some()).cloned() {
            cx.emit(PaletteEvent::Preview(item));
        }
    }

    fn move_by(&mut self, delta: i32, cx: &mut Context<Self>) {
        if self.rows.is_empty() {
            return;
        }
        let mut next = self.selected as i32;
        loop {
            next += delta;
            if next < 0 || next >= self.rows.len() as i32 {
                return;
            }
            if matches!(self.rows[next as usize], Row::Item { .. }) {
                break;
            }
        }
        self.selected = next as usize;
        self.scroll.scroll_to_item(self.selected, gpui::ScrollStrategy::Center);
        self.preview(cx);
        cx.notify();
    }

    pub fn confirm(&mut self, alternate: bool, cx: &mut Context<Self>) {
        if self.prompt {
            let text = self.input.read(cx).text.clone();
            cx.emit(PaletteEvent::Text(text));
            if let Some(item) = self.items.first().cloned() {
                cx.emit(PaletteEvent::Confirm(item, alternate));
            }
            return;
        }
        if let Some(item) = self.current().cloned() {
            cx.emit(PaletteEvent::Confirm(item, alternate));
        }
    }
}

fn highlighted(text: &SharedString, positions: &[usize], accent: Hsla) -> AnyElement {
    if positions.is_empty() {
        return div().child(text.clone()).into_any_element();
    }
    let mut row = div().flex();
    let mut current = String::new();
    let mut current_marked = false;
    let flush = |row: gpui::Div, text: &mut String, marked: bool| -> gpui::Div {
        if text.is_empty() {
            return row;
        }
        let part = std::mem::take(text);
        if marked { row.child(div().text_color(accent).child(part)) } else { row.child(part) }
    };
    for (index, character) in text.chars().enumerate() {
        let marked = positions.contains(&index);
        if marked != current_marked {
            row = flush(row, &mut current, current_marked);
            current_marked = marked;
        }
        current.push(character);
    }
    flush(row, &mut current, current_marked).into_any_element()
}

impl Render for Palette {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = ui::theme(cx);
        let line = px((crate::settings::get(cx).ui_font_size() * 1.9).round());
        let count = self.rows.len();
        let selected = self.selected;
        let list = uniform_list(
            "palette-list",
            count,
            cx.processor(move |palette: &mut Palette, range: std::ops::Range<usize>, _, cx| {
                let theme = ui::theme(cx);
                range
                    .map(|row_index| match &palette.rows[row_index] {
                        Row::Header(title) => div()
                            .id(("palette-row", row_index))
                            .h(line)
                            .px(px(10.0))
                            .flex()
                            .items_end()
                            .text_color(theme.faint)
                            .child(ui::caps(title))
                            .into_any_element(),
                        Row::Item { index, positions } => {
                            let item = &palette.items[*index];
                            let active = row_index == selected;
                            let (text, faint) =
                                if active { (theme.on_accent, theme.on_accent) } else { (theme.text, theme.muted) };
                            div()
                                .id(("palette-row", row_index))
                                .h(line)
                                .px(px(10.0))
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .when(active, |row| row.bg(theme.accent))
                                .text_color(text)
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_click(cx.listener(move |palette, _, _, cx| {
                                    palette.selected = row_index;
                                    palette.confirm(false, cx);
                                }))
                                .child(
                                    div()
                                        .w(px(16.0))
                                        .flex_none()
                                        .text_color(if active {
                                            theme.on_accent
                                        } else {
                                            item.color.unwrap_or(theme.muted)
                                        })
                                        .child(item.glyph),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_1()
                                        .min_w_0()
                                        .gap(px(10.0))
                                        .overflow_hidden()
                                        .child(div().flex_none().child(highlighted(
                                            &item.title,
                                            positions,
                                            if active { theme.on_accent } else { theme.accent },
                                        )))
                                        .child(div().text_color(faint).truncate().child(item.subtitle.clone())),
                                )
                                .child(div().flex_none().text_color(faint).child(item.detail.clone()))
                                .into_any_element()
                        }
                    })
                    .collect()
            }),
        )
        .track_scroll(self.scroll.clone())
        .h(line * (count.clamp(1, 14) as f32));
        let mut body =
            div().w(px(640.0)).bg(theme.elevated).border_1().border_color(theme.accent).flex().flex_col().child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(10.0))
                    .py(px(6.0))
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .flex_none()
                            .px(px(4.0))
                            .bg(theme.accent)
                            .text_color(theme.on_accent)
                            .child(ui::caps(&self.title)),
                    )
                    .child(self.input.clone()),
            );
        if count > 0 {
            body = body.child(list);
        }
        let hints = if self.prompt { "[enter] ok  [esc] cancel" } else { "[↑↓] move  [enter] open  [esc] close" };
        body.child(
            div().px(px(10.0)).py(px(3.0)).border_t_1().border_color(theme.border).text_color(theme.faint).child(hints),
        )
    }
}

use gpui::prelude::FluentBuilder;
