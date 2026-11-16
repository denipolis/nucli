use std::sync::mpsc::Sender;
use std::time::Duration;

use gpui::prelude::FluentBuilder;
use gpui::{
    Animation, AnimationExt, AnyElement, App, AppContext, ClipboardItem, Context, Div, Entity, FocusHandle, Focusable,
    InteractiveElement, IntoElement, KeyDownEvent, MouseButton, ParentElement, SharedString, Stateful,
    StatefulInteractiveElement, Styled, Subscription, Window, div, px,
};

use super::devices::{Access, Device};
use super::protocol::Hello;
use super::wire::Key;
use super::{Uplink, invite, net};
use crate::icons;
use crate::settings;
use crate::settings_view::{button, group};
use crate::store;
use crate::text_input::{InputEvent, TextInput};
use crate::theme::Theme;
use crate::ui;
use crate::window::WorkspaceView;

pub struct Dialog {
    pub workspace: String,
    pub step: Step,
    pub error: Option<String>,
    opened: bool,
    input: Entity<TextInput>,
    focus: FocusHandle,
    _input: Subscription,
}

pub enum Step {
    Connect,
    Asked { key: Key, name: String, sas: String, answer: Sender<Option<Access>> },
    Waiting { key: Option<Key>, name: String, sas: Option<String> },
    Permissions { key: Key, name: String },
}

impl Step {
    fn number(&self) -> usize {
        match self {
            Step::Connect => 0,
            Step::Asked { .. } | Step::Waiting { .. } => 1,
            Step::Permissions { .. } => 2,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Step::Connect => "connect",
            Step::Asked { .. } => "asked",
            Step::Waiting { .. } => "waiting",
            Step::Permissions { .. } => "permissions",
        }
    }
}

const STEPS: [&str; 3] = ["connect", "confirm", "permissions"];

fn short(name: &str) -> String {
    name.chars().take(64).collect()
}

fn window_of(workspace: &str, cx: &App) -> Option<gpui::WindowHandle<WorkspaceView>> {
    store::store(cx).read(cx).windows.get(workspace).copied()
}

fn with_uplink(cx: &mut App, change: impl FnOnce(&mut Uplink, &mut Context<Uplink>)) {
    if let Some(uplink) = super::uplink(cx) {
        uplink.update(cx, change);
    }
}

pub fn show(view: &WorkspaceView, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let workspace = view.workspace.clone();
    with_uplink(cx, |uplink, cx| uplink.open_dialog(&workspace, true, cx));
    if let Some(focus) = super::uplink(cx).and_then(|uplink| uplink.read(cx).dialog_focus(cx)) {
        window.focus(&focus, cx);
    }
}

impl Uplink {
    fn dialog_focus(&self, cx: &App) -> Option<FocusHandle> {
        let dialog = self.dialog.as_ref()?;
        Some(match dialog.step {
            Step::Connect => dialog.input.focus_handle(cx),
            _ => dialog.focus.clone(),
        })
    }

    fn refocus(&self, cx: &mut Context<Self>) {
        let Some(dialog) = &self.dialog else { return };
        let Some(handle) = window_of(&dialog.workspace, cx) else { return };
        let Some(focus) = self.dialog_focus(cx) else { return };
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| window.focus(&focus, cx));
        });
    }

    pub fn open_dialog(&mut self, workspace: &str, opened: bool, cx: &mut Context<Self>) {
        match self.dialog.as_mut() {
            Some(dialog) => {
                dialog.workspace = workspace.to_string();
                dialog.opened |= opened;
                if matches!(dialog.step, Step::Permissions { .. }) {
                    dialog.step = Step::Connect;
                    dialog.error = None;
                }
            }
            None => {
                let input = cx.new(|cx| TextInput::new("name, address or pairing link", cx));
                let subscription = cx.subscribe(&input, |uplink, input, event, cx| match event {
                    InputEvent::Submit => {
                        let text = input.read(cx).text.clone();
                        uplink.connect_to(&text, cx);
                    }
                    InputEvent::Cancel => uplink.close_dialog(cx),
                    _ => {}
                });
                self.dialog = Some(Dialog {
                    workspace: workspace.to_string(),
                    step: Step::Connect,
                    error: None,
                    opened,
                    input,
                    focus: cx.focus_handle(),
                    _input: subscription,
                });
            }
        }
        if self.pairing().is_none() {
            self.start_pairing(cx);
        }
        cx.notify();
    }

    pub fn close_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.dialog.take() else { return };
        match dialog.step {
            Step::Asked { answer, .. } => {
                let _ = answer.send(None);
            }
            Step::Waiting { .. } => self.shared.cancel_join(),
            _ => {}
        }
        self.cancel_pairing(cx);
        if let Some(handle) = window_of(&dialog.workspace, cx) {
            cx.defer(move |cx| {
                let _ = handle.update(cx, |view, window, cx| view.focus_current(window, cx));
            });
        }
        cx.notify();
    }

    fn set_step(&mut self, step: Step, cx: &mut Context<Self>) {
        if let Some(dialog) = self.dialog.as_mut() {
            dialog.step = step;
            dialog.error = None;
        }
        self.refocus(cx);
        cx.notify();
    }

    fn fail(&mut self, reason: String, cx: &mut Context<Self>) {
        if let Some(dialog) = self.dialog.as_mut() {
            dialog.step = Step::Connect;
            dialog.error = Some(reason);
        }
        self.refocus(cx);
        cx.notify();
    }

    fn nearby_addresses(&self, key: &Key) -> Vec<String> {
        self.nearby
            .iter()
            .filter(|nearby| nearby.key == *key)
            .flat_map(|nearby| nearby.addresses.iter().map(|address| address.to_string()))
            .collect()
    }

    pub fn connect_nearby(&mut self, key: Key, cx: &mut Context<Self>) {
        let Some(name) = self.nearby.iter().find(|nearby| nearby.key == key).map(|nearby| nearby.name.clone()) else {
            return;
        };
        let addresses = self.nearby_addresses(&key);
        self.set_step(Step::Waiting { key: Some(key), name, sas: None }, cx);
        net::join(addresses, None, Some(key), Access::View, settings::get(cx).uplink.port, &self.shared);
    }

    pub fn connect_to(&mut self, text: &str, cx: &mut Context<Self>) {
        let text = text.trim();
        if let Some(invite) = invite::parse(text) {
            if invite.key.as_ref() == Some(self.identity()) {
                self.fail("That is the pairing link of this computer. Open it on the other device.".into(), cx);
                return;
            }
            let found = invite.key.and_then(|key| self.nearby.iter().find(|nearby| nearby.key == key));
            let name = found.map_or_else(|| "the other device".to_string(), |nearby| nearby.name.clone());
            let mut addresses = invite.addresses;
            if let Some(key) = invite.key {
                addresses.extend(self.nearby_addresses(&key));
            }
            self.set_step(Step::Waiting { key: invite.key, name, sas: None }, cx);
            net::join(
                addresses,
                Some(invite.code),
                invite.key,
                Access::View,
                settings::get(cx).uplink.port,
                &self.shared,
            );
            return;
        }
        if text.is_empty() {
            self.fail("Type a name, an address or a pairing link.".into(), cx);
            return;
        }
        let found = self
            .nearby
            .iter()
            .find(|nearby| nearby.name == text || nearby.addresses.iter().any(|address| address.to_string() == text));
        if let Some(key) = found.map(|nearby| nearby.key) {
            self.connect_nearby(key, cx);
            return;
        }
        self.set_step(Step::Waiting { key: None, name: text.to_string(), sas: None }, cx);
        net::join(vec![text.to_string()], None, None, Access::View, settings::get(cx).uplink.port, &self.shared);
    }

    pub fn allow(&mut self) {
        if let Some(Step::Asked { answer, .. }) = self.dialog.as_ref().map(|dialog| &dialog.step) {
            let _ = answer.send(Some(Access::View));
        }
    }

    pub fn deny(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.dialog.as_ref() else { return };
        if let Step::Asked { answer, .. } = &dialog.step {
            let _ = answer.send(None);
        }
        if dialog.opened {
            self.set_step(Step::Connect, cx);
        } else {
            self.close_dialog(cx);
        }
    }

    pub(super) fn asked(
        &mut self,
        key: Key,
        hello: Hello,
        sas: String,
        answer: Sender<Option<Access>>,
        cx: &mut Context<Self>,
    ) {
        if self.device(&key).is_some_and(|device| device.blocked) {
            let _ = answer.send(None);
            return;
        }
        if let Some(Step::Waiting { key: waiting, .. }) = self.dialog.as_ref().map(|dialog| &dialog.step) {
            if *waiting != Some(key) || *self.identity() > key {
                let _ = answer.send(None);
                return;
            }
            self.shared.cancel_join();
        }
        let store = store::store(cx).read(cx);
        let workspace = self
            .dialog
            .as_ref()
            .map(|dialog| dialog.workspace.clone())
            .or_else(|| store.last_workspace.clone())
            .filter(|workspace| store.windows.contains_key(workspace))
            .or_else(|| store.windows.keys().next().cloned());
        let Some(workspace) = workspace else {
            let _ = answer.send(None);
            return;
        };
        self.open_dialog(&workspace, false, cx);
        self.set_step(Step::Asked { key, name: short(&hello.name), sas, answer }, cx);
        if let Some(handle) = window_of(&workspace, cx) {
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, window, _| window.activate_window());
            });
        }
    }

    pub(super) fn confirmed(&mut self, name: String, shown: String, cx: &mut Context<Self>) {
        if let Some(Step::Waiting { name: waiting, sas, .. }) = self.dialog.as_mut().map(|dialog| &mut dialog.step) {
            *waiting = short(&name);
            *sas = Some(shown);
            cx.notify();
        }
    }

    pub(super) fn join_failed(&mut self, reason: String, cx: &mut Context<Self>) {
        if matches!(self.dialog.as_ref().map(|dialog| &dialog.step), Some(Step::Waiting { .. })) {
            self.fail(reason, cx);
        }
    }

    pub(super) fn declined(&mut self, key: Key, name: String, cx: &mut Context<Self>) {
        let Some(dialog) = self.dialog.as_ref() else { return };
        if !matches!(&dialog.step, Step::Asked { key: asked, .. } if *asked == key) {
            return;
        }
        if dialog.opened {
            self.fail(format!("{} stopped connecting.", short(&name)), cx);
        } else {
            self.close_dialog(cx);
        }
    }

    pub(super) fn pairing_failed(&mut self, attempts: u32, ended: bool, cx: &mut Context<Self>) {
        let Some(dialog) = self.dialog.as_mut() else { return };
        dialog.error = Some(if ended {
            format!("Someone tried {attempts} wrong codes. This code no longer works; a new one is shown.")
        } else {
            let left = net::PAIRING_ATTEMPTS - attempts;
            format!("Someone tried to pair with a wrong code. {left} tries left for this code.")
        });
        cx.notify();
    }

    pub(super) fn dialog_paired(&mut self, key: Key, name: String, cx: &mut Context<Self>) {
        if self.dialog.is_some() {
            self.set_step(Step::Permissions { key, name }, cx);
        }
    }

    pub(super) fn keep_code(&mut self, cx: &mut Context<Self>) {
        if matches!(self.dialog.as_ref().map(|dialog| &dialog.step), Some(Step::Connect)) && self.pairing().is_none() {
            self.start_pairing(cx);
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let Some(dialog) = &self.dialog else { return };
        match (event.keystroke.key.as_str(), &dialog.step) {
            ("escape", Step::Asked { .. }) => self.deny(cx),
            ("escape", _) | ("enter", Step::Permissions { .. }) => self.close_dialog(cx),
            _ => return,
        }
        cx.stop_propagation();
    }
}

fn primary(id: &'static str, label: &'static str, theme: &Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .px(px(12.0))
        .bg(theme.accent)
        .text_color(theme.on_accent)
        .border_1()
        .border_color(theme.accent)
        .hover(|style| style.bg(theme.text).border_color(theme.text).text_color(theme.background))
        .child(label)
}

fn spinner(id: &'static str, theme: &Theme) -> AnyElement {
    div()
        .text_color(theme.accent)
        .with_animation(id, Animation::new(Duration::from_millis(1100)).repeat(), |this, delta| {
            this.child(icons::SPINNER[(delta * icons::SPINNER.len() as f32) as usize % icons::SPINNER.len()])
        })
        .into_any_element()
}

fn heading(step: usize, title: String, description: String, theme: &Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .child(div().text_color(theme.faint).child(format!("{:02} / {:02}", step + 1, STEPS.len())))
        .child(div().text_size(px(22.0)).line_height(px(32.0)).child(title))
        .child(div().text_color(theme.muted).child(description))
}

fn section(title: &'static str, theme: &Theme) -> Div {
    div().pt(px(8.0)).text_color(theme.faint).child(title)
}

fn number(sas: Option<&str>, theme: &Theme) -> Div {
    let shown = match sas {
        Some(sas) => div().text_size(px(40.0)).line_height(px(52.0)).text_color(theme.accent).child(sas.to_string()),
        None => div().flex().gap(px(8.0)).child(spinner("pairing-connecting", theme)).child("connecting…"),
    };
    group(theme).px(px(16.0)).py(px(12.0)).child(div().text_color(theme.faint).child("both screens show")).child(shown)
}

fn connect(dialog: &Dialog, uplink: &Uplink, cx: &App, theme: &Theme) -> Vec<AnyElement> {
    let mut body = vec![
        heading(
            0,
            "Connect a device".into(),
            "Pair a phone or another computer with this one. Once paired they connect by themselves.".into(),
            theme,
        )
        .into_any_element(),
    ];
    if let Some((code, ..)) = uplink.pairing() {
        let excluded = &settings::get(cx).uplink.excluded_networks;
        let addresses = uplink.port().map(|port| invite::local_addresses(port, excluded)).unwrap_or_default();
        let link = invite::link(&code, uplink.identity(), &addresses);
        body.push(
            group(theme)
                .flex_row()
                .gap(px(16.0))
                .p(px(12.0))
                .children(super::page::qr(&link, 4.0, theme))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .min_w_0()
                        .child("A phone")
                        .child(
                            div()
                                .text_color(theme.muted)
                                .child("Open Nucli Uplink, tap add computer and scan this code."),
                        )
                        .child(div().flex().child(button("pairing-copy", "copy pairing link", theme).on_click(
                            move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(link.clone())),
                        )))
                        .child(div().text_color(theme.faint).child(
                            "The link pairs from anywhere this computer can be reached. Code and link change every 5 minutes.",
                        )),
                )
                .into_any_element(),
        );
    }
    body.push(section("Computers nearby", theme).into_any_element());
    let mut nearby = group(theme);
    let found: Vec<_> = uplink.nearby.iter().filter(|nearby| uplink.device(&nearby.key).is_none()).collect();
    if found.is_empty() {
        nearby = nearby.child(
            div()
                .flex()
                .gap(px(8.0))
                .px(px(12.0))
                .py(px(6.0))
                .text_color(theme.muted)
                .child(spinner("pairing-looking", theme))
                .child("Looking for computers on this network with Uplink on…"),
        );
    }
    for (index, device) in found.into_iter().enumerate() {
        let key = device.key;
        nearby = nearby.child(
            div()
                .id(("pairing-nearby", index))
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(12.0))
                .py(px(6.0))
                .border_b_1()
                .border_color(theme.border)
                .hover(|style| style.bg(theme.hover))
                .child(div().text_color(theme.accent).child(icons::DEVICE))
                .child(div().flex_none().child(if device.name.is_empty() {
                    "computer".to_string()
                } else {
                    device.name.clone()
                }))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(theme.faint)
                        .children(device.addresses.first().map(|address| address.to_string())),
                )
                .child(button(SharedString::from(format!("pairing-connect-{index}")), "connect", theme))
                .on_click(move |_, _, cx| with_uplink(cx, |uplink, cx| uplink.connect_nearby(key, cx))),
        );
    }
    body.push(nearby.into_any_element());
    body.push(section("Not listed", theme).into_any_element());
    let input = dialog.input.clone();
    body.push(
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .px(px(4.0))
                    .bg(theme.surface)
                    .border_1()
                    .border_color(theme.border)
                    .child(input.clone()),
            )
            .child(button("pairing-connect-typed", "connect", theme).on_click(move |_, _, cx| {
                let text = input.read(cx).text.clone();
                with_uplink(cx, |uplink, cx| uplink.connect_to(&text, cx));
            }))
            .into_any_element(),
    );
    body
}

fn asked(name: &str, sas: &str, theme: &Theme) -> Vec<AnyElement> {
    vec![
        heading(
            1,
            format!("{name} wants to connect"),
            format!("Allow it only if you are connecting {name} right now and it shows this number."),
            theme,
        )
        .into_any_element(),
        number(Some(sas), theme).into_any_element(),
        div().text_color(theme.faint).child("If the numbers are not the same, deny.").into_any_element(),
    ]
}

fn waiting(name: &str, sas: Option<&str>, theme: &Theme) -> Vec<AnyElement> {
    vec![
        heading(
            1,
            format!("Waiting for {name}"),
            format!("{name} asks whether to allow this computer. Check that it shows the same number."),
            theme,
        )
        .into_any_element(),
        number(sas, theme).into_any_element(),
        div()
            .flex()
            .gap(px(8.0))
            .text_color(theme.muted)
            .child(spinner("pairing-waiting", theme))
            .child("waiting for an answer")
            .into_any_element(),
    ]
}

fn choice(id: &'static str, label: &'static str, hint: &'static str, on: bool, theme: &Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(10.0))
        .px(px(12.0))
        .py(px(6.0))
        .border_b_1()
        .border_color(theme.border)
        .hover(|style| style.bg(theme.hover))
        .child(div().flex_none().text_color(if on { theme.accent } else { theme.muted }).child(if on {
            "[x]"
        } else {
            "[ ]"
        }))
        .child(div().flex_none().w(px(72.0)).when(!on, |this| this.text_color(theme.muted)).child(label))
        .child(div().flex_1().min_w_0().text_color(theme.faint).child(hint))
}

fn permissions(key: Key, name: &str, device: Option<&Device>, theme: &Theme) -> Vec<AnyElement> {
    let access = device.map_or(Access::View, |device| device.access);
    vec![
        heading(
            2,
            format!("Paired with {name}"),
            format!("Choose what {name} can do with the tabs and projects this computer shares."),
            theme,
        )
        .into_any_element(),
        group(theme)
            .child(
                choice(
                    "pairing-view",
                    "view",
                    "watch tabs, read files, changes and history",
                    access == Access::View,
                    theme,
                )
                .on_click(move |_, _, cx| with_uplink(cx, |uplink, cx| uplink.set_access(&key, Access::View, cx))),
            )
            .child(
                choice(
                    "pairing-control",
                    "control",
                    "also type into tabs, start agents, change files and git",
                    access == Access::Control,
                    theme,
                )
                .on_click(move |_, _, cx| with_uplink(cx, |uplink, cx| uplink.set_access(&key, Access::Control, cx))),
            )
            .into_any_element(),
        div()
            .text_color(theme.faint)
            .child("Change it any time in Settings › Uplink. Which tabs are shared is set per project and tab.")
            .into_any_element(),
    ]
}

fn chips(current: usize, theme: &Theme) -> Div {
    div().flex().gap(px(4.0)).children(STEPS.iter().enumerate().map(|(position, title)| {
        let active = position == current;
        div()
            .flex()
            .gap(px(6.0))
            .px(px(6.0))
            .when(active, |this| this.bg(theme.accent).text_color(theme.on_accent))
            .when(position < current, |this| this.text_color(theme.muted))
            .when(position > current, |this| this.text_color(theme.faint))
            .child(if position < current { icons::CHECK.to_string() } else { format!("{}", position + 1) })
            .child(*title)
    }))
}

fn actions(step: &Step, theme: &Theme) -> Div {
    let close = |label: &'static str| {
        button("pairing-close", label, theme).on_click(|_, _, cx| with_uplink(cx, |uplink, cx| uplink.close_dialog(cx)))
    };
    let row = div().flex().flex_none().items_center().gap(px(8.0));
    match step {
        Step::Connect => row.child(close("[esc] close")),
        Step::Waiting { .. } => row.child(close("[esc] cancel")),
        Step::Asked { .. } => row
            .child(
                button("pairing-deny", "[esc] deny", theme)
                    .on_click(|_, _, cx| with_uplink(cx, |uplink, cx| uplink.deny(cx))),
            )
            .child(
                primary("pairing-allow", "allow", theme)
                    .on_click(|_, _, cx| with_uplink(cx, |uplink, _| uplink.allow())),
            ),
        Step::Permissions { .. } => row.child(
            primary("pairing-done", "[enter] done", theme)
                .on_click(|_, _, cx| with_uplink(cx, |uplink, cx| uplink.close_dialog(cx))),
        ),
    }
}

pub fn render(view: &WorkspaceView, cx: &App) -> Option<AnyElement> {
    let entity = super::uplink(cx)?;
    let uplink = entity.read(cx);
    let dialog = uplink.dialog.as_ref().filter(|dialog| dialog.workspace == view.workspace)?;
    let theme = ui::theme(cx);
    let mut body = match &dialog.step {
        Step::Connect => connect(dialog, uplink, cx, &theme),
        Step::Asked { name, sas, .. } => asked(name, sas, &theme),
        Step::Waiting { name, sas, .. } => waiting(name, sas.as_deref(), &theme),
        Step::Permissions { key, name } => permissions(*key, name, uplink.device(key), &theme),
    };
    if let Some(error) = &dialog.error {
        body.push(div().text_color(theme.red).child(error.clone()).into_any_element());
    }
    let step = dialog.step.number();
    let keys = entity.clone();
    Some(
        div()
            .id("pairing")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .occlude()
            .flex()
            .flex_col()
            .bg(theme.background)
            .track_focus(&dialog.focus)
            .on_key_down(move |event, _, cx| keys.update(cx, |uplink, cx| uplink.key_down(event, cx)))
            .when(cfg!(target_os = "macos"), |this| {
                this.child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .w_full()
                        .h(px(38.0))
                        .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move()),
                )
            })
            .child(
                div().id("pairing-content").flex_1().min_h_0().overflow_y_scroll().flex().justify_center().child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(12.0))
                        .w_full()
                        .max_w(px(760.0))
                        .px(px(40.0))
                        .pt(px(if cfg!(target_os = "macos") { 56.0 } else { 32.0 }))
                        .pb(px(32.0))
                        .children(body),
                ),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_between()
                    .gap(px(16.0))
                    .px(px(16.0))
                    .py(px(10.0))
                    .border_t_1()
                    .border_color(theme.border)
                    .bg(theme.surface)
                    .child(chips(step, &theme))
                    .child(actions(&dialog.step, &theme)),
            )
            .into_any_element(),
    )
}
