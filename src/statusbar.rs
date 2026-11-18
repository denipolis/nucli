use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::SystemTime;

use gpui::{
    AnyElement, App, AppContext, Context, Div, Entity, InteractiveElement, IntoElement, MouseButton, MouseDownEvent,
    ParentElement, Pixels, Point, PromptLevel, Stateful, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::agent_info::AgentInfo;
use crate::editor::Editor;
use crate::icons::{self, CLOCK, INSTRUCTIONS, LIMITS, MCP, MODEL, SKILLS};
use crate::menu;
use crate::palette::{Item, Palette};
use crate::paths;
use crate::session::Session;
use crate::settings::{self, SettingsError, StatusChips};
use crate::sidebar::DragPreview;
use crate::theme::Theme;
use crate::transcript::Entry;
use crate::ui;
use crate::window::WorkspaceView;
use nucli_core::chips::{duration, level, running, tokens};

type Chips = Vec<(Chip, AnyElement)>;

#[derive(Clone, Copy)]
struct DraggedChip(Chip);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Chip {
    Folder,
    Branch,
    Changes,
    Program,
    Uplink,
    Skills,
    Instructions,
    Model,
    Context,
    Usage,
    Mcp,
    Session,
    Position,
    Language,
    FileChanges,
    Commit,
}

impl Chip {
    pub const ALL: [Chip; 16] = [
        Chip::Folder,
        Chip::Branch,
        Chip::Changes,
        Chip::Program,
        Chip::Uplink,
        Chip::Skills,
        Chip::Instructions,
        Chip::Model,
        Chip::Context,
        Chip::Usage,
        Chip::Mcp,
        Chip::Session,
        Chip::Position,
        Chip::Language,
        Chip::FileChanges,
        Chip::Commit,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Chip::Folder => "folder",
            Chip::Branch => "branch",
            Chip::Changes => "changes",
            Chip::Program => "program",
            Chip::Uplink => "uplink",
            Chip::Skills => "skills",
            Chip::Instructions => "instructions",
            Chip::Model => "model",
            Chip::Context => "context",
            Chip::Usage => "usage",
            Chip::Mcp => "mcp",
            Chip::Session => "session",
            Chip::Position => "position",
            Chip::Language => "language",
            Chip::FileChanges => "file_changes",
            Chip::Commit => "commit",
        }
    }

    fn from_id(id: &str) -> Option<Chip> {
        Chip::ALL.into_iter().find(|chip| chip.id() == id)
    }

    pub fn ordered(cx: &App) -> Vec<Chip> {
        let mut chips = Vec::new();
        for chip in settings::get(cx).chip_order.iter().filter_map(|id| Chip::from_id(id)).chain(Chip::ALL) {
            if !chips.contains(&chip) {
                chips.push(chip);
            }
        }
        chips
    }

    fn move_to(self, target: Chip, cx: &mut App) {
        let mut order = Chip::ordered(cx);
        let Some(index) = order.iter().position(|chip| *chip == target).filter(|_| self != target) else {
            return;
        };
        order.retain(|chip| *chip != self);
        order.insert(index, self);
        settings::update(cx, |settings| settings.chip_order = order.iter().map(|chip| chip.id().to_string()).collect());
    }

    pub fn title(self) -> &'static str {
        match self {
            Chip::Folder => "Folder",
            Chip::Branch => "Branch",
            Chip::Changes => "Changes",
            Chip::Program => "Program",
            Chip::Uplink => "Uplink",
            Chip::Skills => "Skills",
            Chip::Instructions => "Instruction files",
            Chip::Model => "Model",
            Chip::Context => "Context",
            Chip::Usage => "Usage limits",
            Chip::Mcp => "MCP servers",
            Chip::Session => "Session time",
            Chip::Position => "Line and column",
            Chip::Language => "Language",
            Chip::FileChanges => "File changes",
            Chip::Commit => "Commit",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Chip::Folder => "The shell's current folder",
            Chip::Branch => "Git branch of that folder",
            Chip::Changes => "Uncommitted changes of the project",
            Chip::Program => "The program running in the tab",
            Chip::Uplink => "The computer a remote tab runs on, or the devices watching this tab",
            Chip::Skills => "Skills the agent can load, the used ones first",
            Chip::Instructions => "Instruction files the agent loaded (CLAUDE.md, AGENTS.md, …)",
            Chip::Model => "The agent's current model",
            Chip::Context => "Context window used by the conversation",
            Chip::Usage => "5-hour and 7-day usage of the Claude Code or Codex plan",
            Chip::Mcp => "MCP servers of the agent, the called ones first",
            Chip::Session => "How long the agent has been running",
            Chip::Position => "Line and column of the cursor",
            Chip::Language => "Language of the file",
            Chip::FileChanges => "Added and removed lines of the file",
            Chip::Commit => "Commit of a file shown from history",
        }
    }

    pub fn group(self) -> &'static str {
        match self {
            Chip::Folder | Chip::Branch | Chip::Changes | Chip::Program | Chip::Uplink => "Terminal",
            Chip::Position | Chip::Language | Chip::FileChanges | Chip::Commit => "File view",
            _ => "Agent",
        }
    }

    pub fn shown(self, cx: &App) -> bool {
        let settings = settings::get(cx);
        match self {
            Chip::Usage => settings.usage_limits,
            _ => !settings.hidden_chips.iter().any(|id| id == self.id()),
        }
    }

    pub fn asks(self, cx: &App) -> bool {
        self == Chip::Usage && !self.shown(cx)
    }

    pub fn toggle(self, window: &mut Window, cx: &mut App) {
        if self.asks(cx) {
            ask_usage(window, cx);
            return;
        }
        settings::update(cx, |settings| {
            if self == Chip::Usage {
                settings.usage_limits = false;
            } else if settings.hidden_chips.iter().any(|id| id == self.id()) {
                settings.hidden_chips.retain(|id| id != self.id());
            } else {
                settings.hidden_chips.push(self.id().to_string());
            }
        });
    }
}

fn ask_usage(window: &mut Window, cx: &mut App) {
    let actions = ["Turn On", "Details", "Cancel"];
    let answer = window.prompt(PromptLevel::Info, "Show usage limits?", Some(USAGE_NOTICE), &actions, cx);
    let handle = window.window_handle();
    cx.spawn(async move |cx| match answer.await {
        Ok(0) => cx.update(|cx| settings::update(cx, |settings| settings.usage_limits = true)),
        Ok(1) => cx.update(|cx| {
            cx.open_url(USAGE_DETAILS);
            let _ = handle.update(cx, |_, window, cx| ask_usage(window, cx));
        }),
        _ => {}
    })
    .detach();
}

const USAGE_NOTICE: &str = "Some agents report their usage limits only through their API. For those, Nucli reads your usage with your own sign-in token for that agent. The token is never stored or sent anywhere else.";
const USAGE_DETAILS: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/blob/main/docs/USAGE_LIMITS.md");

fn frame(id: &'static str, theme: &Theme) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .gap(px(4.0))
        .px(px(4.0))
        .bg(theme.elevated)
        .border_1()
        .border_color(theme.border)
        .text_color(theme.text)
}

fn chip(id: &'static str, glyph: &'static str, theme: &Theme) -> Stateful<Div> {
    frame(id, theme).child(div().flex_none().text_color(theme.muted).child(glyph))
}

fn clickable(element: Stateful<Div>, kind: Chip, theme: &Theme, cx: &Context<WorkspaceView>) -> Stateful<Div> {
    let accent = theme.accent;
    element
        .hover(|style| style.bg(theme.hover))
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                view.open_menu(event.position, chip_menu(Some(kind), cx), cx);
            }),
        )
        .on_drag(DraggedChip(kind), |drag: &DraggedChip, _, _, cx| cx.new(|_| DragPreview(drag.0.title().into())))
        .drag_over::<DraggedChip>(move |style, _, _, _| style.border_color(accent))
        .on_drop(move |drag: &DraggedChip, _, cx| drag.0.move_to(kind, cx))
}

fn arrange(mut chips: Chips, cx: &App) -> Vec<AnyElement> {
    let order = Chip::ordered(cx);
    chips.sort_by_key(|(chip, _)| order.iter().position(|other| other == chip));
    chips.into_iter().map(|(_, element)| element).collect()
}

pub fn chip_menu(chip: Option<Chip>, cx: &App) -> Vec<menu::Item> {
    let mut items = Vec::new();
    if let Some(chip) = chip {
        items.push(menu::action(format!("Hide {}", chip.title()), move |_, window, cx| chip.toggle(window, cx)));
        items.push(menu::Item::Separator);
    }
    let mut group = "";
    for chip in Chip::ALL {
        if chip.group() != group {
            group = chip.group();
            items.push(menu::Item::Header(group.into()));
        }
        let mark = if chip.shown(cx) { "[x]" } else { "[ ]" };
        items.push(menu::action(format!("{mark} {}", chip.title()), move |_, window, cx| chip.toggle(window, cx)));
    }
    items.push(menu::Item::Separator);
    items.push(menu::command("Status Chips…", "configure_status_chips", cx));
    items
}

pub fn every_pane(cx: &App) -> bool {
    settings::get(cx).status_chips == StatusChips::EveryPane
}

fn row(theme: &Theme, cx: &Context<WorkspaceView>) -> Div {
    let size = (settings::get(cx).ui_font_size() - 2.0).max(8.0);
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(4.0))
        .pl(px(10.0))
        .pr(px(10.0))
        .pb(px(4.0))
        .text_size(px(size))
        .line_height(px((size * 1.5).round()))
        .bg(theme.background)
        .overflow_hidden()
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(|view, event: &MouseDownEvent, _, cx| view.open_menu(event.position, chip_menu(None, cx), cx)),
        )
}

fn file_chips(editor: &Entity<Editor>, theme: &Theme, cx: &Context<WorkspaceView>) -> Chips {
    let file = crate::file_view::file_status(editor, cx);
    let mut chips = Chips::new();
    if let Some(position) = file.position.filter(|_| Chip::Position.shown(cx)) {
        chips.push((
            Chip::Position,
            clickable(frame("status-position", theme), Chip::Position, theme, cx).child(position).into_any_element(),
        ));
    }
    if let Some(language) = file.language.filter(|_| Chip::Language.shown(cx)) {
        let (glyph, color) = icons::file(&file.icon_file);
        chips.push((
            Chip::Language,
            clickable(frame("status-language", theme), Chip::Language, theme, cx)
                .child(div().flex_none().text_color(color.color(theme.dark)).child(glyph))
                .child(language)
                .on_click(|_, window, cx| window.dispatch_action(Box::new(crate::commands::SelectLanguage), cx))
                .into_any_element(),
        ));
    }
    if file.added + file.removed > 0 && Chip::FileChanges.shown(cx) {
        chips.push((
            Chip::FileChanges,
            clickable(frame("status-file-changes", theme), Chip::FileChanges, theme, cx)
                .child(div().text_color(theme.green).child(format!("+{}", file.added)))
                .child(div().text_color(theme.red).child(format!("-{}", file.removed)))
                .into_any_element(),
        ));
    }
    if let Some(commit) = file.commit.filter(|_| Chip::Commit.shown(cx)) {
        chips.push((
            Chip::Commit,
            clickable(chip("status-commit", icons::COMMIT, theme), Chip::Commit, theme, cx)
                .child(commit[..commit.len().min(7)].to_string())
                .into_any_element(),
        ));
    }
    chips
}

fn changes_chip(
    stat: Option<crate::git::DiffStat>,
    theme: &Theme,
    cx: &Context<WorkspaceView>,
) -> Option<Stateful<Div>> {
    let stat = stat.filter(|stat| stat.files > 0 && Chip::Changes.shown(cx))?;
    Some(
        clickable(chip("status-changes", "±", theme), Chip::Changes, theme, cx)
            .child(stat.files.to_string())
            .child(div().text_color(theme.green).child(format!("+{}", stat.added)))
            .child(div().text_color(theme.red).child(format!("-{}", stat.removed))),
    )
}

fn program_chip(session: &Session, theme: &Theme, cx: &Context<WorkspaceView>) -> Option<(Chip, AnyElement)> {
    let name = session.program().map(str::to_string).filter(|_| Chip::Program.shown(cx))?;
    let element = clickable(chip("status-program", icons::PLAY, theme), Chip::Program, theme, cx).child(name);
    Some((Chip::Program, element.into_any_element()))
}

fn session_chips(
    view: &WorkspaceView,
    session: &Entity<Session>,
    theme: &Theme,
    cx: &mut Context<WorkspaceView>,
) -> Chips {
    let session = session.read(cx);
    if let Some(remote) = session.remote.clone() {
        return mirror_chips(session, remote, theme, cx);
    }
    let folder = session.working_dir().to_path_buf();
    let (reveal, copy) = (folder.clone(), folder.clone());
    let mut chips = Chips::new();
    if Chip::Folder.shown(cx) {
        chips.push((
            Chip::Folder,
            clickable(chip("status-folder", icons::FOLDER, theme), Chip::Folder, theme, cx)
                .flex_shrink(1.)
                .min_w_0()
                .child(div().min_w_0().truncate().child(paths::display(&folder)))
                .on_click(cx.listener(move |view, event: &gpui::ClickEvent, _, cx| {
                    let reveal = reveal.clone();
                    let items = vec![
                        menu::action(crate::actions::reveal_label(), move |_, _, cx| cx.reveal_path(&reveal)),
                        menu::copy("Copy Path", copy.to_string_lossy()),
                    ];
                    view.open_menu(event.position(), items, cx);
                }))
                .into_any_element(),
        ));
    }
    if let Some(branch) = session.branch.clone().filter(|_| Chip::Branch.shown(cx)) {
        chips.push((
            Chip::Branch,
            clickable(chip("status-branch", icons::BRANCH, theme), Chip::Branch, theme, cx)
                .child(branch)
                .on_click(cx.listener(|view, _, window, cx| crate::palettes::branches(view, window, cx)))
                .into_any_element(),
        ));
    }
    let project = view.state(cx).and_then(|state| state.project_of(&session.id)).map(|project| project.path.clone());
    let stat = project.as_ref().and_then(|path| view.store(cx).diffstats.get(path)).copied();
    if let Some(element) = changes_chip(stat, theme, cx) {
        let element =
            element.on_click(|_, window, cx| window.dispatch_action(Box::new(crate::commands::ShowChanges), cx));
        chips.push((Chip::Changes, element.into_any_element()));
    }
    chips.extend(program_chip(session, theme, cx));
    chips.extend(watched_chip(session, theme, cx));
    if let Some(info) = session.agent_info.clone() {
        let root = project.unwrap_or_else(|| folder.clone());
        chips.extend(agent_chips(Rc::new(info), Some(root), theme, cx));
    }
    chips
}

fn watched_chip(session: &Session, theme: &Theme, cx: &Context<WorkspaceView>) -> Option<(Chip, AnyElement)> {
    let uplink = crate::uplink::uplink(cx)?;
    let uplink = uplink.read(cx);
    let watchers = uplink.watchers_of(&session.id);
    if watchers.is_empty() || !Chip::Uplink.shown(cx) {
        return None;
    }
    let names: Vec<(crate::uplink::wire::Key, String)> =
        watchers.iter().map(|key| (*key, uplink.device_name(key))).collect();
    let label = names.iter().map(|(_, name)| name.as_str()).collect::<Vec<_>>().join(", ");
    let id = session.id.clone();
    let element = clickable(chip("status-uplink", icons::EYE, theme), Chip::Uplink, theme, cx)
        .child(div().text_color(theme.accent).child(label))
        .on_click(cx.listener(move |view, event: &gpui::ClickEvent, _, cx| {
            let tab = id.clone();
            let mut items = vec![menu::action("Stop Sharing This Tab", move |_, _, cx| {
                crate::actions::set_tab_shared(&tab, false, cx)
            })];
            for (key, name) in names.clone() {
                items.push(menu::action(format!("Disconnect {name}"), move |_, _, cx| {
                    if let Some(uplink) = crate::uplink::uplink(cx) {
                        uplink.update(cx, |uplink, cx| uplink.disconnect(&key, cx));
                    }
                }));
            }
            view.open_menu(event.position(), items, cx);
        }));
    Some((Chip::Uplink, element.into_any_element()))
}

fn mirror_chips(
    session: &Session,
    remote: crate::session::Remote,
    theme: &Theme,
    cx: &Context<WorkspaceView>,
) -> Chips {
    let mut chips = Chips::new();
    if Chip::Uplink.shown(cx) {
        let (id, key) = (session.id.clone(), remote.key);
        let online = remote.online;
        chips.push((
            Chip::Uplink,
            clickable(chip("status-uplink", icons::UPLINK, theme), Chip::Uplink, theme, cx)
                .child(div().text_color(if online { theme.accent } else { theme.faint }).child(remote.device.clone()))
                .when(!online, |this| this.child(div().text_color(theme.red).child("offline")))
                .when(online && !remote.control, |this| this.child(div().text_color(theme.orange).child("view only")))
                .on_click(cx.listener(move |view, event: &gpui::ClickEvent, _, cx| {
                    let close = id.clone();
                    let mut items = vec![menu::action("Close View", move |_, _, cx| {
                        if let Some(uplink) = crate::uplink::uplink(cx) {
                            uplink.update(cx, |uplink, cx| uplink.close_mirror(&close, cx));
                        }
                    })];
                    if online {
                        items.push(menu::action("Disconnect", move |_, _, cx| {
                            if let Some(uplink) = crate::uplink::uplink(cx) {
                                uplink.update(cx, |uplink, cx| uplink.disconnect(&key, cx));
                            }
                        }));
                    }
                    items.push(menu::Item::Separator);
                    items.push(menu::action("Uplink Settings…", |view, window, cx| {
                        crate::settings_view::show(view, Some(crate::persist::SettingsSection::Uplink), window, cx)
                    }));
                    view.open_menu(event.position(), items, cx);
                }))
                .into_any_element(),
        ));
    }
    if let Some(folder) = remote.cwd.clone().filter(|_| Chip::Folder.shown(cx)) {
        let copy = folder.clone();
        chips.push((
            Chip::Folder,
            clickable(chip("status-folder", icons::FOLDER, theme), Chip::Folder, theme, cx)
                .flex_shrink(1.)
                .min_w_0()
                .child(div().min_w_0().truncate().child(folder))
                .on_click(cx.listener(move |view, event: &gpui::ClickEvent, _, cx| {
                    view.open_menu(event.position(), vec![menu::copy("Copy Path", copy.clone())], cx);
                }))
                .into_any_element(),
        ));
    }
    if let Some(branch) = remote.branch.clone().filter(|_| Chip::Branch.shown(cx)) {
        chips.push((
            Chip::Branch,
            clickable(chip("status-branch", icons::BRANCH, theme), Chip::Branch, theme, cx)
                .child(branch)
                .into_any_element(),
        ));
    }
    if let Some(element) = changes_chip(remote.changes, theme, cx) {
        chips.push((Chip::Changes, element.into_any_element()));
    }
    chips.extend(program_chip(session, theme, cx));
    if let Some(info) = session.agent_info.clone() {
        chips.extend(agent_chips(Rc::new(info), None, theme, cx));
    }
    chips
}

fn agent_chips(info: Rc<AgentInfo>, root: Option<PathBuf>, theme: &Theme, cx: &Context<WorkspaceView>) -> Chips {
    let mut chips = Chips::new();
    let used = |entries: &[Entry]| entries.iter().filter(|entry| entry.used).count();
    let count = |element: Stateful<Div>, total: usize, label: &str, used: usize| {
        element
            .child(format!("{total} {label}"))
            .when(used > 0, |element| element.child(div().text_color(theme.accent).child(format!("{used} used"))))
    };
    let list = |chip: Chip, entries: Vec<Entry>, root: Option<PathBuf>| {
        move |view: &mut WorkspaceView,
              event: &gpui::ClickEvent,
              window: &mut Window,
              cx: &mut Context<WorkspaceView>| {
            entries_palette(view, chip, &entries, root.as_deref(), event.position(), window, cx)
        }
    };
    if !info.skills.is_empty() && Chip::Skills.shown(cx) {
        let element = clickable(chip("status-skills", SKILLS, theme), Chip::Skills, theme, cx);
        chips.push((
            Chip::Skills,
            count(element, info.skills.len(), "skills", used(&info.skills))
                .on_click(cx.listener(list(Chip::Skills, info.skills.clone(), root.clone())))
                .into_any_element(),
        ));
    }
    if !info.memory.is_empty() && Chip::Instructions.shown(cx) {
        let element = clickable(chip("status-instructions", INSTRUCTIONS, theme), Chip::Instructions, theme, cx);
        chips.push((
            Chip::Instructions,
            count(element, info.memory.len(), "md", 0)
                .on_click(cx.listener(list(Chip::Instructions, info.memory.clone(), root.clone())))
                .into_any_element(),
        ));
    }
    let menu_for = |info: Rc<AgentInfo>| {
        move |view: &mut WorkspaceView, event: &gpui::ClickEvent, _: &mut Window, cx: &mut Context<WorkspaceView>| {
            view.open_menu(event.position(), agent_menu(&info), cx)
        }
    };
    if let Some(model) = info.model.clone().filter(|_| Chip::Model.shown(cx)) {
        chips.push((
            Chip::Model,
            clickable(chip("status-model", MODEL, theme), Chip::Model, theme, cx)
                .child(model)
                .on_click(cx.listener(menu_for(info.clone())))
                .into_any_element(),
        ));
    }
    if (info.tokens.is_some() || info.compacting) && Chip::Context.shown(cx) {
        let mut element = clickable(chip("status-context", icons::GAUGE, theme), Chip::Context, theme, cx);
        if info.compacting {
            let spinner = icons::SPINNER[crate::store::store(cx).read(cx).tick_fast % icons::SPINNER.len()];
            element = element.child(div().text_color(theme.accent).child(format!("{spinner} compacting")));
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
            element = element.child(div().text_color(theme.muted).child(format!("compacted {}x", info.compactions)));
        }
        chips.push((Chip::Context, element.on_click(cx.listener(menu_for(info.clone()))).into_any_element()));
    }
    if !info.limits.is_empty() && Chip::Usage.shown(cx) {
        let mut element = clickable(chip("status-usage", LIMITS, theme), Chip::Usage, theme, cx);
        for limit in &info.limits {
            let percent = limit.current();
            element = element
                .child(div().text_color(theme.muted).child(limit.label()))
                .child(div().text_color(level(percent, theme)).child(format!("{percent}%")));
        }
        chips.push((Chip::Usage, element.on_click(cx.listener(menu_for(info.clone()))).into_any_element()));
    }
    if !info.mcp.is_empty() && Chip::Mcp.shown(cx) {
        let element = clickable(chip("status-mcp", MCP, theme), Chip::Mcp, theme, cx);
        chips.push((
            Chip::Mcp,
            count(element, info.mcp.len(), "mcp", used(&info.mcp))
                .on_click(cx.listener(list(Chip::Mcp, info.mcp.clone(), root.clone())))
                .into_any_element(),
        ));
    }
    if let Some(started) = info.started.filter(|_| Chip::Session.shown(cx)) {
        chips.push((
            Chip::Session,
            clickable(chip("status-session", CLOCK, theme), Chip::Session, theme, cx)
                .child(running(started))
                .on_click(cx.listener(menu_for(info.clone())))
                .into_any_element(),
        ));
    }
    chips
}

fn agent_menu(info: &AgentInfo) -> Vec<menu::Item> {
    let name = crate::agents::AgentKind::from_executable(&info.agent)
        .map(|kind| kind.display_name().to_string())
        .unwrap_or_else(|| info.agent.clone());
    let mut items = vec![menu::Item::Header(match &info.model {
        Some(model) => format!("{name} · {model}").into(),
        None => name.into(),
    })];
    if let Some(used) = info.tokens {
        let text = match info.window.filter(|window| *window > 0) {
            Some(window) => format!("context {} of {} ({}%)", tokens(used), tokens(window), used * 100 / window),
            None => format!("context {}", tokens(used)),
        };
        items.push(menu::Item::Header(text.into()));
    }
    if info.compacting {
        items.push(menu::Item::Header("compacting now".into()));
    }
    if info.compactions > 0 {
        let times = if info.compactions == 1 { "once".to_string() } else { format!("{} times", info.compactions) };
        items.push(menu::Item::Header(format!("compacted {times}").into()));
    }
    for limit in &info.limits {
        let mut text = format!("{} limit {}% used", limit.label(), limit.current());
        if let Some(resets) = limit.resets.and_then(|resets| resets.duration_since(SystemTime::now()).ok()) {
            text.push_str(&format!(", resets in {}", duration(resets.as_secs())));
        }
        items.push(menu::Item::Header(text.into()));
    }
    if let Some(started) = info.started {
        items.push(menu::Item::Header(format!("running for {}", running(started)).into()));
    }
    if let Some(path) = info.transcript.clone() {
        let (open, reveal) = (path.clone(), path.clone());
        items.push(menu::Item::Separator);
        items.push(menu::action("Open Transcript in Default App", move |_, _, cx| cx.open_with_system(&open)));
        items.push(menu::action(crate::actions::reveal_label(), move |_, _, cx| cx.reveal_path(&reveal)));
        items.push(menu::copy("Copy Transcript Path", path.to_string_lossy()));
    }
    if let Some(id) = info.conversation.clone() {
        items.push(menu::copy("Copy Conversation ID", id));
    }
    items
}

fn shown_path(path: &Path, root: Option<&Path>) -> String {
    match root.and_then(|root| path.strip_prefix(root).ok()) {
        Some(relative) if !relative.as_os_str().is_empty() => relative.to_string_lossy().into_owned(),
        Some(_) => paths::display(path),
        None => path.to_string_lossy().into_owned(),
    }
}

fn entries_palette(
    view: &mut WorkspaceView,
    chip: Chip,
    entries: &[Entry],
    root: Option<&Path>,
    position: Point<Pixels>,
    window: &mut Window,
    cx: &mut Context<WorkspaceView>,
) {
    let theme = ui::theme(cx);
    let (glyph, sections, placeholder) = match chip {
        Chip::Skills => (SKILLS, ("used", "available"), "find a skill"),
        Chip::Mcp => (MCP, ("called", "connected"), "find a server"),
        _ => (INSTRUCTIONS, ("", ""), "find a file"),
    };
    let items = entries
        .iter()
        .map(|entry| {
            let open = entry.path.clone().filter(|_| root.is_some());
            let title = match (&entry.path, chip) {
                (Some(path), Chip::Instructions) => shown_path(path, root),
                _ => entry.name.clone(),
            };
            let subtitle = match (chip, &entry.path) {
                (Chip::Instructions, _) => String::new(),
                (Chip::Skills, _) if !entry.description.is_empty() => entry.description.clone(),
                (_, Some(path)) => shown_path(path, root),
                _ => String::new(),
            };
            let target = open.clone();
            let mut item = Item::new(title, move |view, window, cx| {
                if let Some(path) = &target {
                    view.open_path(path, None, window, cx);
                }
            })
            .subtitle(subtitle)
            .detail(entry.detail.clone())
            .glyph(glyph, entry.used.then_some(theme.accent))
            .section(if entry.used { sections.0 } else { sections.1 });
            item.path = chip == Chip::Instructions;
            if let Some(path) = open.clone() {
                item.alternate = Some(Rc::new(move |_, _, cx| cx.open_with_system(&path)));
            }
            item
        })
        .collect();
    let anchor = Point::new(position.x - px(12.0), position.y - px(16.0));
    let palette = cx.new(|cx| Palette::new(chip.title(), placeholder, items, 0, cx).anchored(anchor));
    view.show_palette(palette, window, cx);
}

fn session_theme(session: &Entity<Session>, cx: &App) -> std::rc::Rc<Theme> {
    session.read(cx).view.theme.clone().unwrap_or_else(|| ui::theme(cx))
}

pub fn session_row(view: &WorkspaceView, session: &Entity<Session>, cx: &mut Context<WorkspaceView>) -> AnyElement {
    let theme = session_theme(session, cx);
    let chips = arrange(session_chips(view, session, &theme, cx), cx);
    row(&theme, cx).children(chips).into_any_element()
}

pub fn file_row(editor: &Entity<Editor>, cx: &mut Context<WorkspaceView>) -> Option<AnyElement> {
    let theme = ui::theme(cx);
    let chips = arrange(file_chips(editor, &theme, cx), cx);
    (!chips.is_empty()).then(|| row(&theme, cx).children(chips).into_any_element())
}

pub fn render(view: &WorkspaceView, cx: &mut Context<WorkspaceView>) -> Option<AnyElement> {
    let error = cx
        .try_global::<SettingsError>()
        .and_then(|error| error.0.clone())
        .or_else(|| cx.try_global::<crate::commands::Keys>().and_then(|keys| keys.error.clone()))
        .or_else(|| cx.try_global::<crate::theme::Themes>().and_then(|themes| themes.error.clone()));
    let focused = !every_pane(cx) && view.settings.is_none();
    let file_shown = focused && view.file_shown(cx);
    let session = if focused && !file_shown { view.selected_session(cx) } else { None };
    let theme = session.as_ref().map(|session| session_theme(session, cx)).unwrap_or_else(|| ui::theme(cx));
    let chips = match (&session, file_shown) {
        (Some(session), _) => session_chips(view, session, &theme, cx),
        (None, true) => {
            crate::file_view::active_editor(view, cx).map(|editor| file_chips(&editor, &theme, cx)).unwrap_or_default()
        }
        _ => Vec::new(),
    };
    let mut chips = arrange(chips, cx);
    if let Some(error) = error {
        chips.push(div().min_w_0().truncate().text_color(theme.red).child(error).into_any_element());
    }
    (session.is_some() || !chips.is_empty()).then(|| row(&theme, cx).children(chips).into_any_element())
}

use gpui::prelude::FluentBuilder;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chip_ids_are_unique() {
        let ids: Vec<_> = Chip::ALL.iter().map(|chip| chip.id()).collect();
        let mut unique = ids.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(ids.len(), unique.len());
        assert!(ids.iter().all(|id| Chip::from_id(id).is_some()));
    }
}
