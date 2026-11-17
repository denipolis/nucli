use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::transcript::Parsed;

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

const INTERVAL: Duration = Duration::from_secs(300);
const ANTHROPIC_USAGE: &str = "https://api.anthropic.com/api/oauth/usage";
const CHATGPT_USAGE: &str = "https://chatgpt.com/backend-api/wham/usage";
const CODE_ASSIST: &str = "https://cloudcode-pa.googleapis.com/v1internal";

type Cache = HashMap<String, (Option<Instant>, Vec<Limit>)>;

static ONLINE: Mutex<Option<Cache>> = Mutex::new(None);

pub fn other_api(env: &std::collections::BTreeMap<String, String>) -> bool {
    env.contains_key("ANTHROPIC_API_KEY")
        || env.contains_key("ANTHROPIC_AUTH_TOKEN")
        || env.get("ANTHROPIC_BASE_URL").is_some_and(|url| !url.contains("anthropic.com"))
}

pub fn limits(agent: &str, config: &Path, parsed: &Parsed) -> Vec<Limit> {
    match agent {
        "claude" => claude(config),
        "codex" if parsed.limits.is_empty() => codex(config),
        "gemini" => gemini(config, parsed.model.clone()),
        "pi" => pi(config, parsed.provider.clone()),
        "opencode" => opencode(),
        _ => parsed.limits.clone(),
    }
}

fn online(key: String, fetch: impl FnOnce() -> Option<Vec<Limit>> + Send + 'static) -> Vec<Limit> {
    let mut guard = ONLINE.lock().unwrap_or_else(PoisonError::into_inner);
    let entry = guard.get_or_insert_with(HashMap::new).entry(key.clone()).or_default();
    if entry.0.is_none_or(|checked| checked.elapsed() >= INTERVAL) {
        entry.0 = Some(Instant::now());
        std::thread::spawn(move || {
            let Some(limits) = fetch() else { return };
            let mut guard = ONLINE.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(entry) = guard.get_or_insert_with(HashMap::new).get_mut(&key) {
                entry.1 = limits;
            }
        });
    }
    entry.1.clone()
}

fn claude(config: &Path) -> Vec<Limit> {
    let config = config.to_path_buf();
    online(format!("claude {}", config.display()), move || anthropic(&claude_token(&config)?))
}

fn codex(config: &Path) -> Vec<Limit> {
    let path = config.join("auth.json");
    online(format!("codex {}", path.display()), move || {
        let json = read_json(&path)?;
        let tokens = &json["tokens"];
        chatgpt(tokens["access_token"].as_str()?, tokens["account_id"].as_str())
    })
}

fn gemini(config: &Path, model: Option<String>) -> Vec<Limit> {
    let path = config.join("oauth_creds.json");
    online(format!("gemini {} {model:?}", path.display()), move || {
        let json = read_json(&path)?;
        if expired(&json["expiry_date"]) {
            return None;
        }
        google(json["access_token"].as_str()?, None, model.as_deref())
    })
}

fn pi(config: &Path, provider: Option<String>) -> Vec<Limit> {
    let known = ["anthropic", "openai-codex", "github-copilot"];
    let Some(provider) = provider.filter(|provider| known.contains(&provider.as_str())) else {
        return Vec::new();
    };
    let path = config.join("auth.json");
    online(format!("pi {} {provider}", path.display()), move || {
        let json = read_json(&path)?;
        let credential = &json[&provider];
        let copilot_sign_in = provider == "github-copilot";
        if credential["type"] != "oauth" || (!copilot_sign_in && expired(&credential["expires"])) {
            return None;
        }
        let token = credential["access"].as_str()?;
        match provider.as_str() {
            "anthropic" => anthropic(token),
            "openai-codex" => chatgpt(token, chatgpt_account(token).as_deref()),
            _ => copilot(credential["refresh"].as_str()?, credential["enterpriseUrl"].as_str()),
        }
    })
}

fn opencode() -> Vec<Limit> {
    let data = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from);
    let path = data.unwrap_or_else(|| crate::paths::home().join(".local/share")).join("opencode").join("auth.json");
    online(format!("opencode {}", path.display()), move || {
        let json = read_json(&path)?;
        let signed_in = |provider: &str| {
            Some(&json[provider]).filter(|credential| credential["type"] == "oauth" && !expired(&credential["expires"]))
        };
        if let Some(credential) = signed_in("anthropic") {
            return anthropic(credential["access"].as_str()?);
        }
        let credential = signed_in("openai")?;
        let token = credential["access"].as_str()?;
        let account = credential["accountId"].as_str().map(String::from).or_else(|| chatgpt_account(token));
        chatgpt(token, account.as_deref())
    })
}

fn chatgpt_account(token: &str) -> Option<String> {
    use base64::Engine;
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(token.split('.').nth(1)?).ok()?;
    let claims: Value = serde_json::from_slice(&payload).ok()?;
    claims["https://api.openai.com/auth"]["chatgpt_account_id"].as_str().map(String::from)
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn expired(milliseconds: &Value) -> bool {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64;
    milliseconds.as_u64().is_some_and(|expires| expires <= now)
}

fn claude_token(config: &Path) -> Option<String> {
    let text = keychain(config).or_else(|| std::fs::read_to_string(config.join(".credentials.json")).ok())?;
    let json: Value = serde_json::from_str(&text).ok()?;
    let oauth = &json["claudeAiOauth"];
    if expired(&oauth["expiresAt"]) {
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

fn request(url: &str, headers: &[String], body: Option<&str>) -> Option<Value> {
    let mut command = Command::new("curl");
    command.args(["-sSf", "-m", "10", "-H", "@-"]);
    if let Some(body) = body {
        command.args(["-H", "Content-Type: application/json", "--data-binary", body]);
    }
    command.arg(url).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().ok()?;
    let text: String = headers.iter().map(|header| format!("{header}\n")).collect();
    let written = child.stdin.take().is_some_and(|mut stdin| stdin.write_all(text.as_bytes()).is_ok());
    let output = child.wait_with_output().ok()?;
    if !written || !output.status.success() {
        return None;
    }
    serde_json::from_slice(&output.stdout).ok()
}

fn anthropic(token: &str) -> Option<Vec<Limit>> {
    let headers = [format!("Authorization: Bearer {token}"), "anthropic-beta: oauth-2025-04-20".to_string()];
    Some(claude_limits(&request(ANTHROPIC_USAGE, &headers, None)?))
}

fn chatgpt(token: &str, account: Option<&str>) -> Option<Vec<Limit>> {
    let mut headers = vec![format!("Authorization: Bearer {token}")];
    headers.extend(account.map(|account| format!("ChatGPT-Account-Id: {account}")));
    Some(chatgpt_limits(&request(CHATGPT_USAGE, &headers, None)?, SystemTime::now()))
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

fn google(token: &str, project: Option<&str>, model: Option<&str>) -> Option<Vec<Limit>> {
    let headers = [format!("Authorization: Bearer {token}")];
    let project = match project {
        Some(project) => project.to_string(),
        None => {
            let body =
                r#"{"metadata":{"ideType":"IDE_UNSPECIFIED","platform":"PLATFORM_UNSPECIFIED","pluginType":"GEMINI"}}"#;
            let loaded = request(&format!("{CODE_ASSIST}:loadCodeAssist"), &headers, Some(body))?;
            let project = &loaded["cloudaicompanionProject"];
            project.as_str().or_else(|| project["id"].as_str())?.to_string()
        }
    };
    let body = serde_json::json!({ "project": project }).to_string();
    Some(gemini_limits(&request(&format!("{CODE_ASSIST}:retrieveUserQuota"), &headers, Some(&body))?, model))
}

fn gemini_limits(json: &Value, model: Option<&str>) -> Vec<Limit> {
    let buckets: Vec<(&Value, f64)> = json["buckets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|bucket| Some((bucket, bucket["remainingFraction"].as_f64()?)))
        .collect();
    let chosen = buckets
        .iter()
        .find(|(bucket, _)| model.is_some() && bucket["modelId"].as_str() == model)
        .or_else(|| buckets.iter().min_by(|a, b| a.1.total_cmp(&b.1)));
    chosen
        .map(|(bucket, remaining)| Limit {
            minutes: 1440,
            percent: ((1.0 - remaining) * 100.0).round().clamp(0.0, 100.0) as u64,
            resets: bucket["resetTime"].as_str().and_then(parse_time),
            scope: "",
        })
        .into_iter()
        .collect()
}

fn copilot(github_token: &str, enterprise: Option<&str>) -> Option<Vec<Limit>> {
    let host = enterprise.filter(|host| !host.is_empty()).unwrap_or("github.com");
    let host = host.trim_start_matches("https://").trim_end_matches('/');
    let url = format!("https://api.{host}/copilot_internal/user");
    Some(copilot_limits(&request(&url, &[format!("Authorization: token {github_token}")], None)?))
}

fn copilot_limits(json: &Value) -> Vec<Limit> {
    let premium = &json["quota_snapshots"]["premium_interactions"];
    let Some(remaining) = premium["percent_remaining"].as_f64().filter(|_| premium["unlimited"] != true) else {
        return Vec::new();
    };
    let reset = json["quota_reset_date_utc"].as_str().or_else(|| json["quota_reset_date"].as_str());
    let resets = reset.and_then(|date| parse_time(date).or_else(|| parse_time(&format!("{date}T00:00:00Z"))));
    vec![Limit { minutes: 43200, percent: (100.0 - remaining).round().clamp(0.0, 100.0) as u64, resets, scope: "" }]
}

fn chatgpt_limits(json: &Value, now: SystemTime) -> Vec<Limit> {
    ["primary_window", "secondary_window"]
        .into_iter()
        .filter_map(|key| {
            let window = &json["rate_limit"][key];
            let resets = window["reset_at"]
                .as_u64()
                .map(|seconds| UNIX_EPOCH + Duration::from_secs(seconds))
                .or_else(|| Some(now + Duration::from_secs(window["reset_after_seconds"].as_u64()?)));
            Some(Limit {
                minutes: window["limit_window_seconds"].as_u64()? / 60,
                percent: window["used_percent"].as_f64()?.round() as u64,
                resets,
                scope: "",
            })
        })
        .collect()
}

pub fn rollout(json: &Value, at: Option<SystemTime>) -> Vec<Limit> {
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
    fn spots_other_apis() {
        let env =
            |pairs: &[(&str, &str)]| pairs.iter().map(|(key, value)| (key.to_string(), value.to_string())).collect();
        assert!(!other_api(&env(&[])));
        assert!(!other_api(&env(&[("ANTHROPIC_BASE_URL", "https://api.anthropic.com")])));
        assert!(other_api(&env(&[("ANTHROPIC_BASE_URL", "https://api.z.ai/api/anthropic")])));
        assert!(other_api(&env(&[("ANTHROPIC_AUTH_TOKEN", "x")])));
    }

    #[test]
    fn reads_chatgpt_account() {
        use base64::Engine;
        let claims = r#"{"https://api.openai.com/auth":{"chatgpt_account_id":"account-1"}}"#;
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(claims);
        assert_eq!(chatgpt_account(&format!("header.{payload}.signature")).as_deref(), Some("account-1"));
        assert_eq!(chatgpt_account("not a token"), None);
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
        let codex: Value = serde_json::from_str(
            r#"{"primary":{"used_percent":12.5,"window_minutes":300,"resets_in_seconds":60},"secondary":{"used_percent":3.0,"window_minutes":10080,"resets_at":1000000000}}"#,
        )
        .unwrap();
        assert_eq!(
            rollout(&codex, at(100)),
            vec![
                Limit { minutes: 300, percent: 13, resets: at(160), scope: "" },
                Limit { minutes: 10080, percent: 3, resets: at(1000000000), scope: "" },
            ]
        );
        let chatgpt: Value = serde_json::from_str(
            r#"{"plan_type":"plus","rate_limit":{"allowed":true,"primary_window":{"used_percent":41,"limit_window_seconds":18000,"reset_after_seconds":30,"reset_at":null},"secondary_window":{"used_percent":9,"limit_window_seconds":604800,"reset_at":1000000000}}}"#,
        )
        .unwrap();
        let quota: Value = serde_json::from_str(
            r#"{"buckets":[{"modelId":"gemini-2.5-pro","remainingFraction":0.75,"resetTime":"2001-09-09T01:46:40Z","tokenType":"REQUESTS"},{"modelId":"gemini-2.5-flash","remainingFraction":0.5},{"modelId":"other"}]}"#,
        )
        .unwrap();
        let daily = |percent, resets| vec![Limit { minutes: 1440, percent, resets, scope: "" }];
        assert_eq!(gemini_limits(&quota, Some("gemini-2.5-pro")), daily(25, at(1000000000)));
        assert_eq!(gemini_limits(&quota, Some("unknown")), daily(50, None));
        assert_eq!(gemini_limits(&quota, None)[0].label(), "1d");
        assert_eq!(gemini_limits(&Value::Null, None), vec![]);
        let copilot: Value = serde_json::from_str(
            r#"{"quota_reset_date":"2001-09-09","quota_snapshots":{"chat":{"unlimited":true},"premium_interactions":{"entitlement":300,"remaining":129,"percent_remaining":43.2,"unlimited":false}}}"#,
        )
        .unwrap();
        let monthly = copilot_limits(&copilot);
        assert_eq!(monthly, vec![Limit { minutes: 43200, percent: 57, resets: at(999993600), scope: "" }]);
        assert_eq!(monthly[0].label(), "1mo");
        let unlimited: Value = serde_json::from_str(
            r#"{"quota_snapshots":{"premium_interactions":{"percent_remaining":100,"unlimited":true}}}"#,
        )
        .unwrap();
        assert_eq!(copilot_limits(&unlimited), vec![]);
        let limits = chatgpt_limits(&chatgpt, at(100).unwrap());
        assert_eq!(
            limits,
            vec![
                Limit { minutes: 300, percent: 41, resets: at(130), scope: "" },
                Limit { minutes: 10080, percent: 9, resets: at(1000000000), scope: "" },
            ]
        );
    }
}
