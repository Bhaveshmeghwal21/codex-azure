# Azure Provider Refactor — Design Spec

Date: 2026-08-10
Branch: `azure-peer-provider-refactor` (not merged to `main` without explicit approval)

## Context

This fork adds Azure OpenAI support on top of upstream `openai/codex`. A codebase
read (core loop, sandboxing, model-provider abstraction, app-server/plugins) plus
an idiomatic-Rust review found the Azure integration functionally works but is
architecturally a special case bolted onto the shared OpenAI code path, rather
than a peer implementation like the first-party Amazon Bedrock provider:

- `is_azure_responses_provider()` (`codex-api/src/provider.rs:106-127`) does a
  name/URL string match, re-derived at 7+ call sites: `model-provider-info/src/lib.rs:433`,
  `model-provider/src/provider.rs:301`, `model-provider/src/auth.rs:272`,
  `codex-api/src/provider.rs:88-89`, `core/src/client.rs:943,951,1176`,
  `cli/src/doctor.rs:2692`.
- `BearerAuthProvider.is_azure: bool` (`model-provider/src/bearer_auth_provider.rs:11`)
  is a flag-argument-style smell controlling two entirely different header
  behaviors (`api-key` vs `Authorization: Bearer`).
- Azure-only wire transforms live inline in `client.rs` (lines 943, 951,
  1176-1226): stripping `encrypted_content`, injecting a placeholder reasoning
  summary, `omit_null_encrypted_content`.
- `azure_command.rs` returns `Result<_, String>` instead of a typed error.
- `account_processor.rs:527-572` repeats
  `vec!["model_providers".to_string(), "azure".to_string(), field]` four times.
- `agent_worker_command.rs:216-247` bundles 8 unrelated parse scenarios into one
  `#[test]` function.
- Azure test coverage (~17 scattered test functions) is thin next to Bedrock's
  dedicated test modules (catalog, auth, error).

Goal stated by the repo owner: get the most reliable use out of Codex with Azure
API keys — not a security-hardening pass. Credential storage
(`experimental_bearer_token` in plaintext `config.toml`) is explicitly **out of
scope** for this work.

## Approach

Chosen over a full Bedrock-style reimplementation because Azure shares ~90% of
its request pipeline with OpenAI (same Responses API, same SSE streaming, same
retry logic) — only auth-header shape and a few payload quirks differ. A full
parallel `AzureModelProvider` struct would duplicate that shared pipeline.
Instead: resolve "is this Azure" once, thread the result through explicitly,
and centralize the actual differences in one place.

## Architecture

### `ProviderDialect` (new)

```rust
// codex-rs/model-provider-info/src/dialect.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderDialect {
    OpenAi,
    Azure,
}

impl ProviderDialect {
    /// Wraps the existing name/URL heuristic. Called once, at provider
    /// construction, not re-derived at each call site.
    pub fn detect(provider: &ModelProviderInfo) -> Self { ... }
}
```

A plain enum, not `dyn Trait` — two dialects, simple data-shaped differences
(header style, a couple of booleans), no need for dynamic dispatch or
heterogeneous collections.

### Auth

`BearerAuthProvider.is_azure: bool` becomes:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthHeaderStyle {
    Bearer,
    AzureApiKey,
}
```

Constructed once from `ProviderDialect` at provider setup, not threaded in as a
raw bool from `auth.rs:272`.

### Wire-format quirks

New module `codex-rs/core/src/client/azure_compat.rs` holds the three
Azure-only transforms currently inline in `client.rs`:
- `azure_compatible_input_item(...)` (strip `encrypted_content`, inject
  reasoning-summary stub)
- `should_omit_null_encrypted_content(dialect) -> bool`
- the `store` flag behavior

`client.rs` calls into this module from one site, gated on
`dialect == ProviderDialect::Azure`, replacing the current 3 separate
`is_azure_responses_endpoint()` checks.

### Call sites updated to consume `ProviderDialect` instead of re-detecting

- `codex-api/src/provider.rs`
- `cli/src/doctor.rs`
- `model-provider-info/src/lib.rs:433`
- `model-provider/src/provider.rs:301`

### Typed errors

`azure_command.rs`'s `Result<_, String>` becomes:

```rust
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum AzureCommandError {
    #[error("{AZURE_USAGE}")]
    Usage,
    #[error("Missing --base-url")]
    MissingBaseUrl,
    #[error("Missing --api-version")]
    MissingApiVersion,
    #[error("Missing --key")]
    MissingKey,
    #[error("--context-window must be an integer")]
    InvalidContextWindow,
    #[error("Provider id may contain only letters, numbers, `_`, and `-`.")]
    InvalidProviderId,
    #[error("Missing value for {0}")]
    MissingFlagValue(String),
    #[error("Unterminated quote in /azure command.")]
    UnterminatedQuote,
    #[error("Provider `{0}` is active. Run `/azure use <other-id>` before removing it.")]
    ProviderActive(String),
}
```

Display text is preserved exactly (byte-for-byte), so `/azure` command output
is unchanged for users. This is purely an internal typing change.

### Deduplication

`account_processor.rs` gets a helper:

```rust
fn azure_provider_path(field: &str) -> Vec<String> {
    vec!["model_providers".to_string(), "azure".to_string(), field.to_string()]
}
```

replacing the four hand-built copies of the same path shape.

## Data flow

Provider config loads → `ProviderDialect::detect()` runs once at provider
construction → the resolved value is stored on the provider and passed into
`BearerAuthProvider` (as `AuthHeaderStyle`) and into `client.rs`'s
request-building path (as the gate for `azure_compat` calls). Nothing
downstream re-inspects `base_url`/`name` strings.

## Error handling

Covered above under Typed errors. No change to error handling elsewhere;
`CodexResult`/`thiserror` conventions already used by the rest of the codebase
are followed, not introduced.

## Testing

- New test module for `ProviderDialect::detect()` covering the existing
  name/URL match cases (case-insensitive `"azure"` name, and the 6 URL
  substrings currently checked in `is_azure_responses_provider()`).
- New tests for the extracted `azure_compat` transforms (moved from wherever
  they're currently tested inline in `client.rs`, kept behaviorally identical).
- `agent_worker_command.rs`'s bundled 8-assertion test splits into 8 named
  `#[test]` functions (one parse scenario each).
- New tests for `AzureCommandError` `Display` output (chapter 4.6 guidance:
  unit tests should exercise error messages).
- Net effect: Azure gets a test module comparable in shape to Bedrock's
  (`catalog`/`auth`/`error` test split), not just parity in count.

### CI

Per repo owner's request, verification for this branch runs through GitHub
Actions rather than relying solely on local Windows builds (this fork's own
`rust-ci*.yml` / `build.yml` workflows already exist). The implementation plan
pushes the feature branch to `origin` (the user's own fork,
`Bhaveshmeghwal21/codex-azure`) — never `main` — and checks the resulting run
status before this is considered done. `just fmt` / `just test -p <crate>` /
`just fix -p <crate>` still run locally first as a fast pre-check.

## Non-goals

- No change to `config.toml` schema or `/azure` command syntax — this is an
  internal refactor. Existing users' config files keep working unmodified.
- No change to credential storage mechanism (explicitly out of scope).
- Bedrock's code (`model-provider/src/amazon_bedrock/*`) is untouched.
- No merge to `main` as part of this work; stays on
  `azure-peer-provider-refactor` until the repo owner explicitly asks to merge.

## Rollout / risk notes

- This fork has a documented history of merge pain against `openai/codex`
  upstream (many `fix: resolve CI build errors from upstream merge` commits).
  Every file this refactor touches (`client.rs`, `provider.rs`, `auth.rs`,
  `model-provider-info/src/lib.rs`) is a shared/high-churn file also touched by
  upstream. Keeping the diff small and behavior-preserving (no schema/CLI
  changes) minimizes future merge-conflict surface.
- `client.rs` and `session/*` are called out in this repo's own `AGENTS.md` as
  high-touch files; changes there should stay focused per that guidance.
