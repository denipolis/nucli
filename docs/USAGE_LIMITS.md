# Usage limits

The usage limits status chip shows how much of an agent plan's 5-hour and 7-day limits the account has used. It is off until you turn it on.

Some agents keep these limits in local files. Others report them only through their online API: for those, Nucli reads your usage with the sign-in token the agent itself saved. The token is never stored, refreshed, logged or sent anywhere other than the agent's own provider. Turning the chip off stops all requests.

| Agent | Source | Network | Token |
|---|---|---|---|
| Claude Code | `https://api.anthropic.com/api/oauth/usage`, asked with `curl` at most every 5 minutes per Claude Code config folder while Claude Code runs in a tab | Yes, Anthropic only | Claude Code's saved sign-in: the Keychain item `Claude Code-credentials` on macOS (with `-` and the first 8 hex digits of the config folder's SHA-256 for a custom config folder), else `<config>/.credentials.json`. An expired token is skipped. The token goes to `curl` on standard input, never on the command line. |
| Codex | The `rate_limits` of the latest `token_count` event in the session's rollout file (`~/.codex/sessions`) | No | Not used |

Other agents (Gemini CLI, pi, Qwen Code, opencode, Copilot CLI, Amp, Droid, Cursor Agent, crush, goose) do not report 5-hour and 7-day limits, so the chip does not show for them.
