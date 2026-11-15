use std::sync::Arc;

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Context, Div, Hsla, InteractiveElement, IntoElement, ListHorizontalSizingBehavior, ParentElement,
    SharedString, StatefulInteractiveElement, Styled, div, px, uniform_list,
};
use nucli_core::theme::Theme;
use nucli_core::uplink::protocol::{LineKind, Response};
use nucli_core::uplink::wire::Key;

use crate::home::Home;
use crate::ui;

const CODE: f32 = 13.0;
const LINE: f32 = 20.0;

struct Line {
    number: SharedString,
    sign: &'static str,
    text: SharedString,
    color: Hsla,
    background: Option<Hsla>,
}

fn name_of(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

fn lines_view(id: &'static str, lines: Vec<Line>, gutter: usize, theme: &Theme) -> AnyElement {
    let widest = lines.iter().enumerate().max_by_key(|(_, line)| line.text.len()).map(|(index, _)| index);
    let lines = Arc::new(lines);
    let (faint, count) = (theme.faint, lines.len());
    uniform_list(id, count, move |range, _, _| {
        range
            .map(|index| {
                let line = &lines[index];
                div()
                    .flex()
                    .h(px(LINE))
                    .min_w_full()
                    .items_center()
                    .whitespace_nowrap()
                    .when_some(line.background, |this, background| this.bg(background))
                    .child(
                        div()
                            .flex_none()
                            .w(px(gutter as f32 * 8.0 + 16.0))
                            .pr(px(8.0))
                            .flex()
                            .justify_end()
                            .text_color(faint)
                            .child(line.number.clone()),
                    )
                    .child(div().flex_none().w(px(12.0)).text_color(line.color).child(line.sign))
                    .child(div().pr(px(16.0)).text_color(line.color).child(line.text.clone()))
            })
            .collect()
    })
    .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
    .with_width_from_item(widest)
    .flex_1()
    .min_h_0()
    .text_size(px(CODE))
    .into_any_element()
}

fn notice(text: impl Into<SharedString>, theme: &Theme) -> Div {
    div().child(ui::hint(text, theme).py(px(24.0)))
}

impl Home {
    fn viewer(&self, title: String, body: AnyElement, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        div().flex_1().min_h_0().flex().flex_col().child(self.back_bar(title, theme, cx)).child(body).into_any_element()
    }

    pub fn file(&mut self, path: &str, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let body = match &self.view {
            None => self.loading(theme).into_any_element(),
            Some(Response::Failed { reason }) => notice(reason.clone(), theme).into_any_element(),
            Some(Response::File { binary: true, .. }) => notice("This is a binary file.", theme).into_any_element(),
            Some(Response::File { text, truncated, .. }) => {
                let lines: Vec<Line> = text
                    .lines()
                    .enumerate()
                    .map(|(index, line)| Line {
                        number: (index + 1).to_string().into(),
                        sign: "",
                        text: line.replace('\t', "    ").into(),
                        color: theme.text,
                        background: None,
                    })
                    .collect();
                let gutter = lines.len().to_string().len();
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(
                        ui::hint(path.to_string(), theme)
                            .text_color(theme.faint)
                            .border_b_1()
                            .border_color(theme.border),
                    )
                    .child(lines_view("file", lines, gutter, theme))
                    .when(*truncated, |this| {
                        this.child(
                            ui::hint("Only the first megabyte is shown.", theme)
                                .border_t_1()
                                .border_color(theme.border),
                        )
                    })
                    .into_any_element()
            }
            Some(_) => div().into_any_element(),
        };
        self.viewer(name_of(path), body, theme, cx)
    }

    pub fn diff(&mut self, path: &str, commit: Option<String>, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let body = match &self.view {
            None => self.loading(theme).into_any_element(),
            Some(Response::Failed { reason }) => notice(reason.clone(), theme).into_any_element(),
            Some(Response::Diff { lines }) => {
                let rows: Vec<Line> = lines
                    .iter()
                    .map(|line| {
                        let (sign, color, background) = match line.kind {
                            LineKind::Added => ("+", theme.green, Some(theme.green.opacity(0.12))),
                            LineKind::Removed => ("-", theme.red, Some(theme.red.opacity(0.12))),
                            LineKind::Context => ("", theme.text, None),
                        };
                        Line {
                            number: line.new.or(line.old).map(|number| number.to_string()).unwrap_or_default().into(),
                            sign,
                            text: line.text.replace('\t', "    ").into(),
                            color,
                            background,
                        }
                    })
                    .collect();
                let gutter = rows.iter().map(|line| line.number.len()).max().unwrap_or(1);
                let subtitle = match &commit {
                    Some(hash) => format!("{path} · {}", hash.chars().take(7).collect::<String>()),
                    None => path.to_string(),
                };
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(ui::hint(subtitle, theme).text_color(theme.faint).border_b_1().border_color(theme.border))
                    .child(lines_view("diff", rows, gutter, theme))
                    .into_any_element()
            }
            Some(_) => div().into_any_element(),
        };
        self.viewer(name_of(path), body, theme, cx)
    }

    pub fn commit(&mut self, key: Key, project: &str, hash: &str, theme: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let short: String = hash.chars().take(7).collect();
        let body = match self.view.clone() {
            None => self.loading(theme).into_any_element(),
            Some(Response::Failed { reason }) => notice(reason, theme).into_any_element(),
            Some(Response::Commit { date, message, stats, changes }) => {
                let mut list = div()
                    .flex()
                    .flex_col()
                    .child(div().px(px(16.0)).pt(px(12.0)).pb(px(8.0)).child(message.trim().to_string()))
                    .child(ui::label_value("commit", div().truncate().child(hash.to_string()), theme))
                    .child(ui::label_value("date", div().truncate().child(date), theme))
                    .when(!stats.is_empty(), |this| this.child(ui::hint(stats, theme)))
                    .child(ui::section(format!("files {}", changes.len()), theme));
                for change in &changes {
                    list = list.child(self.change_row(key, project, change, Some(hash.to_string()), theme, cx));
                }
                div().id("commit").flex_1().min_h_0().overflow_y_scroll().child(list).into_any_element()
            }
            Some(_) => div().into_any_element(),
        };
        self.viewer(format!("commit {short}"), body, theme, cx)
    }
}
