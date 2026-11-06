use gpui::prelude::FluentBuilder;
use gpui::{
    App, AppContext, Context, EventEmitter, FocusHandle, Focusable, InteractiveElement, IntoElement, KeyDownEvent,
    ParentElement, PromptButton, PromptHandle, PromptLevel, PromptResponse, Render, RenderablePromptHandle,
    SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::settings;
use crate::ui;

struct Prompt {
    level: PromptLevel,
    message: SharedString,
    detail: Option<SharedString>,
    actions: Vec<SharedString>,
    selected: usize,
    cancel: usize,
    focus: FocusHandle,
}

impl EventEmitter<PromptResponse> for Prompt {}

impl Focusable for Prompt {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Prompt {
    fn key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let count = self.actions.len().max(1);
        match event.keystroke.key.as_str() {
            "enter" | "space" => cx.emit(PromptResponse(self.selected)),
            "escape" => cx.emit(PromptResponse(self.cancel)),
            "right" | "down" => self.selected = (self.selected + 1) % count,
            "left" | "up" => self.selected = (self.selected + count - 1) % count,
            "tab" if event.keystroke.modifiers.shift => self.selected = (self.selected + count - 1) % count,
            "tab" => self.selected = (self.selected + 1) % count,
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }
}

impl Render for Prompt {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = ui::theme(cx);
        let size = settings::get(cx).ui_font_size();
        let (label, color) = match self.level {
            PromptLevel::Info => ("Confirm", theme.accent),
            PromptLevel::Warning => ("Warning", theme.orange),
            PromptLevel::Critical => ("Danger", theme.red),
        };
        let mut buttons = div().flex().flex_col().gap(px(4.0)).mt(px(12.0));
        for (index, action) in self.actions.iter().enumerate() {
            let active = index == self.selected;
            buttons = buttons.child(
                div()
                    .id(("prompt-action", index))
                    .px(px(8.0))
                    .py(px(2.0))
                    .border_1()
                    .border_color(if active { color } else { theme.border })
                    .when(active, |this| this.bg(color).text_color(theme.on_accent))
                    .when(!active, |this| this.hover(|style| style.bg(theme.hover)))
                    .child(action.clone())
                    .on_click(cx.listener(move |_, _, _, cx| cx.emit(PromptResponse(index)))),
            );
        }
        div()
            .id("prompt")
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .occlude()
            .bg(gpui::black().opacity(0.45))
            .font_family(settings::FONT)
            .text_size(px(size))
            .line_height(px((size * 1.5).round()))
            .text_color(theme.text)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .child(
                div()
                    .w(px(440.0))
                    .bg(theme.elevated)
                    .border_1()
                    .border_color(color)
                    .child(div().px(px(10.0)).bg(color).text_color(theme.on_accent).child(label))
                    .child(
                        div()
                            .p(px(10.0))
                            .child(self.message.clone())
                            .children(
                                self.detail
                                    .clone()
                                    .map(|detail| div().mt(px(6.0)).text_color(theme.muted).child(detail)),
                            )
                            .child(buttons)
                            .child(
                                div()
                                    .mt(px(8.0))
                                    .text_color(theme.faint)
                                    .child("[↑↓] move  [enter] choose  [esc] cancel"),
                            ),
                    ),
            )
    }
}

pub fn build(
    level: PromptLevel,
    message: &str,
    detail: Option<&str>,
    actions: &[PromptButton],
    handle: PromptHandle,
    window: &mut Window,
    cx: &mut App,
) -> RenderablePromptHandle {
    let labels: Vec<SharedString> = actions.iter().map(|action| action.label().clone()).collect();
    let cancel = labels.iter().position(|label| label == "Cancel").unwrap_or(labels.len().saturating_sub(1));
    let prompt = cx.new(|cx| Prompt {
        level,
        message: message.to_string().into(),
        detail: detail.map(|detail| detail.to_string().into()),
        actions: labels,
        selected: 0,
        cancel,
        focus: cx.focus_handle(),
    });
    handle.with_view(prompt, window, cx)
}
