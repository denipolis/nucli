use serde::{Deserialize, Serialize};

use crate::chips::{AgentInfo, DiffStat};
use crate::session::{Activity, Kind};
use crate::terminal::Feed;
use crate::theme::ItemColor;

pub const PROTOCOL: u32 = 2;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Hello {
    pub name: String,
    pub os: String,
    pub version: String,
    pub port: u16,
    pub protocol: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct RemoteTab {
    pub id: String,
    pub title: String,
    pub kind: Kind,
    pub agent: Option<String>,
    pub program: Option<String>,
    pub activity: Activity,
    pub attention: bool,
    pub running: bool,
    pub failed: bool,
    pub color: Option<ItemColor>,
    pub project: String,
    pub workspace: String,
    pub cwd: Option<String>,
    pub branch: Option<String>,
    pub notice: Option<String>,
    pub muted: bool,
    pub changes: Option<DiffStat>,
    pub info: Option<AgentInfo>,
    pub panel: bool,
}

impl RemoteTab {
    pub fn alert(&self, before: &RemoteTab) -> Option<String> {
        if self.muted {
            None
        } else if self.activity == Activity::Waiting && before.activity != Activity::Waiting {
            Some("is waiting for input".into())
        } else if self.attention && !before.attention {
            Some(self.notice.clone().unwrap_or_else(|| "finished".into()))
        } else {
            None
        }
    }
}

impl Default for RemoteTab {
    fn default() -> Self {
        RemoteTab {
            id: String::new(),
            title: String::new(),
            kind: Kind::Shell,
            agent: None,
            program: None,
            activity: Activity::Idle,
            attention: false,
            running: true,
            failed: false,
            color: None,
            project: String::new(),
            workspace: String::new(),
            cwd: None,
            branch: None,
            notice: None,
            muted: false,
            changes: None,
            info: None,
            panel: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct RemoteProject {
    pub path: String,
    pub name: String,
    pub color: Option<ItemColor>,
    pub workspace: String,
    pub muted: bool,
    pub revision: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct RemoteWorkspace {
    pub name: String,
    pub open: bool,
    pub muted: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct RemoteLauncher {
    pub name: String,
    pub agent: bool,
    pub command: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Catalog {
    pub tabs: Vec<RemoteTab>,
    pub projects: Vec<RemoteProject>,
    pub workspaces: Vec<RemoteWorkspace>,
    pub launchers: Vec<RemoteLauncher>,
    pub control: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    Hello(Hello),
    Catalog(Catalog),
    Watch {
        tab: String,
    },
    Unwatch {
        tab: String,
    },
    Resize {
        tab: String,
        cols: u16,
        rows: u16,
    },
    Hide {
        tab: String,
    },
    Clear {
        tab: String,
    },
    Open {
        project: String,
        launcher: String,
        #[serde(default)]
        workspace: String,
    },
    Opened {
        tab: String,
    },
    Stop {
        tab: String,
        reason: Stop,
    },
    Seen {
        tab: String,
    },
    Request {
        id: u64,
        request: Request,
    },
    Sending {
        id: u64,
        size: u64,
    },
    Sent {
        id: u64,
    },
    Response {
        id: u64,
        response: Response,
    },
    Unpair,
    Approved,
    Ping,
    #[serde(other)]
    Unknown,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Request {
    History,
    Resume { conversation: String },
    Folders { path: String },
    AddProject { path: String, workspace: Option<String> },
    OpenWorkspace { workspace: String },
    RenameWorkspace { workspace: String, name: String },
    MuteWorkspace { workspace: String, muted: bool },
    Files { project: String, folder: String },
    File { project: String, path: String },
    Changes { project: String },
    Diff { project: String, path: String, commit: Option<String> },
    Log { project: String },
    Commit { project: String, hash: String },
    Branches { project: String },
    Tree { project: String },
    Download { project: String, path: String },
    Upload { project: String, folder: String, name: String, size: u64 },
    Attach { tab: String, name: String, size: u64 },
    Query { project: String, query: serde_json::Value },
    Act { project: String, action: Action },
    Tab { tab: String, action: TabAction },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "do", rename_all = "snake_case")]
pub enum Action {
    Create { path: String, folder: bool },
    Rename { from: String, to: String },
    Duplicate { path: String },
    Delete { path: String },
    Paste { source: String, from: String, folder: String, cut: bool },
    Open { path: String },
    Reveal { path: String },
    Stage { paths: Vec<String> },
    Unstage { paths: Vec<String> },
    Discard { paths: Vec<String> },
    Commit { message: String },
    Switch { branch: String },
    Track { remote: String },
    Detach { hash: String },
    Branch { name: String, at: Option<String> },
    Tag { name: String, at: String },
    CherryPick { hash: String },
    Revert { hash: String },
    Reset { hash: String, mode: Reset },
    DeleteBranch { name: String },
    DeleteTag { name: String },
    Restore { hash: String, path: String },
    RenameProject { name: String },
    ColorProject { color: Option<ItemColor> },
    MoveProject { workspace: String },
    MuteProject { muted: bool },
    CloseProject,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Reset {
    Soft,
    Mixed,
    Hard,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "do", rename_all = "snake_case")]
pub enum TabAction {
    Rename { title: String },
    Color { color: Option<ItemColor> },
    MoveOut,
    Mute { muted: bool },
    Close,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Response {
    History { conversations: Vec<RemoteConversation> },
    Folders { path: String, folders: Vec<String> },
    Files { entries: Vec<RemoteEntry> },
    File { text: String, truncated: bool, binary: bool },
    Changes { branch: String, changes: Vec<RemoteChange> },
    Diff { lines: Vec<RemoteLine> },
    Log { commits: Vec<RemoteCommit> },
    Commit { date: String, message: String, stats: String, changes: Vec<RemoteChange> },
    Branches { current: String, branches: Vec<String> },
    Tree { files: Vec<String> },
    Attached { text: String },
    Answer { value: serde_json::Value },
    Done,
    Failed { reason: String },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct RemoteConversation {
    pub id: String,
    pub title: String,
    pub modified: u64,
    pub agent: String,
    pub profile: String,
    pub cwd: String,
    pub project: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct RemoteEntry {
    pub name: String,
    pub folder: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct RemoteChange {
    pub path: String,
    pub status: String,
    pub staged: bool,
    pub added: usize,
    pub removed: usize,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum LineKind {
    #[default]
    Context,
    Added,
    Removed,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct RemoteLine {
    pub kind: LineKind,
    pub text: String,
    pub old: Option<usize>,
    pub new: Option<usize>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct RemoteCommit {
    pub hash: String,
    pub parents: Vec<String>,
    pub refs: Vec<String>,
    pub author: String,
    pub time: i64,
    pub subject: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Stop {
    Behind,
    NotShared,
    Closed,
}

pub enum Frame {
    Message(Message),
    Feed(String, Feed),
    Input(String, Vec<u8>),
    Chunk(u64, Vec<u8>),
}

const MESSAGE: u8 = 0;
const START: u8 = 1;
const OUTPUT: u8 = 2;
const LOCAL: u8 = 3;
const SIZE: u8 = 4;
const FLUSH: u8 = 5;
const INPUT: u8 = 6;
const CHUNK: u8 = 7;

pub fn message(message: &Message) -> Vec<u8> {
    let mut frame = vec![MESSAGE];
    serde_json::to_writer(&mut frame, message).expect("messages serialize");
    frame
}

fn tagged(kind: u8, tab: &str, extra: usize) -> Vec<u8> {
    let tab = &tab.as_bytes()[..tab.len().min(255)];
    let mut frame = Vec::with_capacity(2 + tab.len() + extra);
    frame.push(kind);
    frame.push(tab.len() as u8);
    frame.extend_from_slice(tab);
    frame
}

fn size(frame: &mut Vec<u8>, cols: usize, rows: usize) {
    frame.extend_from_slice(&(cols.min(u16::MAX as usize) as u16).to_be_bytes());
    frame.extend_from_slice(&(rows.min(u16::MAX as usize) as u16).to_be_bytes());
}

pub fn feed(tab: &str, feed: &Feed) -> Vec<u8> {
    match feed {
        Feed::Start { cols, rows } => {
            let mut frame = tagged(START, tab, 4);
            size(&mut frame, *cols, *rows);
            frame
        }
        Feed::Size { cols, rows } => {
            let mut frame = tagged(SIZE, tab, 4);
            size(&mut frame, *cols, *rows);
            frame
        }
        Feed::Bytes { local, data } => {
            let mut frame = tagged(if *local { LOCAL } else { OUTPUT }, tab, data.len());
            frame.extend_from_slice(data);
            frame
        }
        Feed::Flush => tagged(FLUSH, tab, 0),
    }
}

pub fn input(tab: &str, data: &[u8]) -> Vec<u8> {
    let mut frame = tagged(INPUT, tab, data.len());
    frame.extend_from_slice(data);
    frame
}

pub fn chunk(id: u64, data: &[u8]) -> Vec<u8> {
    let mut frame = tagged(CHUNK, &id.to_string(), data.len());
    frame.extend_from_slice(data);
    frame
}

pub fn decode(frame: &[u8]) -> Option<Frame> {
    let (&kind, rest) = frame.split_first()?;
    if kind == MESSAGE {
        return serde_json::from_slice(rest).ok().map(Frame::Message);
    }
    let (&length, rest) = rest.split_first()?;
    let tab = std::str::from_utf8(rest.get(..length as usize)?).ok()?.to_string();
    let body = &rest[length as usize..];
    let dimensions = || {
        let cols = u16::from_be_bytes([*body.first()?, *body.get(1)?]) as usize;
        let rows = u16::from_be_bytes([*body.get(2)?, *body.get(3)?]) as usize;
        Some((cols, rows))
    };
    let feed = match kind {
        START => dimensions().map(|(cols, rows)| Feed::Start { cols, rows })?,
        SIZE => dimensions().map(|(cols, rows)| Feed::Size { cols, rows })?,
        OUTPUT => Feed::Bytes { local: false, data: body.to_vec() },
        LOCAL => Feed::Bytes { local: true, data: body.to_vec() },
        FLUSH => Feed::Flush,
        INPUT => return Some(Frame::Input(tab, body.to_vec())),
        CHUNK => return Some(Frame::Chunk(tab.parse().ok()?, body.to_vec())),
        _ => return None,
    };
    Some(Frame::Feed(tab, feed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_round_trip() {
        let hello = Message::Hello(Hello {
            name: "mac".into(),
            os: "macos".into(),
            version: "2".into(),
            port: 7,
            protocol: PROTOCOL,
        });
        assert!(matches!(decode(&message(&hello)), Some(Frame::Message(decoded)) if decoded == hello));
        let catalog = Message::Catalog(Catalog { tabs: vec![RemoteTab::default()], ..Default::default() });
        assert!(matches!(decode(&message(&catalog)), Some(Frame::Message(decoded)) if decoded == catalog));
        let request = Message::Request {
            id: 7,
            request: Request::Diff { project: "/p".into(), path: "a.rs".into(), commit: None },
        };
        assert!(matches!(decode(&message(&request)), Some(Frame::Message(decoded)) if decoded == request));
        let act = Message::Request {
            id: 8,
            request: Request::Act {
                project: "/p".into(),
                action: Action::Reset { hash: "abc".into(), mode: Reset::Hard },
            },
        };
        assert!(matches!(decode(&message(&act)), Some(Frame::Message(decoded)) if decoded == act));
        let tab = Message::Request {
            id: 9,
            request: Request::Tab { tab: "t".into(), action: TabAction::Color { color: Some(ItemColor::Teal) } },
        };
        assert!(matches!(decode(&message(&tab)), Some(Frame::Message(decoded)) if decoded == tab));
        let lines = vec![RemoteLine { kind: LineKind::Added, text: "x".into(), old: None, new: Some(1) }];
        let response = Message::Response { id: 7, response: Response::Diff { lines } };
        assert!(matches!(decode(&message(&response)), Some(Frame::Message(decoded)) if decoded == response));
        let start = feed("tab", &Feed::Start { cols: 80, rows: 24 });
        assert!(matches!(decode(&start), Some(Frame::Feed(tab, Feed::Start { cols: 80, rows: 24 })) if tab == "tab"));
        let bytes = feed("t", &Feed::Bytes { local: true, data: b"hi".to_vec() });
        assert!(matches!(decode(&bytes), Some(Frame::Feed(_, Feed::Bytes { local: true, data })) if data == b"hi"));
        assert!(matches!(decode(&input("t", b"ls\r")), Some(Frame::Input(tab, data)) if tab == "t" && data == b"ls\r"));
        assert!(matches!(decode(&chunk(42, b"\x00\xff")), Some(Frame::Chunk(42, data)) if data == b"\x00\xff"));
    }

    #[test]
    fn alerts_when_a_tab_starts_waiting_or_finishes() {
        let idle = RemoteTab::default();
        let waiting = RemoteTab { activity: Activity::Waiting, ..Default::default() };
        let done = RemoteTab { attention: true, notice: Some("Claude finished".into()), ..Default::default() };
        assert_eq!(waiting.alert(&idle).as_deref(), Some("is waiting for input"));
        assert_eq!(waiting.alert(&waiting), None);
        assert_eq!(done.alert(&idle).as_deref(), Some("Claude finished"));
        assert_eq!(done.alert(&done), None);
        assert_eq!(RemoteTab { muted: true, ..waiting.clone() }.alert(&idle), None);
        let message =
            Message::Request { id: 1, request: Request::MuteWorkspace { workspace: "w".into(), muted: true } };
        assert!(matches!(decode(&self::message(&message)), Some(Frame::Message(decoded)) if decoded == message));
    }

    #[test]
    fn ignores_unknown_and_broken_frames() {
        assert!(matches!(decode(b"\x00{\"type\":\"from_the_future\"}"), Some(Frame::Message(Message::Unknown))));
        assert!(decode(b"").is_none());
        assert!(decode(b"\x01\x05ab").is_none());
        assert!(decode(b"\x01\x01a\x00").is_none());
        assert!(decode(b"\x09\x01a").is_none());
        assert!(decode(b"\x00not json").is_none());
    }
}
