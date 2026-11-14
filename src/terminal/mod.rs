pub mod boxes;
pub mod find;
pub mod keys;
pub mod links;
pub mod snapshot;
pub mod view;

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use alacritty_terminal::event::{Event as TermEvent, EventListener, WindowSize};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::{CursorShape, CursorStyle, Processor, Rgb};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};

pub const LIFTS_PROMPT: bool = cfg!(windows);

pub enum Event {
    Wakeup,
    Title(Option<String>),
    Bell,
    Notify(String),
    Clipboard(String),
    Exit(Option<u32>),
}

pub struct Launch {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub clear_env: bool,
    pub remove_env: Vec<String>,
    pub cwd: PathBuf,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct GridSize {
    pub cols: usize,
    pub rows: usize,
    pub cell_width: f32,
    pub cell_height: f32,
}

impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.rows
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }
}

impl GridSize {
    fn pty(&self) -> PtySize {
        PtySize {
            rows: self.rows as u16,
            cols: self.cols as u16,
            pixel_width: (self.cols as f32 * self.cell_width) as u16,
            pixel_height: (self.rows as f32 * self.cell_height) as u16,
        }
    }

    fn window(&self) -> WindowSize {
        WindowSize {
            num_lines: self.rows as u16,
            num_cols: self.cols as u16,
            cell_width: self.cell_width as u16,
            cell_height: self.cell_height as u16,
        }
    }
}

type Writer = Arc<Mutex<Box<dyn Write + Send>>>;

#[derive(Clone)]
pub struct Listener {
    events: UnboundedSender<Event>,
    writer: Writer,
    colors: Arc<Mutex<Vec<Rgb>>>,
    size: Arc<Mutex<GridSize>>,
    pending: Arc<AtomicBool>,
}

impl Listener {
    fn write(&self, text: &str) {
        self.write_bytes(text.as_bytes());
    }

    fn write_bytes(&self, bytes: &[u8]) {
        if let Ok(mut writer) = self.writer.lock() {
            let _ = writer.write_all(bytes);
            let _ = writer.flush();
        }
    }

    fn wakeup(&self) {
        if !self.pending.swap(true, Ordering::AcqRel) {
            let _ = self.events.unbounded_send(Event::Wakeup);
        }
    }
}

impl EventListener for Listener {
    fn send_event(&self, event: TermEvent) {
        match event {
            TermEvent::Title(title) => {
                let _ = self.events.unbounded_send(Event::Title(Some(title)));
            }
            TermEvent::ResetTitle => {
                let _ = self.events.unbounded_send(Event::Title(None));
            }
            TermEvent::Bell => {
                let _ = self.events.unbounded_send(Event::Bell);
            }
            TermEvent::ClipboardStore(_, text) => {
                let _ = self.events.unbounded_send(Event::Clipboard(text));
            }
            TermEvent::PtyWrite(text) => self.write(&text),
            TermEvent::ColorRequest(index, format) => {
                let color = self.colors.lock().ok().and_then(|colors| colors.get(index).copied());
                if let Some(color) = color {
                    self.write(&format(color));
                }
            }
            TermEvent::TextAreaSizeRequest(format) => {
                let size = self.size.lock().map(|size| size.window()).ok();
                if let Some(size) = size {
                    self.write(&format(size));
                }
            }
            _ => {}
        }
    }
}

pub struct Activity {
    pub last_output: AtomicU64,
    pub busy_since: AtomicU64,
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

#[derive(Clone)]
pub enum Feed {
    Start { cols: usize, rows: usize },
    Bytes { local: bool, data: Vec<u8> },
    Size { cols: usize, rows: usize },
    Flush,
}

pub type Tap = Box<dyn FnMut(&Feed) -> bool + Send>;

pub enum Request {
    Input(Vec<u8>),
    Resize { cols: usize, rows: usize },
    Clear,
}

enum Work {
    Output(Vec<u8>),
    Eof,
    Watch(Tap),
}

type Taps = Arc<Mutex<Vec<(Tap, bool)>>>;

fn tee(taps: &Taps, feed: Feed) {
    if let Ok(mut taps) = taps.lock()
        && !taps.is_empty()
    {
        taps.retain_mut(|(tap, _)| tap(&feed));
    }
}

fn reattach_after_alternate(term: &Term<Listener>, taps: &Taps, waiting: &mut Vec<Tap>) {
    if term.mode().contains(TermMode::ALT_SCREEN) {
        return;
    }
    let Ok(mut taps) = taps.lock() else { return };
    if taps.iter().any(|(_, partial)| *partial) {
        let (partial, full): (Vec<_>, Vec<_>) = taps.drain(..).partition(|(_, partial)| *partial);
        *taps = full;
        waiting.extend(partial.into_iter().map(|(tap, _)| tap));
    }
}

enum Backend {
    Pty {
        master: Box<dyn MasterPty + Send>,
        killer: Box<dyn ChildKiller + Send + Sync>,
        work: std::sync::mpsc::Sender<Work>,
        taps: Taps,
        remote_size: bool,
    },
    Mirror {
        requests: Box<dyn Fn(Request) + Send>,
        requested: Option<(usize, usize)>,
        typed: bool,
    },
}

pub struct Terminal {
    pub term: Arc<FairMutex<Term<Listener>>>,
    listener: Listener,
    backend: Backend,
    pub pid: Option<u32>,
    anchor: Arc<AtomicBool>,
    pub activity: Arc<Activity>,
    size: GridSize,
    local: Processor,
}

fn config(cursor: CursorStyle) -> Config {
    Config { scrolling_history: 10000, kitty_keyboard: true, default_cursor_style: cursor, ..Config::default() }
}

impl Terminal {
    pub fn spawn(
        launch: Launch,
        size: GridSize,
        cursor: CursorStyle,
    ) -> anyhow::Result<(Terminal, UnboundedReceiver<Event>)> {
        let pair = native_pty_system().openpty(size.pty())?;
        let mut command = CommandBuilder::new(&launch.program);
        command.args(&launch.args);
        if launch.clear_env {
            command.env_clear();
        }
        for key in &launch.remove_env {
            command.env_remove(key);
        }
        for (key, value) in &launch.env {
            command.env(key, value);
        }
        command.cwd(&launch.cwd);
        let child = pair.slave.spawn_command(command)?;
        drop(pair.slave);
        let pid = child.process_id();
        let killer = child.clone_killer();
        let reader = pair.master.try_clone_reader()?;
        let writer: Writer = Arc::new(Mutex::new(pair.master.take_writer()?));
        let (sender, receiver) = unbounded();
        let listener = Listener {
            events: sender.clone(),
            writer,
            colors: Arc::new(Mutex::new(Vec::new())),
            size: Arc::new(Mutex::new(size)),
            pending: Arc::new(AtomicBool::new(false)),
        };
        let term = Arc::new(FairMutex::new(Term::new(config(cursor), &size, listener.clone())));
        let anchor = Arc::new(AtomicBool::new(false));
        let activity = Arc::new(Activity { last_output: AtomicU64::new(0), busy_since: AtomicU64::new(0) });
        let taps: Taps = Arc::new(Mutex::new(Vec::new()));
        let work = spawn_reader(reader, term.clone(), listener.clone(), anchor.clone(), activity.clone(), taps.clone());
        spawn_waiter(child, sender);
        Ok((
            Terminal {
                term,
                listener,
                backend: Backend::Pty { master: pair.master, killer, work, taps, remote_size: false },
                pid,
                anchor,
                activity,
                size,
                local: Processor::new(),
            },
            receiver,
        ))
    }

    pub fn mirror(
        size: GridSize,
        cursor: CursorStyle,
        requests: Box<dyn Fn(Request) + Send>,
    ) -> (Terminal, UnboundedReceiver<Event>, std::sync::mpsc::Sender<Feed>) {
        let (sender, receiver) = unbounded();
        let listener = Listener {
            events: sender,
            writer: Arc::new(Mutex::new(Box::new(std::io::sink()))),
            colors: Arc::new(Mutex::new(Vec::new())),
            size: Arc::new(Mutex::new(size)),
            pending: Arc::new(AtomicBool::new(false)),
        };
        let term = Arc::new(FairMutex::new(Term::new(config(cursor), &size, listener.clone())));
        let activity = Arc::new(Activity { last_output: AtomicU64::new(0), busy_since: AtomicU64::new(0) });
        let feeds = spawn_mirror(term.clone(), listener.clone(), activity.clone());
        let terminal = Terminal {
            term,
            listener,
            backend: Backend::Mirror { requests, requested: None, typed: false },
            pid: None,
            anchor: Arc::new(AtomicBool::new(false)),
            activity,
            size,
            local: Processor::new(),
        };
        (terminal, receiver, feeds)
    }

    pub fn size(&self) -> GridSize {
        self.listener.size.lock().map(|size| *size).unwrap_or(self.size)
    }

    pub fn anchor_prompt(&self, on: bool) {
        self.anchor.store(on, Ordering::Release);
    }

    pub fn prompt_lift(&self) -> usize {
        if !LIFTS_PROMPT || !self.anchor.load(Ordering::Acquire) {
            return 0;
        }
        let term = self.term.lock();
        if term.mode().contains(TermMode::ALT_SCREEN) || term.grid().display_offset() != 0 {
            return 0;
        }
        blank_rows_below(&term)
    }

    pub fn write(&mut self, bytes: &[u8]) {
        if let Backend::Mirror { requests, requested, typed } = &mut self.backend {
            *typed = true;
            let current = self.listener.size.lock().map(|size| (size.cols, size.rows)).ok();
            if let Some((cols, rows)) = *requested
                && current != Some((cols, rows))
            {
                requests(Request::Resize { cols, rows });
            }
            requests(Request::Input(bytes.to_vec()));
            return;
        }
        self.listener.write_bytes(bytes);
    }

    pub fn write_remote(&self, bytes: &[u8]) {
        self.listener.write_bytes(bytes);
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        let mut term = self.term.lock();
        self.local.advance(&mut *term, bytes);
        if let Backend::Pty { taps, .. } = &self.backend {
            tee(taps, Feed::Bytes { local: true, data: bytes.to_vec() });
        }
    }

    pub fn watch(&self, tap: Tap) {
        if let Backend::Pty { work, .. } = &self.backend {
            let _ = work.send(Work::Watch(tap));
        }
    }

    pub fn set_colors(&self, colors: Vec<Rgb>) {
        if let Ok(mut current) = self.listener.colors.lock() {
            *current = colors;
        }
    }

    pub fn set_cursor(&self, cursor: CursorStyle) {
        self.term.lock().set_options(config(cursor));
    }

    pub fn take_wakeup(&self) {
        self.listener.pending.store(false, Ordering::Release);
    }

    pub fn resize(&mut self, size: GridSize) {
        match &mut self.backend {
            Backend::Mirror { requests, requested, typed } => {
                self.size = size;
                if *requested != Some((size.cols, size.rows)) && size.cols >= 2 && size.rows >= 1 {
                    let current = self.listener.size.lock().map(|size| (size.cols, size.rows)).ok();
                    let owned = *typed && current == *requested;
                    *requested = Some((size.cols, size.rows));
                    if owned {
                        requests(Request::Resize { cols: size.cols, rows: size.rows });
                    }
                }
            }
            Backend::Pty { remote_size: true, .. } => {}
            Backend::Pty { .. } => self.apply_size(size),
        }
    }

    pub fn resize_remote(&mut self, cols: usize, rows: usize) {
        let size = GridSize { cols: cols.clamp(2, 500), rows: rows.clamp(1, 200), ..self.size };
        if let Backend::Pty { remote_size, .. } = &mut self.backend {
            *remote_size = true;
        }
        self.apply_size(size);
    }

    pub fn claim(&mut self) {
        if let Backend::Pty { remote_size, .. } = &mut self.backend {
            *remote_size = false;
        }
    }

    fn apply_size(&mut self, size: GridSize) {
        let Backend::Pty { master, taps, .. } = &self.backend else { return };
        let current = self.size();
        if (size.cols, size.rows) == (current.cols, current.rows) && size == self.size || size.cols < 2 || size.rows < 1
        {
            return;
        }
        let grew = size.rows.saturating_sub(current.rows);
        self.size = size;
        if let Ok(mut shared) = self.listener.size.lock() {
            *shared = size;
        }
        let _ = master.resize(size.pty());
        let mut term = self.term.lock();
        term.resize(size);
        tee(taps, Feed::Size { cols: size.cols, rows: size.rows });
        let anchored = !LIFTS_PROMPT && self.anchor.load(Ordering::Acquire);
        if grew > 0 && anchored && !term.mode().contains(TermMode::ALT_SCREEN) {
            let cursor = term.grid().cursor.point;
            let below = size.rows as i32 - 1 - cursor.line.0;
            let blank = (cursor.line.0 + 1..size.rows as i32).all(|line| {
                let row = &term.grid()[Line(line)];
                (0..size.cols).all(|col| row[Column(col)].c == ' ')
            });
            if below > 0 && blank {
                let shift = format!("\x1b[{below}T\x1b[{below}B");
                self.local.advance(&mut *term, shift.as_bytes());
                tee(taps, Feed::Bytes { local: true, data: shift.into_bytes() });
            }
        }
    }

    pub fn kill(&mut self) {
        if let Backend::Pty { killer, .. } = &mut self.backend {
            let _ = killer.kill();
        }
    }

    pub fn foreground_pid(&self) -> Option<u32> {
        let Backend::Pty { master, .. } = &self.backend else { return None };
        #[cfg(unix)]
        {
            master.process_group_leader().filter(|pid| *pid > 0).map(|pid| pid as u32).or(self.pid)
        }
        #[cfg(not(unix))]
        {
            let _ = master;
            self.pid
        }
    }

    pub fn mode(&self) -> TermMode {
        *self.term.lock().mode()
    }

    pub fn scroll(&self, scroll: Scroll) {
        self.term.lock().scroll_display(scroll);
    }

    pub fn clear(&mut self) {
        if let Backend::Mirror { requests, .. } = &self.backend {
            requests(Request::Clear);
            return;
        }
        let mut term = self.term.lock();
        term.grid_mut().clear_history();
        let mut sent = b"\x1b[3J".to_vec();
        if !term.mode().contains(TermMode::ALT_SCREEN) {
            self.local.advance(&mut *term, b"\x1b[H\x1b[2J");
            sent.extend_from_slice(b"\x1b[H\x1b[2J");
        }
        if let Backend::Pty { taps, .. } = &self.backend {
            tee(taps, Feed::Bytes { local: true, data: sent });
        }
        drop(term);
        self.listener.write_bytes(b"\x0c");
    }

    pub fn text(&self, last_lines: usize) -> String {
        let term = self.term.lock();
        if term.mode().contains(TermMode::ALT_SCREEN) {
            return String::new();
        }
        let grid = term.grid();
        let top = -(grid.history_size() as i32);
        let bottom = grid.cursor.point.line.0 - 1;
        let mut lines: Vec<String> = Vec::new();
        for line in top..=bottom {
            let row = &grid[Line(line)];
            let text: String = (0..grid.columns()).map(|col| row[Column(col)].c).collect();
            lines.push(text.trim_end().to_string());
        }
        clean_output(lines, last_lines)
    }

    pub fn screen_lines(&self) -> Vec<String> {
        let term = self.term.lock();
        (0..term.screen_lines() as i32)
            .map(|line| {
                let row = &term.grid()[Line(line)];
                (0..term.columns()).map(|column| row[Column(column)].c).collect::<String>().trim_end().to_string()
            })
            .collect()
    }

    pub fn selection_text(&self) -> Option<String> {
        self.term.lock().selection_to_string().filter(|text| !text.is_empty())
    }

    pub fn cursor_line(&self) -> Point {
        self.term.lock().grid().cursor.point
    }
}

pub const RESTORED_MARK: &str = "── restored ──";

fn clean_output(mut lines: Vec<String>, last_lines: usize) -> String {
    lines.retain(|line| line != RESTORED_MARK);
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    let first = lines.iter().position(|line| !line.is_empty()).unwrap_or(lines.len());
    let start = first.max(lines.len().saturating_sub(last_lines));
    lines[start..].join("\n")
}

pub fn cursor_style(shape: crate::settings::CursorShape, blinking: bool) -> CursorStyle {
    let shape = match shape {
        crate::settings::CursorShape::Block => CursorShape::Block,
        crate::settings::CursorShape::Bar => CursorShape::Beam,
        crate::settings::CursorShape::Underline => CursorShape::Underline,
    };
    CursorStyle { shape, blinking }
}

fn spawn_reader(
    mut reader: Box<dyn Read + Send>,
    term: Arc<FairMutex<Term<Listener>>>,
    listener: Listener,
    anchor: Arc<AtomicBool>,
    activity: Arc<Activity>,
    taps: Taps,
) -> std::sync::mpsc::Sender<Work> {
    let (work, received) = channel::<Work>();
    let output = work.clone();
    std::thread::spawn(move || {
        let mut buffer = vec![0u8; 65536];
        while let Ok(count @ 1..) = reader.read(&mut buffer) {
            if output.send(Work::Output(buffer[..count].to_vec())).is_err() {
                return;
            }
        }
        let _ = output.send(Work::Eof);
    });
    std::thread::spawn(move || {
        let mut processor: Processor = Processor::new();
        let mut first = true;
        let mut scanner = Scanner::default();
        let mut waiting: Vec<Tap> = Vec::new();
        loop {
            let next = match processor.sync_timeout().sync_timeout() {
                Some(deadline) => received.recv_timeout(deadline.saturating_duration_since(Instant::now())),
                None => received.recv().map_err(|_| RecvTimeoutError::Disconnected),
            };
            match next {
                Ok(Work::Output(chunk)) => {
                    let now = now_ms();
                    let last = activity.last_output.swap(now, Ordering::AcqRel);
                    if now.saturating_sub(last) > 1500 {
                        activity.busy_since.store(now, Ordering::Release);
                    }
                    for text in scanner.notifications(&chunk) {
                        let _ = listener.events.unbounded_send(Event::Notify(text));
                    }
                    let mut term = term.lock();
                    let data = if !LIFTS_PROMPT && anchor.load(Ordering::Acquire) {
                        anchor_bottom(&chunk, term.mode().contains(TermMode::ALT_SCREEN), first)
                    } else {
                        chunk
                    };
                    processor.advance(&mut *term, &data);
                    tee(&taps, Feed::Bytes { local: false, data });
                    reattach_after_alternate(&term, &taps, &mut waiting);
                    first = false;
                    drop(term);
                    listener.wakeup();
                }
                Ok(Work::Watch(tap)) => waiting.push(tap),
                Ok(Work::Eof) | Err(RecvTimeoutError::Timeout) => {
                    let mut term = term.lock();
                    processor.stop_sync(&mut *term);
                    tee(&taps, Feed::Flush);
                    reattach_after_alternate(&term, &taps, &mut waiting);
                    drop(term);
                    listener.wakeup();
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
            if !waiting.is_empty() && processor.sync_timeout().sync_timeout().is_none() {
                let term = term.lock();
                let (cols, rows) = (term.columns(), term.screen_lines());
                let alternate = term.mode().contains(TermMode::ALT_SCREEN);
                let data = snapshot::snapshot(&term);
                for mut tap in waiting.drain(..) {
                    let start = Feed::Start { cols, rows };
                    if tap(&start)
                        && tap(&Feed::Bytes { local: false, data: data.clone() })
                        && let Ok(mut taps) = taps.lock()
                    {
                        taps.push((tap, alternate));
                    }
                }
            }
        }
    });
    work
}

fn spawn_mirror(
    term: Arc<FairMutex<Term<Listener>>>,
    listener: Listener,
    activity: Arc<Activity>,
) -> std::sync::mpsc::Sender<Feed> {
    let (feeds, received) = channel::<Feed>();
    std::thread::spawn(move || {
        let mut reader: Processor = Processor::new();
        let mut local: Processor = Processor::new();
        while let Ok(feed) = received.recv() {
            let mut term = term.lock();
            match feed {
                Feed::Start { cols, rows } => {
                    resize_mirror(&mut term, &listener, cols, rows);
                    reader = Processor::new();
                    local = Processor::new();
                    reader.advance(&mut *term, b"\x1bc");
                }
                Feed::Bytes { local: true, data } => local.advance(&mut *term, &data),
                Feed::Bytes { local: false, data } => {
                    activity.last_output.store(now_ms(), Ordering::Release);
                    reader.advance(&mut *term, &data);
                }
                Feed::Size { cols, rows } => resize_mirror(&mut term, &listener, cols, rows),
                Feed::Flush => reader.stop_sync(&mut *term),
            }
            drop(term);
            listener.wakeup();
        }
    });
    feeds
}

fn resize_mirror(term: &mut Term<Listener>, listener: &Listener, cols: usize, rows: usize) {
    let Ok(mut size) = listener.size.lock() else { return };
    *size = GridSize { cols: cols.clamp(2, 500), rows: rows.clamp(1, 200), ..*size };
    term.resize(*size);
}

fn blank_rows_below<T>(term: &Term<T>) -> usize {
    let rows = term.screen_lines() as i32;
    let cursor = term.grid().cursor.point.line.0;
    let last = (cursor + 1..rows)
        .rev()
        .find(|line| {
            let row = &term.grid()[Line(*line)];
            (0..term.columns()).any(|column| row[Column(column)].c != ' ')
        })
        .unwrap_or(cursor);
    (rows - 1 - last).max(0) as usize
}

fn spawn_waiter(mut child: Box<dyn portable_pty::Child + Send + Sync>, sender: UnboundedSender<Event>) {
    std::thread::spawn(move || {
        let code = child.wait().ok().map(|status| status.exit_code());
        let _ = sender.unbounded_send(Event::Exit(code));
    });
}

pub fn anchor_bottom(chunk: &[u8], mut alternate: bool, first: bool) -> Vec<u8> {
    const HOME: &[u8] = b"\x1b[999;1H";
    let mut out = Vec::with_capacity(chunk.len() + 16);
    if first && !alternate {
        out.extend_from_slice(HOME);
    }
    let mut index = 0;
    while index < chunk.len() {
        if chunk[index] == 0x1b && chunk.get(index + 1) == Some(&b'[') {
            let start = index;
            let mut end = index + 2;
            while end < chunk.len() && !(0x40..=0x7e).contains(&chunk[end]) {
                end += 1;
            }
            if end >= chunk.len() {
                out.extend_from_slice(&chunk[start..]);
                break;
            }
            let params = &chunk[start + 2..end];
            let last = chunk[end];
            out.extend_from_slice(&chunk[start..=end]);
            if (last == b'h' || last == b'l') && matches!(params, b"?1049" | b"?1047" | b"?47") {
                alternate = last == b'h';
            } else if last == b'J' && params == b"2" && !alternate {
                out.extend_from_slice(HOME);
            }
            index = end + 1;
        } else {
            out.push(chunk[index]);
            index += 1;
        }
    }
    out
}

#[derive(Default)]
struct Scanner {
    partial: Vec<u8>,
}

impl Scanner {
    fn notifications(&mut self, chunk: &[u8]) -> Vec<String> {
        let mut found = Vec::new();
        let mut data = std::mem::take(&mut self.partial);
        data.extend_from_slice(chunk);
        let mut index = 0;
        while let Some(offset) = find(&data[index..], b"\x1b]") {
            let start = index + offset + 2;
            let Some(end) = data[start..].iter().position(|byte| *byte == 0x07 || *byte == 0x1b).map(|end| start + end)
            else {
                if data.len() - start < 512 {
                    self.partial = data[index + offset..].to_vec();
                }
                break;
            };
            let body = String::from_utf8_lossy(&data[start..end]);
            if let Some(text) = body.strip_prefix("777;notify;") {
                found.push(text.replacen(';', ": ", 1));
            } else if let Some(text) = body.strip_prefix("9;")
                && !text.starts_with("4;")
            {
                found.push(text.to_string());
            }
            index = end + 1;
        }
        found
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(program: &str, args: &[&str]) -> (Terminal, UnboundedReceiver<Event>) {
        let launch = Launch {
            program: program.into(),
            args: args.iter().map(|arg| arg.to_string()).collect(),
            env: Vec::new(),
            clear_env: false,
            remove_env: Vec::new(),
            cwd: std::env::temp_dir(),
        };
        let size = GridSize { cols: 80, rows: 24, cell_width: 8.0, cell_height: 16.0 };
        let spawned = Terminal::spawn(launch, size, CursorStyle::default()).expect("the shell starts");
        spawned.0.anchor_prompt(true);
        spawned
    }

    fn screen(terminal: &Terminal) -> String {
        terminal.screen_lines().join("\n")
    }

    fn wait_for(terminal: &Terminal, text: &str) {
        let start = std::time::Instant::now();
        while !screen(terminal).contains(text) {
            let waited = start.elapsed() > std::time::Duration::from_secs(30);
            assert!(!waited, "no {text:?} on screen:\n{}", screen(terminal));
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    #[test]
    fn an_interactive_shell_shows_output_and_takes_input() {
        let (mut terminal, _events) = if cfg!(windows) { run("cmd.exe", &[]) } else { run("/bin/sh", &[]) };
        terminal.write(b"echo nucli-typed\r\n");
        wait_for(&terminal, "nucli-typed");
        terminal.kill();
    }

    #[test]
    fn counts_the_blank_rows_under_the_content() {
        let size = GridSize { cols: 20, rows: 10, cell_width: 8.0, cell_height: 16.0 };
        let blank_rows = |bytes: &[u8]| {
            let mut term = Term::new(Config::default(), &size, alacritty_terminal::event::VoidListener);
            Processor::<alacritty_terminal::vte::ansi::StdSyncHandler>::new().advance(&mut term, bytes);
            blank_rows_below(&term)
        };
        assert_eq!(blank_rows(b""), 9);
        assert_eq!(blank_rows(b"one\r\ntwo"), 8);
        assert_eq!(blank_rows(b"\x1b[5;1Hx\x1b[1;1H"), 5);
        assert_eq!(blank_rows(b"\x1b[10;1Hx"), 0);
    }

    #[test]
    fn a_shell_resized_while_starting_still_shows_output() {
        let (mut terminal, _events) = if cfg!(windows) { run("cmd.exe", &[]) } else { run("/bin/sh", &[]) };
        terminal.resize(GridSize { cols: 160, rows: 45, cell_width: 8.0, cell_height: 16.0 });
        terminal.write(b"echo nucli-resized\r\n");
        wait_for(&terminal, "nucli-resized");
        terminal.kill();
    }

    #[cfg(unix)]
    #[test]
    fn an_unfinished_synchronized_update_still_reaches_the_screen() {
        let (mut terminal, _events) = run("/bin/sh", &["-c", "printf '\\033[?2026hnucli-synced'; sleep 40"]);
        wait_for(&terminal, "nucli-synced");
        terminal.kill();
    }

    #[cfg(unix)]
    #[test]
    fn a_mirror_follows_the_terminal_from_any_point() {
        let (mut terminal, _events) = run("/bin/sh", &[]);
        terminal.write(b"printf 'nucli-before\\n'\r");
        wait_for(&terminal, "nucli-before");
        let size = GridSize { cols: 80, rows: 24, cell_width: 8.0, cell_height: 16.0 };
        let (mirror, _mirror_events, feeds) = Terminal::mirror(size, CursorStyle::default(), Box::new(|_| {}));
        terminal.watch(Box::new(move |feed| feeds.send(feed.clone()).is_ok()));
        terminal.write(b"printf '\\033[31mnucli-after\\033[0m\\n'\r");
        wait_for(&terminal, "nucli-after");
        terminal.resize(GridSize { cols: 50, rows: 12, cell_width: 8.0, cell_height: 16.0 });
        terminal.write(b"printf 'nucli-resized\\n'\r");
        wait_for(&terminal, "nucli-resized");
        let start = std::time::Instant::now();
        while mirror.screen_lines() != terminal.screen_lines() || mirror.cursor_line() != terminal.cursor_line() {
            assert!(start.elapsed() < std::time::Duration::from_secs(10), "mirror differs:\n{}", screen(&mirror));
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(mirror.size().cols, 50);
        assert_eq!(mirror.text(100), terminal.text(100));
        terminal.kill();
    }

    #[cfg(unix)]
    #[test]
    fn a_mirror_attached_during_a_full_screen_program_gets_the_shell_back() {
        let (mut terminal, _events) = run("/bin/sh", &[]);
        terminal
            .write(b"printf 'nucli-history\\n'; printf '\\033[?1049h\\033[2Jfull'; sleep 1; printf '\\033[?1049l'\r");
        wait_for(&terminal, "full");
        let size = GridSize { cols: 80, rows: 24, cell_width: 8.0, cell_height: 16.0 };
        let (mirror, _mirror_events, feeds) = Terminal::mirror(size, CursorStyle::default(), Box::new(|_| {}));
        terminal.watch(Box::new(move |feed| feeds.send(feed.clone()).is_ok()));
        let start = std::time::Instant::now();
        while !screen(&mirror).contains("nucli-history") || mirror.screen_lines() != terminal.screen_lines() {
            assert!(
                start.elapsed() < std::time::Duration::from_secs(10),
                "mirror lost the shell:\n{}",
                screen(&mirror)
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        terminal.kill();
    }

    #[test]
    fn anchors_clears_outside_alternate_screen() {
        assert_eq!(anchor_bottom(b"a\x1b[2Jb", false, false), b"a\x1b[2J\x1b[999;1Hb".to_vec());
        assert_eq!(anchor_bottom(b"x", false, true), b"\x1b[999;1Hx".to_vec());
        assert_eq!(anchor_bottom(b"\x1b[?1049h\x1b[2J", false, false), b"\x1b[?1049h\x1b[2J".to_vec());
        assert_eq!(anchor_bottom(b"\x1b[2J", true, false), b"\x1b[2J".to_vec());
    }

    #[test]
    fn cleans_saved_output() {
        let lines = |items: &[&str]| items.iter().map(|item| item.to_string()).collect::<Vec<_>>();
        assert_eq!(clean_output(lines(&["", "", "a", RESTORED_MARK, "b", ""]), 100), "a\nb");
        assert_eq!(clean_output(lines(&["a", "b", "c"]), 2), "b\nc");
        assert_eq!(clean_output(lines(&["", RESTORED_MARK]), 10), "");
    }

    #[test]
    fn finds_notifications() {
        let mut scanner = Scanner::default();
        assert_eq!(scanner.notifications(b"\x1b]9;done\x07"), vec!["done".to_string()]);
        assert!(scanner.notifications(b"\x1b]9;4;1;50\x07").is_empty());
        assert!(scanner.notifications(b"\x1b]777;notify;Claude;").is_empty());
        assert_eq!(scanner.notifications(b"waiting\x1b\\"), vec!["Claude: waiting".to_string()]);
    }
}
