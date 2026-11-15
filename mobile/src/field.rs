use std::ops::Range;

use gpui::prelude::FluentBuilder;
use gpui::{
    App, Autocapitalize, Bounds, Context, ElementInputHandler, EventEmitter, FocusHandle, InteractiveElement,
    IntoElement, KeyDownEvent, ParentElement, Pixels, Point, Render, StatefulInteractiveElement, Styled,
    TextInputAction, TextInputConfiguration, UTF16Selection, Window, canvas, div, px,
};

use crate::ui;

pub enum FieldEvent {
    Submit,
}

pub struct Field {
    pub text: String,
    placeholder: &'static str,
    capitalize: Autocapitalize,
    action: TextInputAction,
    focus: FocusHandle,
    marked: Option<String>,
}

impl EventEmitter<FieldEvent> for Field {}

impl Field {
    pub fn new(
        placeholder: &'static str,
        capitalize: Autocapitalize,
        action: TextInputAction,
        cx: &mut Context<Self>,
    ) -> Field {
        Field { text: String::new(), placeholder, capitalize, action, focus: cx.focus_handle(), marked: None }
    }

    pub fn set(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        self.text = text.into();
        cx.notify();
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.focus, cx);
    }
}

impl gpui::Focusable for Field {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Field {
    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "backspace" => {
                self.text.pop();
                cx.notify();
            }
            "enter" => {
                window.blur(cx);
                cx.emit(FieldEvent::Submit);
            }
            "escape" => window.blur(cx),
            _ => return,
        }
        cx.stop_propagation();
    }
}

impl Render for Field {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = ui::theme(window, cx);
        let focused = self.focus.is_focused(window);
        let (entity, focus) = (cx.entity(), self.focus.clone());
        let shown = format!("{}{}", self.text, self.marked.as_deref().unwrap_or(""));
        div()
            .id("field")
            .relative()
            .flex()
            .items_center()
            .h(px(ui::ROW))
            .px(px(12.0))
            .border_1()
            .border_color(if focused { theme.accent } else { theme.border })
            .bg(theme.surface)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .on_click(cx.listener(|field, _, window, cx| field.focus(window, cx)))
            .when(shown.is_empty() && !focused, |this| {
                this.child(div().text_color(theme.faint).child(self.placeholder))
            })
            .when(!shown.is_empty() || focused, |this| this.child(div().truncate().child(shown)))
            .when(focused, |this| this.child(div().w(px(8.0)).h(px(18.0)).bg(theme.accent)))
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, cx| {
                        window.handle_input(&focus, ElementInputHandler::new(bounds, entity), cx)
                    },
                )
                .absolute()
                .size_full(),
            )
    }
}

impl gpui::EntityInputHandler for Field {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let units: Vec<u16> = self.text.encode_utf16().collect();
        let range = range.start.min(units.len())..range.end.min(units.len());
        *adjusted = Some(range.clone());
        String::from_utf16(&units[range]).ok()
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let end = self.text.encode_utf16().count();
        Some(UTF16Selection { range: end..end, reversed: false })
    }

    fn marked_text_range(&self, _window: &mut Window, _cx: &mut Context<Self>) -> Option<Range<usize>> {
        let start = self.text.encode_utf16().count();
        self.marked.as_ref().map(|text| start..start + text.encode_utf16().count())
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.marked = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.marked = None;
        if text == "\n" {
            window.blur(cx);
            cx.emit(FieldEvent::Submit);
        } else {
            self.text.push_str(&text.replace('\n', " "));
        }
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        text: &str,
        _selected: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.marked = Some(text.to_string()).filter(|text| !text.is_empty());
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        _range: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        Some(element_bounds)
    }

    fn character_index_for_point(
        &mut self,
        _point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }

    fn text_input_configuration(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> TextInputConfiguration {
        TextInputConfiguration {
            autocorrect: false,
            autocapitalize: self.capitalize,
            suggestions: false,
            input_action: self.action,
        }
    }
}
