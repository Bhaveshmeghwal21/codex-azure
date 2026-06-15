# Azure OpenAI Integration

This fork adds first-class Azure OpenAI support to Codex CLI. You can authenticate
once through the TUI sign-in flow **or** manage multiple Azure deployments at any
time using the `/azure` slash command — no rebuild required.

---

## First-time setup

1. Run `codex`.
2. Select **Provide your own Azure OpenAI details**.
3. Enter the following when prompted:

   | Field | Example |
   |---|---|
   | **Endpoint** | `https://YOUR-RESOURCE.openai.azure.com/openai` |
   | **API Key** | your Azure API key |
   | **API Version** | `2025-04-01-preview` |

4. Codex writes an `azure` provider block to `~/.codex/config.toml` and starts immediately.

> **Important — endpoint format:** Codex appends `/responses` to whatever you enter.
> The correct base is `https://YOUR-RESOURCE.openai.azure.com/openai` (include `/openai`,
> omit `/responses` and any deployment path). The final request URL becomes:
> `https://YOUR-RESOURCE.openai.azure.com/openai/responses?api-version=...`

---

## Managing providers with `/azure`

After first setup you can add, switch, and remove Azure providers without leaving
the chat using the `/azure` slash command.

### Add a provider

```
/azure add <id> --base-url <url> --api-version <version> --key <key> [--model <deployment>] [--context-window <tokens>] [--use]
```

| Flag | Required | Description |
|---|---|---|
| `<id>` | ✅ | Unique name for this provider (letters, numbers, `_`, `-`) |
| `--base-url` | ✅ | `https://YOUR-RESOURCE.openai.azure.com/openai` |
| `--api-version` | ✅ | e.g. `2025-04-01-preview` |
| `--key` | ✅ | Azure API key |
| `--model` | optional | Deployment name, e.g. `gpt-4o`, `gpt-5.5` |
| `--context-window` | optional | Token limit (default: `1050000`). Use `--no-context-window` to omit |
| `--use` | optional | Switch to this provider immediately after adding |

**Example — add GPT-5.5 deployment and switch to it:**
```
/azure add prod --base-url https://rebloom-openai.cognitiveservices.azure.com/openai --api-version 2025-04-01-preview --key sk-... --model gpt-5.5 --use
```

### Switch to an existing provider

```
/azure use <id> [--model <deployment>]
```

```
/azure use prod --model gpt-4o
```

### List all configured Azure providers

```
/azure list
```

Shows all providers whose `base_url` contains `.openai.azure.com` or `/openai`,
with the currently active one marked.

### Remove a provider

```
/azure remove <id>
```

> You cannot remove the currently active provider. Run `/azure use <other-id>` first.

---

## Config file reference

All providers are stored in `~/.codex/config.toml`. A typical Azure block looks like:

```toml
model_provider = "prod"
model = "gpt-5.5"

[model_providers.prod]
name = "prod"
base_url = "https://rebloom-openai.cognitiveservices.azure.com/openai"
experimental_bearer_token = "YOUR_API_KEY"
model_context_window = 1050000

[model_providers.prod.query_params]
api-version = "2025-04-01-preview"
```

You can edit this file directly or let the `/azure` command manage it for you.

---

## Reasoning and context window

GPT-5.5 and other reasoning-capable models work with full capability:

```toml
reasoning_effort = "high"   # low | medium | high
```

Add this to `~/.codex/config.toml` to maximise reasoning depth.

The context usage bar at the bottom of the TUI shows **% of context remaining**
and total tokens. If it does not appear, Azure may not be returning usage data
in the stream. Add this to your provider block to force it:

```toml
[model_providers.prod.extra_body]
stream_options = { include_usage = true }
```

---

## Session resumption

Old sessions (conversations you resume with `r` in the TUI) are replayed to Azure
as a sequence of input items. This fork includes a fix that ensures **reasoning
items are never orphaned** — if a reasoning item has an empty summary it is sent
as a minimal stub so Azure never rejects the resume with:

```
"Item 'msg_...' was provided without its required 'reasoning' item"
```

---

## Troubleshooting

| Error | Cause | Fix |
|---|---|---|
| `401 Unauthorized` | Wrong API key or endpoint | Check key and base URL format |
| `Invalid 'response_id': 'responses'` | `base_url` missing `/openai` suffix | Use `https://RESOURCE.openai.azure.com/openai` |
| `message provided without its required reasoning item` | Old session from pre-fix binary | Rebuild from source and replace `codex.exe` |
| Context bar not showing | Azure not returning usage in stream | Add `stream_options = { include_usage = true }` |
| `/compact` does nothing | Azure does not support the `/responses/compact` endpoint | Expected — use `/azure` to start a fresh session |

---

## Building from source (Windows)

```powershell
cd codex-rs
cargo build --release -p codex-tui
Copy-Item ".\target\release\codex-tui.exe" "C:\Users\<YOU>\codex.exe" -Force
```

For low-RAM machines (< 8 GB free):

```powershell
cargo build --profile release-light -p codex-tui
```

The `release-light` profile disables LTO and limits parallelism, reducing peak
RAM usage from ~4–6 GB to ~1.5–2 GB at the cost of a slightly larger binary.

---

## Building via GitHub Actions

Push your changes to GitHub and let the CI build the Windows binary for you:

1. Ensure `.github/workflows/build.yml` exists in the repo (see the workflow file
   already committed).
2. `git push` — the workflow triggers automatically.
3. Go to **Actions → latest run → Artifacts** and download `codex-exe.zip`.
4. Extract and copy `codex-tui.exe` to your PATH location as `codex.exe`.

Build time on GitHub's `windows-latest` runner (16 GB RAM, 4 cores): **~20–40 min**
for a first build, **~5–10 min** with dependency caching enabled.
