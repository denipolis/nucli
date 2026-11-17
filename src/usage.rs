use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct Limit {
    pub minutes: u64,
    pub percent: u64,
    pub resets: Option<SystemTime>,
    pub scope: &'static str,
}

impl Limit {
    pub fn label(&self) -> String {
        let window = match self.minutes {
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

const INTERVAL: Duration = Duration::from_secs(300);
const CLAUDE_USAGE: &str = "https://api.anthropic.com/api/oauth/usage";

type Cache = HashMap<PathBuf, (Option<Instant>, Vec<Limit>)>;

static CLAUDE: Mutex<Option<Cache>> = Mutex::new(None);

pub fn claude(config: &Path) -> Vec<Limit> {
    let mut guard = CLAUDE.lock().unwrap_or_else(PoisonError::into_inner);
    let entry = guard.get_or_insert_with(HashMap::new).entry(config.to_path_buf()).or_default();
    if entry.0.is_none_or(|checked| checked.elapsed() >= INTERVAL) {
        entry.0 = Some(Instant::now());
        let config = config.to_path_buf();
        std::thread::spawn(move || {
            let Some(limits) = claude_token(&config).and_then(|token| fetch_claude(&token)) else { return };
            let mut guard = CLAUDE.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(entry) = guard.get_or_insert_with(HashMap::new).get_mut(&config) {
                entry.1 = limits;
            }
        });
    }
    entry.1.clone()
}

fn claude_token(config: &Path) -> Option<String> {
    let text = keychain(config).or_else(|| std::fs::read_to_string(config.join(".credentials.json")).ok())?;
    let json: Value = serde_json::from_str(&text).ok()?;
    let oauth = &json["claudeAiOauth"];
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_millis() as u64;
    if oauth["expiresAt"].as_u64().is_some_and(|expires| expires <= now) {
        return None;
    }
    oauth["accessToken"].as_str().map(String::from)
}

#[cfg(target_os = "macos")]
fn keychain(config: &Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    let hash = Sha256::digest(config.to_string_lossy().as_bytes());
    let suffix: String = hash.iter().take(4).map(|byte| format!("{byte:02x}")).collect();
    let hashed = format!("Claude Code-credentials-{suffix}");
    let services = if config == crate::paths::expand("~/.claude") {
        vec!["Claude Code-credentials".to_string(), hashed]
    } else {
        vec![hashed]
    };
    services.iter().find_map(|service| {
        let output = Command::new("security")
            .args(["find-generic-password", "-s", service, "-w"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
    })
}

#[cfg(not(target_os = "macos"))]
fn keychain(_: &Path) -> Option<String> {
    None
}

fn fetch_claude(token: &str) -> Option<Vec<Limit>> {
    let mut command = Command::new("curl");
    command
        .args(["-sSf", "-m", "10", "-H", "@-", "-H", "anthropic-beta: oauth-2025-04-20", CLAUDE_USAGE])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().ok()?;
    let written = child
        .stdin
        .take()
        .is_some_and(|mut stdin| stdin.write_all(format!("Authorization: Bearer {token}\n").as_bytes()).is_ok());
    let output = child.wait_with_output().ok()?;
    if !written || !output.status.success() {
        return None;
    }
    Some(claude_limits(&serde_json::from_slice(&output.stdout).ok()?))
}

fn claude_limits(json: &Value) -> Vec<Limit> {
    [
        ("five_hour", 300, ""),
        ("seven_day", 10080, ""),
        ("seven_day_opus", 10080, "opus"),
        ("seven_day_sonnet", 10080, "sonnet"),
    ]
    .into_iter()
    .filter_map(|(key, minutes, scope)| {
        let window = &json[key];
        Some(Limit {
            minutes,
            percent: window["utilization"].as_f64()?.round() as u64,
            resets: window["resets_at"].as_str().and_then(parse_time),
            scope,
        })
    })
    .collect()
}

pub fn codex(json: &Value, at: Option<SystemTime>) -> Vec<Limit> {
    ["primary", "secondary"]
        .into_iter()
        .filter_map(|key| {
            let window = &json[key];
            let resets = window["resets_at"]
                .as_u64()
                .map(|seconds| UNIX_EPOCH + Duration::from_secs(seconds))
                .or_else(|| Some(at? + Duration::from_secs(window["resets_in_seconds"].as_u64()?)));
            Some(Limit {
                minutes: window["window_minutes"].as_u64()?,
                percent: window["used_percent"].as_f64()?.round() as u64,
                resets,
                scope: "",
            })
        })
        .collect()
}

pub fn parse_time(text: &str) -> Option<SystemTime> {
    let number = |start: usize, end: usize| text.get(start..end)?.parse::<i64>().ok();
    let (year, month, day) = (number(0, 4)?, number(5, 7)?, number(8, 10)?);
    let (hour, minute, second) = (number(11, 13)?, number(14, 16)?, number(17, 19)?);
    let zone = text.get(19..)?.trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    let offset = match zone.chars().next() {
        Some(sign @ ('+' | '-')) => {
            let minutes = zone.get(1..3)?.parse::<i64>().ok()? * 60 + zone.get(4..6)?.parse::<i64>().ok()?;
            if sign == '-' { -minutes * 60 } else { minutes * 60 }
        }
        _ => 0,
    };
    let seconds = days_from_civil(year, month, day) * 86400 + hour * 3600 + minute * 60 + second - offset;
    Some(UNIX_EPOCH + Duration::from_secs(u64::try_from(seconds).ok()?))
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let (era, yoe) = (year.div_euclid(400), year.rem_euclid(400));
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: u64) -> Option<SystemTime> {
        Some(UNIX_EPOCH + Duration::from_secs(seconds))
    }

    #[test]
    fn parses_times() {
        assert_eq!(parse_time("1970-01-01T00:00:00Z"), at(0));
        assert_eq!(parse_time("2001-09-09T01:46:40.5Z"), at(1000000000));
        assert_eq!(parse_time("2001-09-09T03:46:40.106477+02:00"), at(1000000000));
        assert_eq!(parse_time("2001-09-08T20:16:40-05:30"), at(1000000000));
        assert_eq!(parse_time("2024-02-29T00:00:00+00:00"), at(1709164800));
        assert_eq!(parse_time("soon"), None);
    }

    #[test]
    fn reads_limits() {
        let claude: Value = serde_json::from_str(
            r#"{"five_hour":{"utilization":24.4,"resets_at":"2001-09-09T01:46:40+00:00"},"seven_day":{"utilization":45.0,"resets_at":null},"seven_day_opus":null,"seven_day_sonnet":{"utilization":7.0,"resets_at":null}}"#,
        )
        .unwrap();
        let limits = claude_limits(&claude);
        assert_eq!(
            limits,
            vec![
                Limit { minutes: 300, percent: 24, resets: at(1000000000), scope: "" },
                Limit { minutes: 10080, percent: 45, resets: None, scope: "" },
                Limit { minutes: 10080, percent: 7, resets: None, scope: "sonnet" },
            ]
        );
        let labels: Vec<String> = limits.iter().map(Limit::label).collect();
        assert_eq!(labels, ["5h", "7d", "7d sonnet"]);
        assert_eq!((limits[0].current(), limits[1].current()), (0, 45));
        let rollout: Value = serde_json::from_str(
            r#"{"primary":{"used_percent":12.5,"window_minutes":300,"resets_in_seconds":60},"secondary":{"used_percent":3.0,"window_minutes":10080,"resets_at":1000000000}}"#,
        )
        .unwrap();
        assert_eq!(
            codex(&rollout, at(100)),
            vec![
                Limit { minutes: 300, percent: 13, resets: at(160), scope: "" },
                Limit { minutes: 10080, percent: 3, resets: at(1000000000), scope: "" },
            ]
        );
    }
}
