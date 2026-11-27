use gpui::{App, Global};

#[derive(Default)]
struct Awake(Option<Hold>);

impl Global for Awake {}

pub fn is_on(cx: &App) -> bool {
    cx.try_global::<Awake>().is_some_and(|awake| awake.0.is_some())
}

pub fn toggle(cx: &mut App) {
    let awake = cx.default_global::<Awake>();
    awake.0 = match awake.0.take() {
        Some(_) => None,
        None => Hold::new(),
    };
    cx.refresh_windows();
}

#[cfg(target_os = "macos")]
const COMMAND: [&str; 3] = ["/usr/bin/caffeinate", "-i", "/bin/cat"];

#[cfg(all(unix, not(target_os = "macos")))]
const COMMAND: [&str; 5] = ["systemd-inhibit", "--what=idle:sleep", "--who=Nucli", "--why=Keep awake", "cat"];

#[cfg(unix)]
struct Hold(std::process::Child);

#[cfg(unix)]
impl Hold {
    fn new() -> Option<Self> {
        use std::process::{Command, Stdio};
        let [program, args @ ..] = COMMAND;
        Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()
            .map(Hold)
    }
}

#[cfg(unix)]
impl Drop for Hold {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[cfg(windows)]
struct Hold;

#[cfg(windows)]
impl Hold {
    fn new() -> Option<Self> {
        use windows::Win32::System::Power::{ES_CONTINUOUS, ES_SYSTEM_REQUIRED, SetThreadExecutionState};
        (unsafe { SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED) }.0 != 0).then_some(Hold)
    }
}

#[cfg(windows)]
impl Drop for Hold {
    fn drop(&mut self) {
        use windows::Win32::System::Power::{ES_CONTINUOUS, SetThreadExecutionState};
        unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
    }
}
