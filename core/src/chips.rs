use std::path::PathBuf;
use std::time::SystemTime;

use gpui::Hsla;
use serde::{Deserialize, Serialize, Serializer};

use crate::theme::Theme;

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentInfo {
    pub agent: String,
    pub model: Option<String>,
    pub tokens: Option<u64>,
    pub window: Option<u64>,
    pub skills: Vec<Entry>,
    pub memory: Vec<Entry>,
    pub mcp: Vec<Entry>,
    pub started: Option<SystemTime>,
    #[serde(skip)]
    pub transcript: Option<PathBuf>,
    pub conversation: Option<String>,
    pub compactions: usize,
    pub compacting: bool,
    pub limits: Vec<Limit>,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Entry {
    pub name: String,
    #[serde(serialize_with = "lossy")]
    pub path: Option<PathBuf>,
    pub detail: String,
    pub description: String,
    pub used: bool,
}

fn lossy<S: Serializer>(path: &Option<PathBuf>, serializer: S) -> Result<S::Ok, S::Error> {
    path.as_ref().map(|path| path.to_string_lossy()).serialize(serializer)
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Limit {
    pub minutes: u64,
    pub percent: u64,
    pub resets: Option<SystemTime>,
    pub scope: String,
}

impl Limit {
    pub fn label(&self) -> String {
        let window = match self.minutes {
            minutes if minutes >= 40320 => "1mo".to_string(),
            minutes if minutes >= 1440 && minutes.is_multiple_of(1440) => format!("{}d", minutes / 1440),
            minutes if minutes >= 60 && minutes.is_multiple_of(60) => format!("{}h", minutes / 60),
            minutes => format!("{minutes}m"),
        };
        if self.scope.is_empty() { window } else { format!("{window} {}", self.scope) }
    }

    pub fn current(&self) -> u64 {
        if self.resets.is_some_and(|resets| resets <= SystemTime::now()) { 0 } else { self.percent }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DiffStat {
    pub files: usize,
    pub added: usize,
    pub removed: usize,
}

pub fn tokens(count: u64) -> String {
    match count {
        0..1000 => count.to_string(),
        1000..1000000 => format!("{}k", (count + 500) / 1000),
        _ => {
            let tenths = (count + 50000) / 100000;
            if tenths.is_multiple_of(10) {
                format!("{}M", tenths / 10)
            } else {
                format!("{}.{}M", tenths / 10, tenths % 10)
            }
        }
    }
}

pub fn duration(seconds: u64) -> String {
    let (minutes, hours, days) = (seconds / 60, seconds / 3600, seconds / 86400);
    match seconds {
        0..60 => format!("{seconds}s"),
        60..3600 => format!("{minutes}m"),
        3600..86400 => format!("{hours}h {:02}m", minutes % 60),
        _ => format!("{days}d {}h", hours % 24),
    }
}

pub fn running(started: SystemTime) -> String {
    duration(SystemTime::now().duration_since(started).unwrap_or_default().as_secs())
}

pub fn level(percent: u64, theme: &Theme) -> Hsla {
    match percent {
        0..50 => theme.green,
        50..80 => theme.orange,
        _ => theme.red,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_numbers() {
        assert_eq!(tokens(999), "999");
        assert_eq!(tokens(84400), "84k");
        assert_eq!(tokens(200000), "200k");
        assert_eq!(tokens(1000000), "1M");
        assert_eq!(tokens(1048576), "1M");
        assert_eq!(tokens(1500000), "1.5M");
        assert_eq!(duration(42), "42s");
        assert_eq!(duration(600), "10m");
        assert_eq!(duration(3900), "1h 05m");
        assert_eq!(duration(90000), "1d 1h");
    }

    #[test]
    fn agent_info_crosses_the_wire() {
        let info = AgentInfo {
            agent: "claude".into(),
            model: Some("opus".into()),
            skills: vec![Entry {
                name: "review".into(),
                path: Some("/a/SKILL.md".into()),
                used: true,
                ..Default::default()
            }],
            started: Some(SystemTime::UNIX_EPOCH),
            transcript: Some("/a/t.jsonl".into()),
            limits: vec![Limit { minutes: 10080, percent: 7, resets: None, scope: "sonnet".into() }],
            ..Default::default()
        };
        let back: AgentInfo = serde_json::from_str(&serde_json::to_string(&info).unwrap()).unwrap();
        assert_eq!(back, AgentInfo { transcript: None, ..info });
        assert_eq!(back.limits[0].label(), "7d sonnet");
    }
}
