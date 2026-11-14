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
    access: Access,
    join_access: Access,
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

fn join(view: &mut WorkspaceView, cx: &mut Context<WorkspaceView>) {
    let Some(inputs) = view.settings.as_ref().and_then(|page| page.uplink.as_ref()) else { return };
    let (address, code) = (inputs.address.read(cx).text.clone(), inputs.code.read(cx).text.clone());
    let access = inputs.join_access;
    with_uplink(cx, |uplink, cx| uplink.start_join(&address, &code, access, cx));
}

pub fn ensure(view: &mut WorkspaceView, window: &mut Window, cx: &mut Context<WorkspaceView>) {
    let joined = super::uplink(cx).filter(|uplink| matches!(uplink.read(cx).join, Join::Done(_)));
    if let Some(uplink) = joined {
        uplink.update(cx, |uplink, _| uplink.finish_join());
        if let Some(inputs) = view.settings.as_ref().and_then(|page| page.uplink.as_ref()) {
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
        let address = input("pick a device above, or type a name or address", "", cx);
        let code = input("code, e.g. K7QM-4XRP, or a pairing link", "", cx);
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
            cx.subscribe_in(&address, window, |view, _, event, window, cx| match event {
                InputEvent::Submit => join(view, cx),
                InputEvent::NextField => {
                    if let Some(inputs) = view.settings.as_ref().and_then(|page| page.uplink.as_ref()) {
                        inputs.code.read(cx).focus(window);
                    }
                }
                _ => cx.notify(),
            }),
            cx.subscribe_in(&code, window, |view, _, event, _, cx| match event {
                InputEvent::Submit => join(view, cx),
                _ => cx.notify(),
            }),
        ];
        let Some(page) = view.settings.as_mut() else { return };
        page.uplink = Some(Inputs {
            name,
            port,
            address,
            code,
            manual: Vec::new(),
            access: Access::Control,
            join_access: Access::Control,
            subscriptions,
        });
    }
    let devices: Vec<Device> = super::uplink(cx).map(|uplink| uplink.read(cx).devices.clone()).unwrap_or_default();
    let known: Vec<String> = view
        .settings
        .as_ref()
        .and_then(|page| page.uplink.as_ref())
        .map(|inputs| inputs.manual.iter().map(|(key, _)| key.clone()).collect())
        .unwrap_or_default();
    for device in devices.iter().filter(|device| !known.contains(&device.key)) {
        let field = input("name or address used to reach it", &device.manual.join(", "), cx);
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

fn this_device(inputs: &Inputs, uplink: &Uplink, theme: &Theme) -> AnyElement {
    let listening = uplink.port();
    let addresses = listening.map(invite::local_addresses).unwrap_or_default();
    let reach = if addresses.is_empty() { "no network address found".to_string() } else { addresses.join("  ") };
    group(theme)
        .child(row("Name", "How your other devices list this one", field(&inputs.name, 220.0, theme), theme))
        .child(row(
            "Port",
            "TCP port other devices connect to; press enter to apply",
            field(&inputs.port, 90.0, theme),
            theme,
        ))
        .child(
            div()
                .flex()
                .flex_col()
                .px(px(12.0))
                .py(px(8.0))
                .border_b_1()
                .border_color(theme.border)
                .child("Reachable at")
                .child(div().text_color(theme.faint).child(reach)),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .px(px(12.0))
                .py(px(8.0))
                .child(
                    div()
                        .flex()
                        .gap(px(6.0))
                        .child(div().text_color(theme.green).child(icons::LOCK))
                        .child("Fingerprint"),
                )
                .child(div().text_color(theme.accent).child(wire::fingerprint(uplink.identity())))
                .child(div().text_color(theme.faint).child(
                    "Connections are end-to-end encrypted. The other device shows this fingerprint for this device.",
                )),
        )
        .into_any_element()
}

fn sharing(current: &settings::Settings, cx: &App, theme: &Theme) -> AnyElement {
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
    group(theme)
        .child(row(
            "Share tabs by default",
            "Off: only tabs and projects you share from their menu",
            checkbox("uplink-share", current.uplink.share, |settings, on| settings.uplink.share = on, theme),
            theme,
        ))
        .child(
            div()
                .px(px(12.0))
                .py(px(8.0))
                .text_color(theme.faint)
                .child(format!(
                    "{shared} of {total} open tabs are shared. Right-click a tab or project to change it. Paired devices see only shared tabs, and only devices with control can type into them."
                )),
        )
        .into_any_element()
}

fn pairing(inputs: &Inputs, uplink: &Uplink, cx: &Context<WorkspaceView>, theme: &Theme) -> AnyElement {
    let access = inputs.access;
    let body = match uplink.pairing() {
        None => {
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(16.0))
                .px(px(12.0))
                .py(px(8.0))
                .child(
                    div().flex().flex_col().child("Pair a new device").child(
                        div().text_color(theme.faint).child("Shows a one-time code to enter on the other device."),
                    ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(div().text_color(theme.faint).child("it can"))
                        .child(
                            segment("uplink-pair-control".into(), "control", access == Access::Control, theme)
                                .on_click(cx.listener(|view, _, _, cx| set_pair_access(view, Access::Control, cx))),
                        )
                        .child(
                            segment("uplink-pair-view".into(), "view", access == Access::View, theme)
                                .on_click(cx.listener(|view, _, _, cx| set_pair_access(view, Access::View, cx))),
                        )
                        .child(button("uplink-pair", "pair", theme).on_click(cx.listener(move |view, _, _, cx| {
                            let access = view
                                .settings
                                .as_ref()
                                .and_then(|page| page.uplink.as_ref())
                                .map_or(Access::Control, |inputs| inputs.access);
                            with_uplink(cx, |uplink, cx| uplink.start_pairing(access, cx));
                        }))),
                )
        }
        Some((code, left, attempts, access)) => {
            let addresses = uplink.port().map(invite::local_addresses).unwrap_or_default();
            let link = invite::link(&code, uplink.identity(), &addresses);
            let copied = link.clone();
            div()
                .flex()
                .gap(px(20.0))
                .px(px(12.0))
                .py(px(12.0))
                .children(qr(&link, theme))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .min_w_0()
                        .child(
                            div()
                                .text_size(px(34.0))
                                .line_height(px(44.0))
                                .text_color(theme.accent)
                                .child(wire::format_code(&code)),
                        )
                        .child(div().text_color(theme.faint).child(format!(
                            "expires in {} · one device · {} · {} wrong tries left",
                            countdown(left),
                            if access == Access::Control { "it can control" } else { "it can view" },
                            super::net::PAIRING_ATTEMPTS - attempts
                        )))
                        .child(div().child(
                            "On the other device open Settings › Uplink, choose this device under Join, then enter the code.",
                        ))
                        .child(div().text_color(theme.faint).child(
                            "No other device nearby? Copy the pairing link and paste it into the code field there.",
                        ))
                        .child(
                            div()
                                .flex()
                                .gap(px(8.0))
                                .child(button("uplink-copy-link", "copy pairing link", theme).on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(copied.clone()))
                                }))
                                .child(button("uplink-cancel-pair", "cancel", theme).on_click(|_, _, cx| {
                                    with_uplink(cx, |uplink, cx| uplink.cancel_pairing(cx))
                                })),
                        ),
                )
        }
    };
    group(theme).child(body).into_any_element()
}

fn set_pair_access(view: &mut WorkspaceView, access: Access, cx: &mut Context<WorkspaceView>) {
    if let Some(inputs) = view.settings.as_mut().and_then(|page| page.uplink.as_mut()) {
        inputs.access = access;
    }
    cx.notify();
}

fn set_join_access(view: &mut WorkspaceView, access: Access, cx: &mut Context<WorkspaceView>) {
    if let Some(inputs) = view.settings.as_mut().and_then(|page| page.uplink.as_mut()) {
        inputs.join_access = access;
    }
    cx.notify();
}

fn joining(inputs: &Inputs, uplink: &Uplink, cx: &Context<WorkspaceView>, theme: &Theme) -> AnyElement {
    let tick = cx.global::<crate::store::StoreHandle>().0.read(cx).tick_fast;
    let paired: Vec<Key> = uplink.devices.iter().filter_map(Device::public).collect();
    let typed = inputs.address.read(cx).text.trim().to_string();
    let mut nearby = div().flex().flex_col();
    let waiting: Vec<_> =
        uplink.nearby.iter().filter(|nearby| nearby.pairing && !paired.contains(&nearby.key)).collect();
    for (index, device) in waiting.iter().enumerate() {
        let name = device.name.clone();
        let selected = typed == name;
        let address = device.addresses.first().map(|address| address.to_string()).unwrap_or_default();
        nearby = nearby.child(
            div()
                .id(("uplink-nearby", index))
                .flex()
                .gap(px(8.0))
                .px(px(12.0))
                .when(selected, |this| this.bg(theme.accent).text_color(theme.on_accent))
                .when(!selected, |this| this.hover(|style| style.bg(theme.hover)))
                .child(icons::DEVICE)
                .child(div().flex_1().child(device.name.clone()))
                .child(div().text_color(if selected { theme.on_accent } else { theme.faint }).child(address))
                .on_click(cx.listener(move |view, _, window, cx| {
                    if let Some(inputs) = view.settings.as_ref().and_then(|page| page.uplink.as_ref()) {
                        inputs.address.update(cx, |input, cx| input.set_text(&name, cx));
                        inputs.code.read(cx).focus(window);
                    }
                })),
        );
    }
    if waiting.is_empty() {
        nearby = nearby.child(
            div()
                .px(px(12.0))
                .text_color(theme.faint)
                .child("No device nearby is pairing. Start pairing on the other device, or type its address."),
        );
    }
    let status = match &uplink.join {
        Join::Working => Some((format!("{} pairing…", icons::SPINNER[tick % icons::SPINNER.len()]), theme.accent)),
        Join::Failed(reason) => Some((reason.clone(), theme.red)),
        Join::Idle | Join::Done(_) => None,
    };
    group(theme)
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .py(px(8.0))
                .border_b_1()
                .border_color(theme.border)
                .child(div().px(px(12.0)).child("Join a device"))
                .child(nearby),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(12.0))
                .py(px(8.0))
                .child(field(&inputs.address, 280.0, theme))
                .child(field(&inputs.code, 220.0, theme))
                .child(button("uplink-join", "join", theme).on_click(cx.listener(|view, _, _, cx| join(view, cx)))),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(12.0))
                .pb(px(8.0))
                .child(div().text_color(theme.faint).child("the device you join can"))
                .child(
                    segment("uplink-join-control".into(), "control", inputs.join_access == Access::Control, theme)
                        .on_click(cx.listener(|view, _, _, cx| set_join_access(view, Access::Control, cx))),
                )
                .child(
                    segment("uplink-join-view".into(), "view", inputs.join_access == Access::View, theme)
                        .on_click(cx.listener(|view, _, _, cx| set_join_access(view, Access::View, cx))),
                )
                .child(div().text_color(theme.faint).child("this computer's shared tabs")),
        )
        .children(status.map(|(text, color)| div().px(px(12.0)).pb(px(8.0)).text_color(color).child(text)))
        .into_any_element()
}

fn device_card(
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
    let manual = inputs.manual.iter().find(|(id, _)| *id == device.key).map(|(_, input)| input.clone());
    let access = device.access;
    let name = device.name.clone();
    let id = device.key.clone();
    Some(
        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .px(px(12.0))
            .py(px(8.0))
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_color(if link.is_some() { theme.accent } else { theme.faint })
                            .child(icons::os(&device.os)),
                    )
                    .child(div().child(device.name.clone()))
                    .child(div().flex_1().min_w_0().truncate().text_color(state.1).child(state.0)),
            )
            .child(div().pl(px(22.0)).text_color(theme.faint).child(format!("fingerprint {}", wire::fingerprint(&key))))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .pl(px(22.0))
                    .child(div().text_color(theme.faint).child("it can"))
                    .child(
                        segment(format!("uplink-control-{id}"), "control", access == Access::Control, theme).on_click(
                            move |_, _, cx| with_uplink(cx, |uplink, cx| uplink.set_access(&key, Access::Control, cx)),
                        ),
                    )
                    .child(segment(format!("uplink-view-{id}"), "view", access == Access::View, theme).on_click(
                        move |_, _, cx| with_uplink(cx, |uplink, cx| uplink.set_access(&key, Access::View, cx)),
                    ))
                    .child(div().text_color(theme.faint).child("your shared tabs"))
                    .child(div().flex_1())
                    .when(link.is_some(), |this| {
                        this.child(
                            button(format!("uplink-disconnect-{id}"), "disconnect", theme)
                                .on_click(move |_, _, cx| with_uplink(cx, |uplink, cx| uplink.disconnect(&key, cx))),
                        )
                    })
                    .child(
                        button(format!("uplink-block-{id}"), if device.blocked { "unblock" } else { "block" }, theme)
                            .on_click({
                                let blocked = device.blocked;
                                move |_, _, cx| with_uplink(cx, |uplink, cx| uplink.set_blocked(&key, !blocked, cx))
                            }),
                    )
                    .child(
                        button(format!("uplink-forget-{id}"), "forget", theme)
                            .on_click(cx.listener(move |_, _, window, cx| forget(key, &name, window, cx))),
                    ),
            )
            .children(manual.map(|manual| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .pl(px(22.0))
                    .child(div().text_color(theme.faint).child("also try"))
                    .child(field(&manual, 300.0, theme))
                    .child(div().text_color(theme.faint).child("[enter] save"))
            }))
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
    let mut list = group(theme).child(
        div()
            .px(px(12.0))
            .py(px(4.0))
            .border_b_1()
            .border_color(theme.border)
            .text_color(theme.faint)
            .child(format!("Paired devices {}", uplink.devices.len())),
    );
    if uplink.devices.is_empty() {
        list = list
            .child(div().px(px(12.0)).py(px(8.0)).text_color(theme.faint).child("None yet. Pair or join one below."));
    }
    let devices = uplink.devices.clone();
    for device in &devices {
        list = list.children(device_card(device, inputs, uplink, cx, theme));
    }
    list.into_any_element()
}

pub fn render(view: &WorkspaceView, cx: &Context<WorkspaceView>) -> Vec<AnyElement> {
    let theme = ui::theme(cx);
    let current = settings::get(cx).clone();
    let Some(entity) = super::uplink(cx) else { return Vec::new() };
    let mut pages = vec![
        group(&theme)
            .child(row(
                "Uplink",
                "Reach the agents and terminals of your other computers. Off: no port is open and nothing is announced.",
                checkbox("uplink-enabled", current.uplink.enabled, |settings, on| settings.uplink.enabled = on, &theme),
                &theme,
            ))
            .into_any_element(),
    ];
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
    let Some(inputs) = view.settings.as_ref().and_then(|page| page.uplink.as_ref()) else { return pages };
    let uplink = entity.read(cx);
    let online = uplink.links.len();
    let status = match (&uplink.error, uplink.port()) {
        (Some(error), _) => (error.clone(), theme.red),
        (None, Some(port)) => {
            (format!("listening on port {port} · {online} of {} devices online", uplink.devices.len()), theme.green)
        }
        (None, None) => ("starting…".to_string(), theme.faint),
    };
    pages.push(
        div()
            .flex()
            .flex_col()
            .child(div().text_color(status.1).child(status.0))
            .children(uplink.note.clone().map(|note| div().text_color(theme.orange).child(note)))
            .into_any_element(),
    );
    pages.push(devices(inputs, uplink, cx, &theme));
    pages.push(pairing(inputs, uplink, cx, &theme));
    pages.push(joining(inputs, uplink, cx, &theme));
    pages.push(sharing(&current, cx, &theme));
    pages.push(this_device(inputs, uplink, &theme));
    pages
}
