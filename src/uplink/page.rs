use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, AppContext, Bounds, ClipboardItem, Context, Entity, InteractiveElement, IntoElement,
    ParentElement, PromptLevel, SharedString, StatefulInteractiveElement, Styled, Subscription, Window, canvas, div,
    fill, point, px, size,
};

use super::devices::{Access, Device};
use super::wire::{self, Key};
use super::{Join, Uplink, invite};
use crate::icons;
use crate::settings::{self, UPLINK_PORT};
use crate::settings_view::{button, checkbox, group, row};
use crate::text_input::{InputEvent, TextInput};
use crate::theme::Theme;
use crate::ui;
use crate::window::WorkspaceView;

pub struct Inputs {
    name: Entity<TextInput>,
    port: Entity<TextInput>,
    address: Entity<TextInput>,
    code: Entity<TextInput>,
    manual: Vec<(String, Entity<TextInput>)>,
    open: Option<String>,
    subscriptions: Vec<Subscription>,
}

fn input(placeholder: &str, text: &str, cx: &mut Context<WorkspaceView>) -> Entity<TextInput> {
    cx.new(|cx| {
        let mut input = TextInput::new(placeholder, cx);
        input.set_text(text, cx);
        input
    })
}

fn with_uplink(cx: &mut App, change: impl FnOnce(&mut Uplink, &mut Context<Uplink>)) {
    if let Some(uplink) = super::uplink(cx) {
        uplink.update(cx, change);
    }
}

fn page_inputs(view: &WorkspaceView) -> Option<&Inputs> {
    view.settings.as_ref().and_then(|page| page.uplink.as_ref())
}

fn join(view: &mut WorkspaceView, cx: &mut Context<WorkspaceView>) {
    let Some(inputs) = page_inputs(view) else { return };
    let (address, code) = (inputs.address.read(cx).text.clone(), inputs.code.read(cx).text.clone());
    with_uplink(cx, |uplink, cx| uplink.start_join(&address, &code, Access::Control, cx));
}

pub fn ensure(view: &mut WorkspaceView, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let joined = super::uplink(cx).filter(|uplink| matches!(uplink.read(cx).join, Join::Done(_)));
    if let Some(uplink) = joined {
        uplink.update(cx, |uplink, _| uplink.finish_join());
        if let Some(inputs) = page_inputs(view) {
            let (address, code) = (inputs.address.clone(), inputs.code.clone());
            address.update(cx, |input, cx| input.clear(cx));
            code.update(cx, |input, cx| input.clear(cx));
        }
    }
    let Some(page) = view.settings.as_mut() else { return };
    if page.uplink.is_none() {
        let config = settings::get(cx).uplink.clone();
        let name = input(&config.device_name(), &config.name, cx);
        let port = input(&UPLINK_PORT.to_string(), &config.port.to_string(), cx);
        let address = input("its name or address", "", cx);
        let code = input("code or pairing link", "", cx);
        let subscriptions = vec![
            cx.subscribe_in(&name, window, |view, input, event, window, cx| match event {
                InputEvent::Changed => {
                    let text = input.read(cx).text.clone();
                    settings::update(cx, |settings| settings.uplink.name = text);
                }
                InputEvent::Submit | InputEvent::Cancel => view.focus_current(window, cx),
                _ => {}
            }),
            cx.subscribe_in(&port, window, |view, input, event, window, cx| match event {
                InputEvent::Submit => {
                    let text = input.read(cx).text.trim().to_string();
                    match text.parse::<u16>() {
                        Ok(port) if port >= 1024 => settings::update(cx, |settings| settings.uplink.port = port),
                        _ => {
                            let port = settings::get(cx).uplink.port.to_string();
                            input.update(cx, |input, cx| input.set_text(&port, cx));
                        }
                    }
                    view.focus_current(window, cx);
                }
                InputEvent::Cancel => view.focus_current(window, cx),
                _ => {}
            }),
            cx.subscribe_in(&code, window, |view, _, event, window, cx| match event {
                InputEvent::Submit => join(view, cx),
                InputEvent::NextField => {
                    if let Some(inputs) = page_inputs(view) {
                        inputs.address.read(cx).focus(window);
                    }
                }
                _ => cx.notify(),
            }),
            cx.subscribe_in(&address, window, |view, _, event, window, cx| match event {
                InputEvent::Submit => join(view, cx),
                InputEvent::NextField => {
                    if let Some(inputs) = page_inputs(view) {
                        inputs.code.read(cx).focus(window);
                    }
                }
                _ => cx.notify(),
            }),
        ];
        let Some(page) = view.settings.as_mut() else { return };
        page.uplink = Some(Inputs { name, port, address, code, manual: Vec::new(), open: None, subscriptions });
    }
    let devices: Vec<Device> = super::uplink(cx).map(|uplink| uplink.read(cx).devices.clone()).unwrap_or_default();
    let known: Vec<String> =
        page_inputs(view).map(|inputs| inputs.manual.iter().map(|(key, _)| key.clone()).collect()).unwrap_or_default();
    for device in devices.iter().filter(|device| !known.contains(&device.key)) {
        let field = input("names or addresses, comma separated", &device.manual.join(", "), cx);
        let Some(key) = device.public() else { continue };
        let subscription = cx.subscribe_in(&field, window, move |view, input, event, window, cx| match event {
            InputEvent::Submit | InputEvent::Cancel => {
                let text = input.read(cx).text.clone();
                let addresses: Vec<String> =
                    text.split([',', ' ']).map(str::trim).filter(|item| !item.is_empty()).map(String::from).collect();
                with_uplink(cx, |uplink, cx| uplink.set_manual(&key, addresses, cx));
                view.focus_current(window, cx);
            }
            _ => {}
        });
        if let Some(inputs) = view.settings.as_mut().and_then(|page| page.uplink.as_mut()) {
            inputs.manual.push((device.key.clone(), field));
            inputs.subscriptions.push(subscription);
        }
    }
}

fn field(input: &Entity<TextInput>, width: f32, theme: &Theme) -> gpui::Div {
    div().w(px(width)).px(px(4.0)).bg(theme.background).border_1().border_color(theme.border).child(input.clone())
}

fn segment(id: String, label: &'static str, active: bool, theme: &Theme) -> gpui::Stateful<gpui::Div> {
    div()
        .id(SharedString::from(id))
        .px(px(8.0))
        .when(active, |this| this.bg(theme.accent).text_color(theme.on_accent))
        .when(!active, |this| this.text_color(theme.muted).hover(|style| style.text_color(theme.text)))
        .child(label)
}

fn header(title: &'static str, theme: &Theme) -> gpui::Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.0))
        .px(px(12.0))
        .py(px(4.0))
        .border_b_1()
        .border_color(theme.border)
        .child(div().text_color(theme.faint).child(title))
}

fn qr(text: &str, theme: &Theme) -> Option<AnyElement> {
    let code = qrcode::QrCode::new(text.as_bytes()).ok()?;
    let width = code.width();
    let colors = code.to_colors();
    let module = 3.0;
    let quiet = 4;
    let side = (width + quiet * 2) as f32 * module;
    let (dark, light) = if theme.dark { (theme.background, theme.text) } else { (theme.text, theme.background) };
    let element = canvas(
        |_, _, _| {},
        move |bounds: Bounds<gpui::Pixels>, _, window, _| {
            window.paint_quad(fill(bounds, light));
            for (index, color) in colors.iter().enumerate() {
                if *color == qrcode::Color::Dark {
                    let (x, y) = ((index % width + quiet) as f32, (index / width + quiet) as f32);
                    let origin = point(bounds.origin.x + px(x * module), bounds.origin.y + px(y * module));
                    window.paint_quad(fill(Bounds::new(origin, size(px(module), px(module))), dark));
                }
            }
        },
    )
    .w(px(side))
    .h(px(side))
    .flex_none();
    Some(element.into_any_element())
}

fn countdown(left: std::time::Duration) -> String {
    let seconds = left.as_secs();
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

fn adding(inputs: &Inputs, uplink: &Uplink, cx: &Context<WorkspaceView>, theme: &Theme) -> AnyElement {
    let mut panel =
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(8.0))
            .border_b_1()
            .border_color(theme.border)
            .child(div().flex().items_center().justify_between().gap(px(8.0)).child("Add a device").child(
                button("uplink-cancel-add", "cancel", theme).on_click(|_, _, cx| {
                    with_uplink(cx, |uplink, cx| {
                        uplink.finish_join();
                        uplink.cancel_pairing(cx);
                    })
                }),
            ))
            .child(div().text_color(theme.faint).child(
                "Choose add device on the other computer too, then type the code of one computer into the other.",
            ));
    if let Some((code, left, attempts, _)) = uplink.pairing() {
        let excluded = &settings::get(cx).uplink.excluded_networks;
        let addresses = uplink.port().map(|port| invite::local_addresses(port, excluded)).unwrap_or_default();
        let link = invite::link(&code, uplink.identity(), &addresses);
        let copied = link.clone();
        panel =
            panel.child(
                div().flex().gap(px(16.0)).children(qr(&link, theme)).child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .min_w_0()
                        .child(div().text_color(theme.faint).child("Code of this computer"))
                        .child(
                            div()
                                .text_size(px(34.0))
                                .line_height(px(44.0))
                                .text_color(theme.accent)
                                .child(wire::format_code(&code)),
                        )
                        .child(div().text_color(theme.faint).child(format!(
                            "expires in {} · {} wrong tries left",
                            countdown(left),
                            super::net::PAIRING_ATTEMPTS - attempts
                        )))
                        .child(div().flex().child(button("uplink-copy-link", "copy pairing link", theme).on_click(
                            move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(copied.clone())),
                        ))),
                ),
            );
    }
    let tick = cx.global::<crate::store::StoreHandle>().0.read(cx).tick_fast;
    let paired: Vec<Key> = uplink.devices.iter().filter_map(Device::public).collect();
    let typed = inputs.address.read(cx).text.trim().to_string();
    let waiting: Vec<_> =
        uplink.nearby.iter().filter(|nearby| nearby.pairing && !paired.contains(&nearby.key)).collect();
    let mut nearby = div().flex().flex_wrap().items_center().gap(px(8.0));
    if waiting.is_empty() {
        nearby = nearby.child(div().text_color(theme.faint).child(
            "No other computer nearby is adding a device. Type its name or address, or paste its pairing link.",
        ));
    } else {
        nearby = nearby.child(div().text_color(theme.faint).child("Nearby"));
    }
    let single = waiting.len() == 1;
    for (index, device) in waiting.iter().enumerate() {
        let name = device.name.clone();
        let selected = typed == name || (typed.is_empty() && single);
        nearby = nearby.child(
            div()
                .id(("uplink-nearby", index))
                .flex()
                .gap(px(6.0))
                .px(px(8.0))
                .when(selected, |this| this.bg(theme.accent).text_color(theme.on_accent))
                .when(!selected, |this| this.text_color(theme.muted).hover(|style| style.text_color(theme.text)))
                .child(icons::DEVICE)
                .child(device.name.clone())
                .on_click(cx.listener(move |view, _, window, cx| {
                    if let Some(inputs) = page_inputs(view) {
                        inputs.address.update(cx, |input, cx| input.set_text(&name, cx));
                        inputs.code.read(cx).focus(window);
                    }
                })),
        );
    }
    let status = match &uplink.join {
        Join::Working => Some((format!("{} pairing…", icons::SPINNER[tick % icons::SPINNER.len()]), theme.accent)),
        Join::Failed(reason) => Some((reason.clone(), theme.red)),
        Join::Idle | Join::Done(_) => None,
    };
    panel
        .child(div().pt(px(4.0)).child("Code of the other computer"))
        .child(nearby)
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(field(&inputs.code, 220.0, theme).min_w_0())
                .when(!single, |this| this.child(field(&inputs.address, 220.0, theme).min_w_0()))
                .child(button("uplink-join", "join", theme).on_click(cx.listener(|view, _, _, cx| join(view, cx)))),
        )
        .children(status.map(|(text, color)| div().text_color(color).child(text)))
        .into_any_element()
}

fn device_row(
    device: &Device,
    inputs: &Inputs,
    uplink: &Uplink,
    cx: &Context<WorkspaceView>,
    theme: &Theme,
) -> Option<AnyElement> {
    let key = device.public()?;
    let link = uplink.links.get(&key);
    let watching = uplink.watched_count(&key);
    let state = match (device.blocked, link) {
        (true, _) => ("blocked".to_string(), theme.red),
        (false, Some(link)) => {
            let mut text = format!("online · {}", link.address);
            if watching > 0 {
                text.push_str(&format!(" · watching {watching} of your tabs"));
            }
            (text, theme.green)
        }
        (false, None) => {
            let mut text = format!("offline · {}", super::sidebar::seen(device));
            if let Some(error) = uplink.dial_error(&key) {
                text.push_str(&format!(" · {error}"));
            }
            (text, theme.faint)
        }
    };
    let open = inputs.open.as_deref() == Some(device.key.as_str());
    let manual = inputs.manual.iter().find(|(id, _)| *id == device.key).map(|(_, input)| input.clone());
    let access = device.access;
    let name = device.name.clone();
    let id = device.key.clone();
    let toggle = id.clone();
    Some(
        div()
            .flex()
            .flex_col()
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .pr(px(12.0))
                    .child(
                        div()
                            .id(SharedString::from(format!("uplink-device-{id}")))
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .items_center()
                            .gap(px(8.0))
                            .pl(px(12.0))
                            .py(px(6.0))
                            .hover(|style| style.bg(theme.hover))
                            .child(div().text_color(theme.faint).child(if open {
                                icons::CHEVRON_DOWN
                            } else {
                                icons::CHEVRON_RIGHT
                            }))
                            .child(
                                div()
                                    .text_color(if link.is_some() { theme.accent } else { theme.faint })
                                    .child(icons::os(&device.os)),
                            )
                            .child(div().flex_none().child(device.name.clone()))
                            .child(div().flex_1().min_w_0().truncate().text_color(state.1).child(state.0))
                            .on_click(cx.listener(move |view, _, _, cx| {
                                if let Some(inputs) = view.settings.as_mut().and_then(|page| page.uplink.as_mut()) {
                                    let open = inputs.open.as_deref() == Some(toggle.as_str());
                                    inputs.open = (!open).then(|| toggle.clone());
                                }
                                cx.notify();
                            })),
                    )
                    .child(div().text_color(theme.faint).child("can"))
                    .child(
                        segment(format!("uplink-control-{id}"), "control", access == Access::Control, theme).on_click(
                            move |_, _, cx| with_uplink(cx, |uplink, cx| uplink.set_access(&key, Access::Control, cx)),
                        ),
                    )
                    .child(segment(format!("uplink-view-{id}"), "view", access == Access::View, theme).on_click(
                        move |_, _, cx| with_uplink(cx, |uplink, cx| uplink.set_access(&key, Access::View, cx)),
                    )),
            )
            .when(open, |this| {
                this.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .pl(px(34.0))
                        .pr(px(12.0))
                        .pb(px(8.0))
                        .child(div().text_color(theme.faint).child(format!("fingerprint {}", wire::fingerprint(&key))))
                        .children(manual.map(|manual| {
                            div()
                                .flex()
                                .flex_wrap()
                                .items_center()
                                .gap_x(px(8.0))
                                .gap_y(px(4.0))
                                .child(div().text_color(theme.faint).child("also reach it at"))
                                .child(field(&manual, 300.0, theme).min_w_0())
                                .child(div().text_color(theme.faint).child("[enter] save"))
                        }))
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap(px(8.0))
                                .when(link.is_some(), |this| {
                                    this.child(button(format!("uplink-disconnect-{id}"), "disconnect", theme).on_click(
                                        move |_, _, cx| with_uplink(cx, |uplink, cx| uplink.disconnect(&key, cx)),
                                    ))
                                })
                                .child(
                                    button(
                                        format!("uplink-block-{id}"),
                                        if device.blocked { "unblock" } else { "block" },
                                        theme,
                                    )
                                    .on_click({
                                        let blocked = device.blocked;
                                        move |_, _, cx| {
                                            with_uplink(cx, |uplink, cx| uplink.set_blocked(&key, !blocked, cx))
                                        }
                                    }),
                                )
                                .child(
                                    button(format!("uplink-forget-{id}"), "forget", theme)
                                        .on_click(cx.listener(move |_, _, window, cx| forget(key, &name, window, cx))),
                                ),
                        ),
                )
            })
            .into_any_element(),
    )
}

fn forget(key: Key, name: &str, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let answer = window.prompt(
        PromptLevel::Warning,
        &format!("Forget {name}?"),
        Some("It loses access to this device right away and must be paired again to reconnect."),
        &["Forget", "Cancel"],
        cx,
    );
    cx.spawn(async move |_, cx| {
        if answer.await == Ok(0) {
            let _ = cx.update(|cx| with_uplink(cx, |uplink, cx| uplink.forget(&key, cx)));
        }
    })
    .detach();
}

fn devices(inputs: &Inputs, uplink: &Uplink, cx: &Context<WorkspaceView>, theme: &Theme) -> AnyElement {
    let busy = uplink.pairing().is_some() || matches!(uplink.join, Join::Working | Join::Failed(_));
    let mut list = group(theme).child(header("Devices", theme).when(!busy, |this| {
        this.child(
            button("uplink-add", "add device", theme)
                .on_click(|_, _, cx| with_uplink(cx, |uplink, cx| uplink.start_pairing(Access::Control, cx))),
        )
    }));
    if busy {
        list = list.child(adding(inputs, uplink, cx, theme));
    }
    if uplink.devices.is_empty() && !busy {
        list = list.child(
            div()
                .px(px(12.0))
                .py(px(8.0))
                .text_color(theme.faint)
                .child("None yet. Choose add device here and on the other computer."),
        );
    }
    for device in &uplink.devices {
        list = list.children(device_row(device, inputs, uplink, cx, theme));
    }
    list.into_any_element()
}

fn this_computer(
    inputs: &Inputs,
    uplink: &Uplink,
    current: &settings::Settings,
    cx: &App,
    theme: &Theme,
) -> AnyElement {
    let store = crate::store::store(cx).read(cx);
    let (mut total, mut shared) = (0, 0);
    for workspace in store.workspaces.iter().filter(|workspace| !workspace.closed) {
        for project in workspace.all_projects() {
            for id in project.tabs.iter().chain(&project.terminals) {
                if store.session(id).is_some() {
                    total += 1;
                    shared += usize::from(super::shared(id, cx));
                }
            }
        }
    }
    let excluded = &current.uplink.excluded_networks;
    let mut networks: Vec<(String, Option<String>)> = uplink
        .port()
        .map(invite::networks)
        .unwrap_or_default()
        .into_iter()
        .map(|(name, address)| (name, Some(address)))
        .collect();
    let missing: Vec<String> =
        excluded.iter().filter(|name| !networks.iter().any(|(known, _)| known == *name)).cloned().collect();
    networks.extend(missing.into_iter().map(|name| (name, None)));
    let mut list = div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .px(px(12.0))
        .py(px(8.0))
        .border_b_1()
        .border_color(theme.border)
        .child("Addresses")
        .child(
            div()
                .text_color(theme.faint)
                .child("Unchecked: connections arriving there are refused and this computer is not announced there"),
        );
    if networks.is_empty() {
        list = list.child(div().text_color(theme.faint).child("no network address found"));
    }
    for (index, (name, address)) in networks.into_iter().enumerate() {
        let on = !excluded.contains(&name);
        list = list.child(
            div()
                .id(("uplink-network", index))
                .flex()
                .gap(px(8.0))
                .hover(|style| style.bg(theme.hover))
                .child(div().text_color(if on { theme.accent } else { theme.muted }).child(if on {
                    "[x]"
                } else {
                    "[ ]"
                }))
                .child(
                    div()
                        .text_color(if on { theme.text } else { theme.faint })
                        .child(address.unwrap_or_else(|| "not connected".into())),
                )
                .child(div().text_color(theme.faint).child(name.clone()))
                .on_click(move |_, _, cx| {
                    settings::update(cx, |settings| {
                        let excluded = &mut settings.uplink.excluded_networks;
                        if on {
                            excluded.push(name.clone());
                        } else {
                            excluded.retain(|item| *item != name);
                        }
                    })
                }),
        );
    }
    group(theme)
        .child(header("This computer", theme))
        .child(row("Name", "How your other devices list this one", field(&inputs.name, 220.0, theme), theme))
        .child(row(
            "Share tabs by default",
            format!("{shared} of {total} open tabs shared. Right-click a tab or project to change it."),
            checkbox("uplink-share", current.uplink.share, |settings, on| settings.uplink.share = on, theme),
            theme,
        ))
        .child(row("Port", "Press enter to apply", field(&inputs.port, 90.0, theme), theme))
        .child(list)
        .child(row(
            "Fingerprint",
            "Other devices show it for this computer",
            div()
                .flex()
                .gap(px(6.0))
                .child(div().text_color(theme.green).child(icons::LOCK))
                .child(div().text_color(theme.accent).child(wire::fingerprint(uplink.identity()))),
            theme,
        ))
        .into_any_element()
}

pub fn render(view: &WorkspaceView, cx: &Context<WorkspaceView>) -> Vec<AnyElement> {
    let theme = ui::theme(cx);
    let current = settings::get(cx).clone();
    let Some(entity) = super::uplink(cx) else { return Vec::new() };
    let uplink = entity.read(cx);
    let status = match (current.uplink.enabled, &uplink.error, uplink.port()) {
        (false, _, _) => "Off: no port is open and nothing is announced.".to_string(),
        (true, Some(_), _) => "On, but not listening.".to_string(),
        (true, None, Some(port)) => format!(
            "On as {} · port {port} · {} of {} devices online",
            current.uplink.device_name(),
            uplink.links.len(),
            uplink.devices.len()
        ),
        (true, None, None) => "Starting…".to_string(),
    };
    let mut toggle = group(&theme).child(row(
        "Uplink",
        status,
        checkbox("uplink-enabled", current.uplink.enabled, |settings, on| settings.uplink.enabled = on, &theme),
        &theme,
    ));
    if current.uplink.enabled {
        toggle = toggle
            .children(
                uplink.error.clone().map(|error| div().px(px(12.0)).py(px(6.0)).text_color(theme.red).child(error)),
            )
            .children(
                uplink.note.clone().map(|note| div().px(px(12.0)).py(px(6.0)).text_color(theme.orange).child(note)),
            );
    }
    let mut pages = vec![toggle.into_any_element()];
    if !current.uplink.enabled {
        pages.push(
            group(&theme)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .px(px(12.0))
                        .py(px(10.0))
                        .child("How it works")
                        .child(div().text_color(theme.faint).child(
                            "Pair two computers once with a one-time code. They then connect directly to each other over your local network, or any private network you set up between them. Nothing goes through a server.",
                        ))
                        .child(div().text_color(theme.faint).child(
                            "Every connection is end-to-end encrypted and both sides prove who they are. You choose which tabs are shared and whether a device may type into them or only watch.",
                        )),
                )
                .into_any_element(),
        );
        return pages;
    }
    let Some(inputs) = page_inputs(view) else { return pages };
    pages.push(devices(inputs, uplink, cx, &theme));
    pages.push(this_computer(inputs, uplink, &current, cx, &theme));
    pages
}
