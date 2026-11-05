use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Duration;

use gpui::Context;

use crate::agents::{self, AgentKind};
use crate::claude::{self, ClaudeProcess, Subagent};
use crate::process::{self, ProcessInfo};
use crate::session::{Activity, Kind, Session};
use crate::settings;
use crate::store::Store;
use crate::terminal::now_ms;

struct Job {
    id: String,
    pid: u32,
    root: Option<u32>,
    dirs: Vec<PathBuf>,
}

pub struct Probe {
    pub process: Option<ProcessInfo>,
    pub claude: Option<ClaudeProcess>,
    pub subagents: Vec<Subagent>,
    pub branch: Option<String>,
}

fn probe(job: &Job) -> Probe {
    let process = process::inspect(job.pid, job.root);
    let claude =
        process.as_ref().filter(|info| info.name == "claude").and_then(|info| claude::process(info.pid, &job.dirs));
    let subagents = claude
        .as_ref()
        .and_then(|status| {
            let cwd = status.cwd.clone()?;
            job.dirs
                .iter()
                .map(|dir| claude::subagents_dir(dir, &cwd, &status.session_id))
                .find(|folder| folder.is_dir())
        })
        .map(|folder| claude::running_subagents(&folder))
        .unwrap_or_default();
    let branch = process.as_ref().and_then(|info| info.cwd.as_deref()).and_then(crate::git::current_branch);
    Probe { process, claude, subagents, branch }
}

impl Store {
    pub fn start_polling(&mut self, cx: &mut Context<Self>) {
        self.poll_task = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(1000)).await;
                if this.update(cx, |store, cx| store.poll(cx)).is_err() {
                    break;
                }
            }
        }));
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(110)).await;
                let alive = this.update(cx, |store, cx| {
                    let working = store.sessions.values().any(|session| session.read(cx).activity == Activity::Working);
                    if working {
                        store.tick_fast = store.tick_fast.wrapping_add(1);
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn refresh_diffstats(&mut self, cx: &mut Context<Self>) {
        let projects: Vec<PathBuf> = self
            .workspaces
            .iter()
            .filter(|workspace| self.windows.contains_key(&workspace.id))
            .flat_map(|workspace| workspace.projects.iter().map(|project| project.path.clone()))
            .collect();
        cx.spawn(async move |this, cx| {
            let stats: Vec<(PathBuf, Option<crate::git::DiffStat>)> = cx
                .background_executor()
                .spawn(async move {
                    projects
                        .into_iter()
                        .map(|path| {
                            let stat = crate::git::diff_stat(&path);
                            (path, stat)
                        })
                        .collect()
                })
                .await;
            let _ = this.update(cx, |store, cx| {
                let mut changed = false;
                for (path, stat) in stats {
                    let previous = store.diffstats.get(&path).copied();
                    match stat {
                        Some(stat) if previous != Some(stat) => {
                            store.diffstats.insert(path, stat);
                            changed = true;
                        }
                        None if previous.is_some() => {
                            store.diffstats.remove(&path);
                            changed = true;
                        }
                        _ => {}
                    }
                }
                if changed {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn poll(&mut self, cx: &mut Context<Self>) {
        self.tick += 1;
        if self.tick % 3 == 1 {
            self.refresh_diffstats(cx);
        }
        let profiles = settings::get(cx).agents.clone();
        let mut claude_dirs: Vec<PathBuf> = profiles
            .iter()
            .filter(|profile| profile.kind() == Some(AgentKind::Claude))
            .filter_map(|profile| profile.data_dir())
            .collect();
        claude_dirs.push(crate::paths::expand(AgentKind::Claude.default_config_dir()));
        claude_dirs.dedup();
        let jobs: Vec<Job> = self
            .sessions
            .values()
            .filter_map(|entity| {
                let session = entity.read(cx);
                if session.exit.is_some() {
                    return None;
                }
                let own = session
                    .profile
                    .as_ref()
                    .and_then(|name| profiles.iter().find(|profile| &profile.name == name))
                    .filter(|profile| profile.kind() == Some(AgentKind::Claude))
                    .and_then(|profile| profile.data_dir());
                let dirs = match own {
                    Some(dir) => vec![dir],
                    None => claude_dirs.clone(),
                };
                Some(Job {
                    id: session.id.clone(),
                    pid: session.terminal.foreground_pid()?,
                    root: session.terminal.pid,
                    dirs,
                })
            })
            .collect();
        if jobs.is_empty() {
            return;
        }
        let commands: HashSet<String> = agents::KNOWN
            .iter()
            .map(|name| name.to_string())
            .chain(profiles.iter().map(|profile| agents::command_name(&profile.executable)))
            .collect();
        cx.spawn(async move |this, cx| {
            let results: Vec<(String, Probe)> = cx
                .background_executor()
                .spawn(async move { jobs.iter().map(|job| (job.id.clone(), probe(job))).collect() })
                .await;
            let _ = this.update(cx, |store, cx| {
                let mut events = Vec::new();
                for (id, probe) in results {
                    let Some(entity) = store.sessions.get(&id).cloned() else { continue };
                    let event = entity.update(cx, |session, cx| session.apply(probe, &commands, cx));
                    if let Some(text) = event {
                        events.push((id, text));
                    }
                }
                for (id, text) in events {
                    store.mark_attention(&id, Some(text), cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn watched(&self, id: &str, cx: &mut Context<Self>) -> bool {
        let Some(workspace) = self.workspace_of(id) else { return false };
        let Some(window) = self.windows.get(&workspace.id).copied() else { return false };
        window.update(cx, |view, window, cx| window.is_window_active() && view.shows(id, cx)).unwrap_or(false)
    }

    pub fn mark_attention(&mut self, id: &str, text: Option<String>, cx: &mut Context<Self>) {
        if self.watched(id, cx) {
            return;
        }
        let Some(entity) = self.sessions.get(id).cloned() else { return };
        let title = entity.update(cx, |session, cx| {
            session.attention = true;
            cx.notify();
            session.display_title()
        });
        if let Some(text) = text
            && settings::get(cx).notifications
        {
            crate::notify::post(&title, &text, id);
        }
        self.update_badge(cx);
        cx.notify();
    }

    pub fn update_badge(&self, cx: &mut Context<Self>) {
        let count = self.sessions.values().filter(|session| session.read(cx).attention).count();
        crate::notify::badge(count);
    }
}

impl Session {
    pub fn apply(&mut self, probe: Probe, agents: &HashSet<String>, cx: &mut Context<Self>) -> Option<String> {
        let name = probe.process.as_ref().map(|info| info.name.clone());
        let agent_running = name.as_ref().is_some_and(|name| agents.contains(name));
        self.terminal.anchor.store(self.kind != Kind::Agent && !agent_running, Ordering::Release);
        let changed = self.foreground != name || self.subagents != probe.subagents;
        self.foreground = name.clone();
        self.running_agent = agent_running.then(|| name.clone().unwrap_or_default());
        self.cwd = probe.process.as_ref().and_then(|info| info.cwd.clone()).or(self.cwd.take());
        self.branch = probe.branch;
        self.subagents = probe.subagents;
        let previous = self.activity;
        let now = now_ms();
        if let Some(status) = &probe.claude {
            self.activity = match status.status.as_str() {
                "busy" => Activity::Working,
                "waiting" => Activity::Waiting,
                _ => Activity::Idle,
            };
            if self.kind == Kind::Agent && self.conversation.as_deref() != Some(&status.session_id) {
                self.conversation = Some(status.session_id.clone());
            }
        } else if agent_running {
            let last = self.terminal.activity.last_output.load(Ordering::Acquire);
            let since = self.terminal.activity.busy_since.load(Ordering::Acquire);
            let busy = now.saturating_sub(last) < 1500;
            if busy && now.saturating_sub(since) >= 2000 {
                self.activity = Activity::Working;
            } else if !busy && previous == Activity::Working {
                let worked = last.saturating_sub(since) >= 5000;
                if now.saturating_sub(last) >= 2000 {
                    self.activity = Activity::Idle;
                    if !worked {
                        return None;
                    }
                }
            }
        } else {
            self.activity = Activity::Idle;
        }
        if changed || previous != self.activity {
            cx.emit(crate::session::SessionEvent::Changed);
            cx.notify();
        }
        let agent = self.running_agent.clone().unwrap_or_else(|| "agent".into());
        match (previous, self.activity) {
            (Activity::Working, Activity::Idle) => Some(format!("{agent} finished")),
            (previous, Activity::Waiting) if previous != Activity::Waiting => {
                Some(format!("{agent} is waiting for input"))
            }
            _ => None,
        }
    }
}
