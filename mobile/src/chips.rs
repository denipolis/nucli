use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, Div, InteractiveElement, IntoElement, ParentElement, StatefulInteractiveElement, Styled, div, px,
};
use nucli_core::chips::{Entry, level, running, tokens};
use nucli_core::icons;
use nucli_core::theme::Theme;
use nucli_core::uplink::protocol::RemoteTab;

use crate::ui;

fn chip(glyph: &'static str, theme: &Theme) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(4.0))
        .px(px(4.0))
        .bg(theme.elevated)
        .border_1()
        .border_color(theme.border)
        .child(div().text_color(theme.muted).child(glyph))
}

fn count(glyph: &'static str, entries: &[Entry], label: &str, theme: &Theme) -> Div {
    let used = entries.iter().filter(|entry| entry.used).count();
    chip(glyph, theme)
        .child(format!("{} {label}", entries.len()))
        .when(used > 0, |this| this.child(div().text_color(theme.accent).child(format!("{used} used"))))
}

pub fn row(tab: &RemoteTab, time: u128, theme: &Theme) -> Option<AnyElement> {
    let mut chips = Vec::new();
    if let Some(cwd) = &tab.cwd {
        chips.push(chip(icons::FOLDER, theme).child(cwd.clone()));
    }
    if let Some(branch) = &tab.branch {
        chips.push(chip(icons::BRANCH, theme).child(branch.clone()));
    }
    if let Some(stat) = tab.changes {
        chips.push(
            chip("±", theme)
                .child(stat.files.to_string())
                .child(div().text_color(theme.green).child(format!("+{}", stat.added)))
                .child(div().text_color(theme.red).child(format!("-{}", stat.removed))),
        );
    }
    if let Some(program) = &tab.program {
        chips.push(chip(icons::PLAY, theme).child(program.clone()));
    }
    if let Some(info) = &tab.info {
        if !info.skills.is_empty() {
            chips.push(count(icons::SKILLS, &info.skills, "skills", theme));
        }
        if !info.memory.is_empty() {
            chips.push(chip(icons::INSTRUCTIONS, theme).child(format!("{} md", info.memory.len())));
        }
        if let Some(model) = &info.model {
            chips.push(chip(icons::MODEL, theme).child(model.clone()));
        }
        if info.tokens.is_some() || info.compacting {
            let mut element = chip(icons::GAUGE, theme);
            if info.compacting {
                element =
                    element.child(div().text_color(theme.accent).child(format!("{} compacting", ui::spinner(time))));
            }
            match (info.tokens, info.window.filter(|window| *window > 0)) {
                (Some(used), Some(window)) => {
                    let percent = used * 100 / window;
                    element = element
                        .child(div().text_color(level(percent, theme)).child(format!("{percent}%")))
                        .child(div().text_color(theme.muted).child(format!("{}/{}", tokens(used), tokens(window))));
                }
                (Some(used), None) => element = element.child(tokens(used)),
                (None, _) => {}
            }
            if info.compactions > 0 {
                element =
                    element.child(div().text_color(theme.muted).child(format!("compacted {}x", info.compactions)));
            }
            chips.push(element);
        }
        if !info.limits.is_empty() {
            let mut element = chip(icons::LIMITS, theme);
            for limit in &info.limits {
                let percent = limit.current();
                element = element
                    .child(div().text_color(theme.muted).child(limit.label()))
                    .child(div().text_color(level(percent, theme)).child(format!("{percent}%")));
            }
            chips.push(element);
        }
        if !info.mcp.is_empty() {
            chips.push(count(icons::MCP, &info.mcp, "mcp", theme));
        }
        if let Some(started) = info.started {
            chips.push(chip(icons::CLOCK, theme).child(running(started)));
        }
    }
    (!chips.is_empty()).then(|| {
        div()
            .id("chips")
            .flex()
            .flex_none()
            .gap(px(4.0))
            .px(px(8.0))
            .py(px(4.0))
            .overflow_x_scroll()
            .text_size(px(ui::SMALL))
            .text_color(theme.text)
            .bg(theme.background)
            .children(chips)
            .into_any_element()
    })
}
