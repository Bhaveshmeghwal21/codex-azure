<p align="center"><strong>Codex CLI</strong> is a coding agent from OpenAI that runs locally on your computer.
<p align="center">
  <img src="https://github.com/openai/codex/blob/main/.github/codex-cli-splash.png" alt="Codex CLI splash" width="80%" />
</p>
</br>
If you want Codex in your code editor (VS Code, Cursor, Windsurf), <a href="https://developers.openai.com/codex/ide">install in your IDE.</a>
</br>If you want the desktop app experience, run <code>codex app</code> or visit <a href="https://chatgpt.com/codex?app-landing-page=true">the Codex App page</a>.
</br>If you are looking for the <em>cloud-based agent</em> from OpenAI, <strong>Codex Web</strong>, go to <a href="https://chatgpt.com/codex">chatgpt.com/codex</a>.</p>

---

## About this fork

This repository tracks Codex CLI with Azure-focused and local workflow changes on top of the upstream OpenAI project.

- Azure OpenAI onboarding is available from the TUI sign-in flow. Choose **Provide your own Azure OpenAI details** and enter the deployment endpoint, API key, and optional API version.
- The app-server account API accepts an `azureOpenAi` login request and persists an `azure` model provider in `config.toml`.
- The TUI includes `/agent` worker commands for bounded subagent delegation: `spawn`, `explore`, `review`, `test`, `implement`, and `auto`.
- Clipboard image paste support has been adjusted for `Ctrl+V`/paste flows in the TUI.
- The local model catalog includes a `gpt-5.5` preset with extended context settings.
- The root `justfile` includes Windows-aware helpers for running the Rust workspace from this checkout.

## Quickstart

### Installing and running Codex CLI

Use the upstream installers below when you want the official OpenAI release.
To use the fork-specific changes in this repository, build from source instead.

Run the following on Mac or Linux to install the official Codex CLI:

```shell
curl -fsSL https://chatgpt.com/codex/install.sh | sh
```

Run the following on Windows to install the official Codex CLI:

```
powershell -ExecutionPolicy ByPass -c "irm https://chatgpt.com/codex/install.ps1 | iex"
```

The official Codex CLI can also be installed via the following package managers:

```shell
# Install using npm
npm install -g @openai/codex
```

```shell
# Install using Homebrew
brew install --cask codex
```

Then simply run `codex` to get started.

### Building this fork from source

From this checkout:

```shell
cd codex-rs
cargo build --release -p codex-tui
```

On Windows, copy the result to your PATH:

```powershell
Copy-Item ".\target\release\codex-tui.exe" "C:\Users\<YOU>\codex.exe" -Force
```

For low-RAM machines use the lighter profile:

```shell
cargo build --profile release-light -p codex-tui
```

After making Rust changes, run the repo helpers from the repository root:

```shell
just fmt
just test -p <crate-you-changed>
```

<details>
<summary>You can also go to the <a href="https://github.com/openai/codex/releases/latest">latest GitHub Release</a> and download the appropriate binary for your platform.</summary>

Each GitHub Release contains many executables, but in practice, you likely want one of these:

- macOS
  - Apple Silicon/arm64: `codex-aarch64-apple-darwin.tar.gz`
  - x86_64 (older Mac hardware): `codex-x86_64-apple-darwin.tar.gz`
- Linux
  - x86_64: `codex-x86_64-unknown-linux-musl.tar.gz`
  - arm64: `codex-aarch64-unknown-linux-musl.tar.gz`

Each archive contains a single entry with the platform baked into the name (e.g., `codex-x86_64-unknown-linux-musl`), so you likely want to rename it to `codex` after extracting it.

</details>

### Using Codex with your ChatGPT plan

Run `codex` and select **Sign in with ChatGPT**. We recommend signing into your ChatGPT account to use Codex as part of your Plus, Pro, Business, Edu, or Enterprise plan. [Learn more about what's included in your ChatGPT plan](https://help.openai.com/en/articles/11369540-codex-in-chatgpt).

You can also use Codex with an API key, but this requires [additional setup](https://developers.openai.com/codex/auth#sign-in-with-an-api-key).

### Using Azure OpenAI in this fork

Run `codex`, select **Provide your own Azure OpenAI details**, and enter:

| Field | Example |
|---|---|
| **Endpoint** | `https://YOUR-RESOURCE.openai.azure.com/openai` |
| **API Key** | your Azure API key |
| **API Version** | `2025-04-01-preview` |

> **Endpoint format:** include `/openai` at the end — do **not** include `/responses` or a deployment path. Codex appends `/responses` automatically.

This writes an `azure` model provider to `~/.codex/config.toml`. After setup, manage multiple Azure deployments at any time using the `/azure` slash command inside Codex — no rebuild required.

See **[Azure OpenAI Integration guide](./docs/azure-openai.md)** for full details including the `/azure` command reference, config file format, reasoning settings, session resume, troubleshooting, and CI build instructions.

## Docs

- [**Codex Documentation**](https://developers.openai.com/codex)
- [**Azure OpenAI Integration**](./docs/azure-openai.md)
- [**Contributing**](./docs/contributing.md)
- [**Installing & building**](./docs/install.md)
- [**Open source fund**](./docs/open-source-fund.md)

This repository is licensed under the [Apache-2.0 License](LICENSE).
