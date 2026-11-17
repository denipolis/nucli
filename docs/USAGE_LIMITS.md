# Usage limits

The usage limits status chip shows how much of an agent plan's limits (5-hour, 7-day, or daily) the account has used. It is off until you turn it on.

Some agents keep these limits in local files. Others report them only through their online API: for those, Nucli reads your usage with the sign-in token the agent itself saved. The token is never stored, refreshed, logged or sent anywhere other than the agent's own provider. Turning the chip off stops all requests.

| Agent | Source | Network | Token |
|---|---|---|---|
| Claude Code | 5-hour, 7-day and the per-model weekly limits a plan has (`7d opus`, `7d sonnet`) from `https://api.anthropic.com/api/oauth/usage`, asked with `curl` at most every 5 minutes per Claude Code config folder while Claude Code runs in a tab | Yes, Anthropic only | Claude Code's saved sign-in: the Keychain item `Claude Code-credentials` on macOS (with `-` and the first 8 hex digits of the config folder's SHA-256 for a custom config folder), else `<config>/.credentials.json`. An expired token is skipped. The token goes to `curl` on standard input, never on the command line. |
| Codex | The `rate_limits` of the latest `token_count` event in the session's rollout file (`~/.codex/sessions`). Before the first reply, while the rollout has none, `https://chatgpt.com/backend-api/wham/usage` | Only before the first reply, OpenAI only | `auth.json` in Codex's config folder (`tokens.access_token`, `tokens.account_id`), only for the online request |
| Gemini CLI | The daily request quota of the current model (or the most used one) from `https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuota`, after `:loadCodeAssist` for the account's Code Assist project id, at most every 5 minutes. Shown as `1d` | Yes, Google only | `oauth_creds.json` in Gemini CLI's config folder (Google sign-in only; API keys show nothing). An expired token is skipped. |
| pi | When pi's last reply came from a Claude plan (`anthropic`) or a ChatGPT plan (`openai-codex`) sign-in: the same Anthropic or OpenAI usage endpoint as above, at most every 5 minutes | Yes, Anthropic or OpenAI only | The `access` token of that provider in `auth.json` in pi's config folder (`~/.pi/agent`). An expired token is skipped. For ChatGPT, the account id is read from inside the token. |

Other agents (Qwen Code, opencode, Copilot CLI, Amp, Droid, Cursor Agent, crush, goose) do not report 5-hour and 7-day limits, so the chip does not show for them.
