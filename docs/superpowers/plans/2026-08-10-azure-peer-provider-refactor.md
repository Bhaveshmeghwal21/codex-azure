# Azure Provider Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the "bolted on" Azure special-casing pattern (a boolean re-derived from raw strings, threaded around as an untyped flag) and replace it with a single typed `ProviderDialect`/`AuthHeaderStyle` vocabulary, typed command errors, deduplicated config-path building, and Azure test coverage comparable in shape to the first-party Bedrock integration — without touching credential storage, config schema, or `/azure` command syntax.

**Architecture:** See `docs/superpowers/specs/2026-08-10-azure-peer-provider-refactor-design.md`. Approach B: encapsulate Azure's differences behind types resolved once, keep the shared OpenAI Responses-API pipeline shared.

**Tech Stack:** Rust workspace (`codex-rs`), `thiserror` for typed errors, existing `insta`/`pretty_assertions` test conventions, `just fmt` / `just test -p <crate>` for local verification, GitHub Actions for CI verification (per repo owner's request — this fork already has `rust-ci*.yml` / `build.yml` workflows).

## Global Constraints

- Branch: `azure-peer-provider-refactor`. Never commit to `main`; never push without being asked.
- No change to `config.toml` schema (`experimental_bearer_token`, `query_params."api-version"`, etc.) or `/azure` command syntax. Existing users' config files must keep working unmodified.
- No change to credential storage mechanism (explicitly out of scope).
- Do not touch `codex-rs/model-provider/src/amazon_bedrock/*` behavior — only mechanical field-rename fallout there (Task 2).
- User-facing error/success message text must stay byte-identical unless a task says otherwise.
- After each task: run `just fmt` from `codex-rs/`, then `just test -p <crate>` for every crate touched in that task.
- Follow this repo's own `AGENTS.md` conventions: `pretty_assertions::assert_eq` in tests, exhaustive `match` (no wildcard arms) on the new enums, new test modules use `#[cfg(test)] #[path = "..._tests.rs"] mod tests;` when introduced fresh.

---

### Task 1: `ProviderDialect` in `codex-api`, consumed by `cli/doctor.rs`

**Files:**
- Modify: `codex-rs/codex-api/src/provider.rs:88-127`
- Modify: `codex-rs/codex-api/src/lib.rs:85`
- Modify: `codex-rs/cli/src/doctor.rs:2691-2693`

**Interfaces:**
- Produces: `codex_api::ProviderDialect` (`pub enum { OpenAi, Azure }`, `Debug, Clone, Copy, PartialEq, Eq`), with `ProviderDialect::detect(name: &str, base_url: Option<&str>) -> ProviderDialect` and `codex_api::Provider::dialect(&self) -> ProviderDialect`. Re-exported as `codex_api::ProviderDialect`. Task 2 consumes `ProviderDialect::detect`.

- [ ] **Step 1: Write the failing test**

Add to the existing `#[cfg(test)] mod tests` block in `codex-rs/codex-api/src/provider.rs` (after the existing `detects_azure_responses_base_urls` test, before the closing `}` of the module):

```rust
    #[test]
    fn provider_dialect_detects_azure_by_name_or_url() {
        assert_eq!(
            ProviderDialect::detect("Azure", Some("https://example.com")),
            ProviderDialect::Azure
        );
        assert_eq!(
            ProviderDialect::detect("test", Some("https://foo.openai.azure.com/openai")),
            ProviderDialect::Azure
        );
        assert_eq!(
            ProviderDialect::detect("OpenAI", Some("https://api.openai.com/v1")),
            ProviderDialect::OpenAi
        );
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd codex-rs && just test -p codex-api`
Expected: FAIL to compile — `ProviderDialect` is not defined.

- [ ] **Step 3: Implement `ProviderDialect`**

In `codex-rs/codex-api/src/provider.rs`, immediately above `pub fn is_azure_responses_provider` (currently line 106), insert:

```rust
/// Which API dialect a provider's HTTP endpoint speaks. Resolved once, at
/// provider-construction time, instead of re-derived from raw name/base_url
/// strings independently at each call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderDialect {
    OpenAi,
    Azure,
}

impl ProviderDialect {
    pub fn detect(name: &str, base_url: Option<&str>) -> Self {
        if is_azure_responses_provider(name, base_url) {
            Self::Azure
        } else {
            Self::OpenAi
        }
    }
}

```

Then replace the existing `Provider::is_azure_responses_endpoint` method (current lines 88-90) with:

```rust
    pub fn dialect(&self) -> ProviderDialect {
        ProviderDialect::detect(&self.name, Some(&self.base_url))
    }

    pub fn is_azure_responses_endpoint(&self) -> bool {
        self.dialect() == ProviderDialect::Azure
    }
```

(`is_azure_responses_endpoint` keeps its existing signature and behavior — `client.rs`'s two call sites at lines 943 and 951 are unchanged. This step only changes what backs it internally.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cd codex-rs && just test -p codex-api`
Expected: PASS, including the new `provider_dialect_detects_azure_by_name_or_url` test and the pre-existing `detects_azure_responses_base_urls` test.

- [ ] **Step 5: Re-export `ProviderDialect`**

In `codex-rs/codex-api/src/lib.rs`, next to the existing line 85 (`pub use crate::provider::is_azure_responses_provider;`), add:

```rust
pub use crate::provider::ProviderDialect;
```

- [ ] **Step 6: Consume it in `cli/src/doctor.rs`**

In `codex-rs/cli/src/doctor.rs`, change the import at line 33 from:

```rust
use codex_api::is_azure_responses_provider;
```

to:

```rust
use codex_api::ProviderDialect;
```

Then replace lines 2691-2693:

```rust
fn should_probe_models_route(provider_name: &str, base_url: &str, is_amazon_bedrock: bool) -> bool {
    !is_amazon_bedrock && !is_azure_responses_provider(provider_name, Some(base_url))
}
```

with:

```rust
fn should_probe_models_route(provider_name: &str, base_url: &str, is_amazon_bedrock: bool) -> bool {
    !is_amazon_bedrock && ProviderDialect::detect(provider_name, Some(base_url)) != ProviderDialect::Azure
}
```

- [ ] **Step 7: Run tests for both touched crates**

Run: `cd codex-rs && just test -p codex-api && just test -p codex-cli`
Expected: PASS, no behavior change (this is a pure refactor of `should_probe_models_route`'s internals).

- [ ] **Step 8: Format and commit**

```bash
cd codex-rs && just fmt
git add codex-rs/codex-api/src/provider.rs codex-rs/codex-api/src/lib.rs codex-rs/cli/src/doctor.rs
git commit -m "refactor: introduce ProviderDialect as single source of truth for Azure detection"
```

---

### Task 2: Replace `BearerAuthProvider.is_azure: bool` with `AuthHeaderStyle`

**Files:**
- Modify: `codex-rs/model-provider/src/bearer_auth_provider.rs`
- Modify: `codex-rs/model-provider/src/auth.rs:269-312`
- Modify: `codex-rs/model-provider/src/amazon_bedrock/auth.rs:62-67,195-200`

**Interfaces:**
- Consumes: `codex_api::ProviderDialect::detect` (Task 1).
- Produces: `codex_model_provider::bearer_auth_provider::AuthHeaderStyle` (`pub enum { Bearer, AzureApiKey }`, `Debug, Clone, Copy, PartialEq, Eq, Default` with `#[default] Bearer`). `BearerAuthProvider.auth_style: AuthHeaderStyle` replaces `is_azure: bool`.

- [ ] **Step 1: Update the struct and header logic in `bearer_auth_provider.rs`**

Replace the whole file `codex-rs/model-provider/src/bearer_auth_provider.rs` with:

```rust
use codex_api::AuthProvider;
use http::HeaderMap;
use http::HeaderValue;

/// Which auth header shape a provider expects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AuthHeaderStyle {
    #[default]
    Bearer,
    AzureApiKey,
}

/// Bearer-token auth provider for OpenAI-compatible model-provider requests.
#[derive(Clone, Default)]
pub struct BearerAuthProvider {
    pub token: Option<String>,
    pub account_id: Option<String>,
    pub is_fedramp_account: bool,
    pub auth_style: AuthHeaderStyle,
}

impl BearerAuthProvider {
    pub fn new(token: String) -> Self {
        Self {
            token: Some(token),
            account_id: None,
            is_fedramp_account: false,
            auth_style: AuthHeaderStyle::Bearer,
        }
    }

    pub fn for_test(token: Option<&str>, account_id: Option<&str>) -> Self {
        Self {
            token: token.map(str::to_string),
            account_id: account_id.map(str::to_string),
            is_fedramp_account: false,
            auth_style: AuthHeaderStyle::Bearer,
        }
    }
}

impl AuthProvider for BearerAuthProvider {
    fn add_auth_headers(&self, headers: &mut HeaderMap) {
        if let Some(token) = self.token.as_ref() {
            match self.auth_style {
                AuthHeaderStyle::AzureApiKey => {
                    if let Ok(header) = HeaderValue::from_str(token) {
                        let _ = headers.insert("api-key", header);
                    }
                }
                AuthHeaderStyle::Bearer => {
                    if let Ok(header) = HeaderValue::from_str(&format!("Bearer {token}")) {
                        let _ = headers.insert(http::header::AUTHORIZATION, header);
                    }
                }
            }
        }
        if let Some(account_id) = self.account_id.as_ref()
            && let Ok(header) = HeaderValue::from_str(account_id)
        {
            let _ = headers.insert("ChatGPT-Account-ID", header);
        }
        if self.is_fedramp_account {
            let _ = headers.insert("X-OpenAI-Fedramp", HeaderValue::from_static("true"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn bearer_auth_provider_reports_when_auth_header_will_attach() {
        let auth = BearerAuthProvider {
            token: Some("access-token".to_string()),
            account_id: None,
            is_fedramp_account: false,
            auth_style: AuthHeaderStyle::Bearer,
        };

        assert_eq!(
            codex_api::auth_header_telemetry(&auth),
            codex_api::AuthHeaderTelemetry {
                attached: true,
                name: Some("authorization"),
            }
        );
    }

    #[test]
    fn bearer_auth_provider_adds_auth_headers() {
        let auth = BearerAuthProvider::for_test(Some("access-token"), Some("workspace-123"));
        let mut headers = HeaderMap::new();

        auth.add_auth_headers(&mut headers);

        assert_eq!(
            headers
                .get(http::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer access-token")
        );
        assert_eq!(
            headers
                .get("ChatGPT-Account-ID")
                .and_then(|value| value.to_str().ok()),
            Some("workspace-123")
        );
    }

    #[test]
    fn bearer_auth_provider_adds_fedramp_routing_header_for_fedramp_accounts() {
        let auth = BearerAuthProvider {
            token: Some("access-token".to_string()),
            account_id: Some("workspace-123".to_string()),
            is_fedramp_account: true,
            auth_style: AuthHeaderStyle::Bearer,
        };
        let mut headers = HeaderMap::new();

        auth.add_auth_headers(&mut headers);

        assert_eq!(
            headers
                .get("X-OpenAI-Fedramp")
                .and_then(|value| value.to_str().ok()),
            Some("true")
        );
    }

    #[test]
    fn bearer_auth_provider_adds_azure_api_key_header() {
        let auth = BearerAuthProvider {
            token: Some("azure-key-123".to_string()),
            account_id: None,
            is_fedramp_account: false,
            auth_style: AuthHeaderStyle::AzureApiKey,
        };
        let mut headers = HeaderMap::new();

        auth.add_auth_headers(&mut headers);

        assert_eq!(
            headers.get("api-key").and_then(|value| value.to_str().ok()),
            Some("azure-key-123")
        );
        assert!(headers.get(http::header::AUTHORIZATION).is_none());
    }
}
```

- [ ] **Step 2: Run test to verify it fails elsewhere (compile errors at other construction sites)**

Run: `cd codex-rs && just test -p codex-model-provider`
Expected: FAIL to compile — `auth.rs` and `amazon_bedrock/auth.rs` still construct `BearerAuthProvider { is_azure: ..., .. }`.

- [ ] **Step 3: Fix `auth.rs`**

In `codex-rs/model-provider/src/auth.rs`, change the import at line 22 from:

```rust
use codex_api::is_azure_responses_provider;
```

to:

```rust
use codex_api::ProviderDialect;
```

Replace `bearer_auth_for_provider` (current lines 269-292):

```rust
fn bearer_auth_for_provider(
    provider: &ModelProviderInfo,
) -> codex_protocol::error::Result<Option<BearerAuthProvider>> {
    let is_azure = is_azure_responses_provider(&provider.name, provider.base_url.as_deref());
    if let Some(api_key) = provider.api_key()? {
        return Ok(Some(BearerAuthProvider {
            token: Some(api_key),
            account_id: None,
            is_fedramp_account: false,
            is_azure,
        }));
    }

    if let Some(token) = provider.experimental_bearer_token.clone() {
        return Ok(Some(BearerAuthProvider {
            token: Some(token),
            account_id: None,
            is_fedramp_account: false,
            is_azure,
        }));
    }

    Ok(None)
}
```

with:

```rust
fn bearer_auth_for_provider(
    provider: &ModelProviderInfo,
) -> codex_protocol::error::Result<Option<BearerAuthProvider>> {
    let auth_style = match ProviderDialect::detect(&provider.name, provider.base_url.as_deref()) {
        ProviderDialect::Azure => AuthHeaderStyle::AzureApiKey,
        ProviderDialect::OpenAi => AuthHeaderStyle::Bearer,
    };
    if let Some(api_key) = provider.api_key()? {
        return Ok(Some(BearerAuthProvider {
            token: Some(api_key),
            account_id: None,
            is_fedramp_account: false,
            auth_style,
        }));
    }

    if let Some(token) = provider.experimental_bearer_token.clone() {
        return Ok(Some(BearerAuthProvider {
            token: Some(token),
            account_id: None,
            is_fedramp_account: false,
            auth_style,
        }));
    }

    Ok(None)
}
```

Replace the `is_azure: false` at current line 309 (inside `auth_provider_from_auth`) with `auth_style: AuthHeaderStyle::Bearer,`.

Confirm `AuthHeaderStyle` is in scope in this file (check the existing `use` block near the top of `auth.rs` for `BearerAuthProvider`'s import path, e.g. `use crate::bearer_auth_provider::BearerAuthProvider;`, and add `AuthHeaderStyle` to that same `use` line).

- [ ] **Step 4: Fix `amazon_bedrock/auth.rs`**

In `codex-rs/model-provider/src/amazon_bedrock/auth.rs`, change `is_azure: false,` at line 66 (inside `resolve_provider_auth`) and at line 199 (inside a test) to `auth_style: AuthHeaderStyle::Bearer,`. Add `AuthHeaderStyle` to whatever `use` line already imports `BearerAuthProvider` in this file (check the top of the file — `BearerAuthProvider` is already imported since it's constructed here).

- [ ] **Step 5: Run tests to verify everything passes**

Run: `cd codex-rs && just test -p codex-model-provider`
Expected: PASS — all tests in `bearer_auth_provider.rs`, `auth.rs`, and `amazon_bedrock/auth.rs` green.

- [ ] **Step 6: Format and commit**

```bash
cd codex-rs && just fmt
git add codex-rs/model-provider/src/bearer_auth_provider.rs codex-rs/model-provider/src/auth.rs codex-rs/model-provider/src/amazon_bedrock/auth.rs
git commit -m "refactor: replace BearerAuthProvider is_azure bool with AuthHeaderStyle enum"
```

---

### Task 3: Extract Azure wire-compat logic out of `core/src/client.rs`

**Files:**
- Create: `codex-rs/core/src/client/azure_compat.rs`
- Create: `codex-rs/core/src/client/azure_compat_tests.rs`
- Modify: `codex-rs/core/src/client.rs:165-166,938,1172-1240` (remove moved code, add `mod` + call)
- Modify: `codex-rs/core/src/client_tests.rs` (remove the 3 tests and helpers that move; leave `azure_responses_request_omits_null_encrypted_content_on_wire` and its use of `azure_api_provider`/`openai_api_provider` in place)

**Interfaces:**
- Produces: `pub(crate) fn model_input_for_provider(provider: &codex_api::Provider, input: Vec<ResponseItem>) -> Vec<ResponseItem>` in `crate::client::azure_compat`, called from `client.rs:938` as `azure_compat::model_input_for_provider(provider, input)`.

- [ ] **Step 1: Create the new module with the moved implementation**

Create `codex-rs/core/src/client/azure_compat.rs`:

```rust
//! Wire-format adjustments needed when talking to Azure OpenAI's Responses
//! API deployment, as opposed to OpenAI's own. Kept separate from
//! `client.rs` so the Azure-specific surface area is easy to find, review,
//! and test in isolation.

use codex_protocol::models::FunctionCallOutputContentItem;
use codex_protocol::models::ReasoningItemReasoningSummary;
use codex_protocol::models::ResponseItem;

const AZURE_ENCRYPTED_TOOL_OUTPUT_UNAVAILABLE: &str =
    "[encrypted tool output unavailable for Azure provider]";

pub(crate) fn model_input_for_provider(
    provider: &codex_api::Provider,
    input: Vec<ResponseItem>,
) -> Vec<ResponseItem> {
    if provider.is_azure_responses_endpoint() {
        input
            .into_iter()
            .filter_map(azure_compatible_input_item)
            .collect()
    } else {
        input
    }
}

fn azure_compatible_input_item(mut item: ResponseItem) -> Option<ResponseItem> {
    match &mut item {
        ResponseItem::Reasoning {
            id,
            summary,
            content,
            encrypted_content,
            internal_chat_message_metadata_passthrough: _,
        } => {
            *encrypted_content = None;
            // Never drop a reasoning item that has an id, even if summary/content
            // are empty. Azure requires the reasoning item to be present whenever
            // its paired message item appears in the input. Dropping an empty
            // reasoning item causes the API to reject the next resume with:
            //   "Item 'msg_...' was provided without its required 'reasoning' item"
            // Instead, inject a minimal placeholder summary so the item is valid.
            if summary.is_empty() && content.as_ref().is_none_or(Vec::is_empty) {
                if id.as_ref().is_none_or(|id| id.is_empty()) {
                    // No id either — safe to drop, nothing to pair against.
                    return None;
                }
                summary.push(ReasoningItemReasoningSummary::SummaryText {
                    text: String::new(),
                });
            }
            Some(item)
        }
        ResponseItem::Compaction { .. } | ResponseItem::ContextCompaction { .. } => None,
        ResponseItem::FunctionCallOutput { output, .. }
        | ResponseItem::CustomToolCallOutput { output, .. } => {
            if let Some(items) = output.content_items_mut() {
                for item in items {
                    if matches!(item, FunctionCallOutputContentItem::EncryptedContent { .. }) {
                        *item = FunctionCallOutputContentItem::InputText {
                            text: AZURE_ENCRYPTED_TOOL_OUTPUT_UNAVAILABLE.to_string(),
                        };
                    }
                }
            }
            Some(item)
        }
        ResponseItem::Message { .. }
        | ResponseItem::AgentMessage { .. }
        | ResponseItem::AdditionalTools { .. }
        | ResponseItem::LocalShellCall { .. }
        | ResponseItem::FunctionCall { .. }
        | ResponseItem::ToolSearchCall { .. }
        | ResponseItem::ToolSearchOutput { .. }
        | ResponseItem::CustomToolCall { .. }
        | ResponseItem::WebSearchCall { .. }
        | ResponseItem::ImageGenerationCall { .. }
        | ResponseItem::CompactionTrigger {}
        | ResponseItem::Other => Some(item),
    }
}

#[cfg(test)]
#[path = "azure_compat_tests.rs"]
mod tests;
```

Note: check `codex-rs/core/src/client_tests.rs`'s existing imports for `ContentItem`, `FunctionCallOutputPayload`, `ModelProviderInfo`, and the `to_api_provider` extension — the new test file (Step 2) needs the same imports since `azure_api_provider()`/`openai_api_provider()` are being duplicated there (test helpers are cheap to duplicate — see the rust-best-practices guidance already applied elsewhere in this codebase: shared *production* code should be deduplicated, shared *test setup* is fine to repeat so each test file reads standalone).

- [ ] **Step 2: Create the moved test file**

Create `codex-rs/core/src/client/azure_compat_tests.rs` with the 3 pure `model_input_for_provider` tests currently in `client_tests.rs` (lines 280-294 for the two helpers, 296-412, 414-438, and 484-497), adapted to the new module path:

```rust
use super::model_input_for_provider;
use codex_protocol::config_types::ModelProviderInfo;
use codex_protocol::models::ContentItem;
use codex_protocol::models::FunctionCallOutputContentItem;
use codex_protocol::models::FunctionCallOutputPayload;
use codex_protocol::models::ReasoningItemReasoningSummary;
use codex_protocol::models::ResponseItem;
use pretty_assertions::assert_eq;

fn azure_api_provider() -> codex_api::Provider {
    ModelProviderInfo {
        name: "Azure".to_string(),
        base_url: Some("https://example.openai.azure.com/openai".to_string()),
        ..Default::default()
    }
    .to_api_provider(/*auth_mode*/ None)
    .expect("azure test provider should convert to api provider")
}

fn openai_api_provider() -> codex_api::Provider {
    ModelProviderInfo::create_openai_provider(/*base_url*/ None)
        .to_api_provider(/*auth_mode*/ None)
        .expect("openai test provider should convert to api provider")
}

#[test]
fn azure_model_input_omits_replayed_encrypted_content_without_mutating_history() {
    let user = ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: "hello".to_string(),
        }],
        phase: None,
    };
    let reasoning = ResponseItem::Reasoning {
        id: "rs_1".to_string(),
        summary: vec![],
        content: None,
        encrypted_content: Some("stale-reasoning".to_string()),
    };
    let compacted_summary = ResponseItem::Compaction {
        encrypted_content: "stale-compaction".to_string(),
    };
    let function_call = ResponseItem::FunctionCall {
        id: None,
        name: "web.run".to_string(),
        namespace: None,
        arguments: "{}".to_string(),
        call_id: "call_1".to_string(),
    };
    let encrypted_output = ResponseItem::FunctionCallOutput {
        call_id: "call_1".to_string(),
        output: FunctionCallOutputPayload::from_content_items(vec![
            FunctionCallOutputContentItem::EncryptedContent {
                encrypted_content: "stale-tool-output".to_string(),
            },
        ]),
    };
    let custom_tool_call = ResponseItem::CustomToolCall {
        id: None,
        status: None,
        call_id: "custom_call_1".to_string(),
        name: "custom-tool".to_string(),
        input: "{}".to_string(),
    };
    let encrypted_custom_output = ResponseItem::CustomToolCallOutput {
        call_id: "custom_call_1".to_string(),
        name: Some("custom-tool".to_string()),
        output: FunctionCallOutputPayload::from_content_items(vec![
            FunctionCallOutputContentItem::EncryptedContent {
                encrypted_content: "stale-custom-output".to_string(),
            },
        ]),
    };
    let input = vec![
        user.clone(),
        reasoning.clone(),
        compacted_summary,
        function_call.clone(),
        encrypted_output,
        custom_tool_call.clone(),
        encrypted_custom_output,
    ];

    let projected = model_input_for_provider(&azure_api_provider(), input.clone());

    assert_eq!(
        projected,
        vec![
            user.clone(),
            function_call.clone(),
            ResponseItem::FunctionCallOutput {
                call_id: "call_1".to_string(),
                output: FunctionCallOutputPayload::from_content_items(vec![
                    FunctionCallOutputContentItem::InputText {
                        text: "[encrypted tool output unavailable for Azure provider]".to_string(),
                    },
                ]),
            },
            custom_tool_call.clone(),
            ResponseItem::CustomToolCallOutput {
                call_id: "custom_call_1".to_string(),
                name: Some("custom-tool".to_string()),
                output: FunctionCallOutputPayload::from_content_items(vec![
                    FunctionCallOutputContentItem::InputText {
                        text: "[encrypted tool output unavailable for Azure provider]".to_string(),
                    },
                ]),
            },
        ]
    );
    assert_eq!(
        input,
        vec![
            user,
            reasoning,
            ResponseItem::Compaction {
                encrypted_content: "stale-compaction".to_string(),
            },
            function_call,
            ResponseItem::FunctionCallOutput {
                call_id: "call_1".to_string(),
                output: FunctionCallOutputPayload::from_content_items(vec![
                    FunctionCallOutputContentItem::EncryptedContent {
                        encrypted_content: "stale-tool-output".to_string(),
                    },
                ]),
            },
            custom_tool_call,
            ResponseItem::CustomToolCallOutput {
                call_id: "custom_call_1".to_string(),
                name: Some("custom-tool".to_string()),
                output: FunctionCallOutputPayload::from_content_items(vec![
                    FunctionCallOutputContentItem::EncryptedContent {
                        encrypted_content: "stale-custom-output".to_string(),
                    },
                ]),
            },
        ]
    );
}

#[test]
fn azure_model_input_preserves_reasoning_summary_without_encrypted_content() {
    let input = vec![ResponseItem::Reasoning {
        id: "rs_1".to_string(),
        summary: vec![ReasoningItemReasoningSummary::SummaryText {
            text: "readable summary".to_string(),
        }],
        content: None,
        encrypted_content: Some("stale-reasoning".to_string()),
    }];

    let projected = model_input_for_provider(&azure_api_provider(), input);

    assert_eq!(
        projected,
        vec![ResponseItem::Reasoning {
            id: "rs_1".to_string(),
            summary: vec![ReasoningItemReasoningSummary::SummaryText {
                text: "readable summary".to_string(),
            }],
            content: None,
            encrypted_content: None,
        }]
    );
}

#[test]
fn non_azure_model_input_preserves_encrypted_content() {
    let input = vec![ResponseItem::Reasoning {
        id: "rs_1".to_string(),
        summary: vec![],
        content: None,
        encrypted_content: Some("provider-owned-state".to_string()),
    }];

    assert_eq!(
        model_input_for_provider(&openai_api_provider(), input.clone()),
        input
    );
}
```

If `ModelProviderInfo::create_openai_provider` or `.to_api_provider` are not directly reachable via `codex_protocol::config_types::ModelProviderInfo` from this new path, match whatever import path `client_tests.rs` currently uses for the same calls (check the top of `client_tests.rs` for its `ModelProviderInfo` import before assuming the path above).

- [ ] **Step 3: Remove the moved code from `client.rs`**

In `codex-rs/core/src/client.rs`:
- Delete the `AZURE_ENCRYPTED_TOOL_OUTPUT_UNAVAILABLE` const (lines 165-166).
- Delete `model_input_for_provider` and `azure_compatible_input_item` (lines 1172-1240).
- At line 938, change `input: model_input_for_provider(provider, input),` to `input: azure_compat::model_input_for_provider(provider, input),`.
- Add `mod azure_compat;` near the top of the file, alongside other `mod` declarations (search the file for existing `mod ` lines near the top imports and place it there; if there are none, add it directly after the last `use` statement).

- [ ] **Step 4: Remove the moved tests and helpers from `client_tests.rs`**

In `codex-rs/core/src/client_tests.rs`, delete:
- `azure_api_provider()` and `openai_api_provider()` (lines 280-294) — but only if nothing else in this file still needs them. Check for remaining usages first: `azure_responses_request_omits_null_encrypted_content_on_wire` (around line 440) uses `azure_api_provider()` — so **do not delete the helpers**, only delete the 3 moved `#[test]` functions: `azure_model_input_omits_replayed_encrypted_content_without_mutating_history` (296-412), `azure_model_input_preserves_reasoning_summary_without_encrypted_content` (414-438), and `non_azure_model_input_preserves_encrypted_content` (484-497). Leave `azure_api_provider()`, `openai_api_provider()`, and `azure_responses_request_omits_null_encrypted_content_on_wire` in place — that test calls `client.build_responses_request(...)`, a method that stays in `client.rs`, not the extracted module.
- Also delete the now-unused `use super::model_input_for_provider;` import at line 12 of `client_tests.rs`.

- [ ] **Step 5: Run tests to verify everything passes**

Run: `cd codex-rs && just test -p codex-core`
Expected: PASS. The 3 moved tests now run from `codex_core::client::azure_compat::tests::*`; `azure_responses_request_omits_null_encrypted_content_on_wire` still passes from `client_tests.rs`.

- [ ] **Step 6: Format and commit**

```bash
cd codex-rs && just fmt
git add codex-rs/core/src/client.rs codex-rs/core/src/client_tests.rs codex-rs/core/src/client/azure_compat.rs codex-rs/core/src/client/azure_compat_tests.rs
git commit -m "refactor: extract Azure Responses API wire-compat logic into its own module"
```

---

### Task 4: Typed `AzureCommandError` in `azure_command.rs`

**Files:**
- Modify: `codex-rs/tui/src/azure_command.rs` (entire file — see below)
- Modify: `codex-rs/tui/src/chatwidget/slash_dispatch.rs:985-1001`

**Interfaces:**
- Produces: `pub(crate) enum AzureCommandError` (see variants below) implementing `thiserror::Error + Debug + PartialEq + Eq`. `parse_azure_command`, `build_write_request`, and their helpers return `Result<_, AzureCommandError>` instead of `Result<_, String>`.

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` block at the bottom of `codex-rs/tui/src/azure_command.rs` (after the existing two tests):

```rust
    #[test]
    fn azure_command_error_messages_match_previous_string_errors() {
        assert_eq!(AzureCommandError::Usage.to_string(), AZURE_USAGE);
        assert_eq!(
            AzureCommandError::MissingBaseUrl.to_string(),
            "Missing --base-url"
        );
        assert_eq!(
            AzureCommandError::MissingApiVersion.to_string(),
            "Missing --api-version"
        );
        assert_eq!(AzureCommandError::MissingKey.to_string(), "Missing --key");
        assert_eq!(
            AzureCommandError::InvalidContextWindow.to_string(),
            "--context-window must be an integer"
        );
        assert_eq!(
            AzureCommandError::InvalidProviderId.to_string(),
            "Provider id may contain only letters, numbers, `_`, and `-`."
        );
        assert_eq!(
            AzureCommandError::MissingFlagValue("--model".to_string()).to_string(),
            "Missing value for --model"
        );
        assert_eq!(
            AzureCommandError::UnterminatedQuote.to_string(),
            "Unterminated quote in /azure command."
        );
        assert_eq!(
            AzureCommandError::ProviderActive("prod".to_string()).to_string(),
            "Provider `prod` is active. Run `/azure use <other-id>` before removing it."
        );
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd codex-rs && just test -p codex-tui`
Expected: FAIL to compile — `AzureCommandError` is not defined.

- [ ] **Step 3: Replace the whole file with the typed-error version**

Replace `codex-rs/tui/src/azure_command.rs` in full with:

```rust
use codex_app_server_protocol::ConfigEdit;
use serde_json::json;

use crate::config_update::clear_config_value;
use crate::config_update::replace_config_value;
use crate::legacy_core::config::Config;

pub(crate) const AZURE_USAGE: &str = "Usage: /azure list | /azure add <id> --base-url <url> --api-version <version> --key <key> [--model <deployment>] [--context-window <tokens>] [--use] | /azure use <id> [--model <deployment>] | /azure remove <id>";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AzureCommand {
    List,
    Add(AzureAddArgs),
    Use(AzureUseArgs),
    Remove { id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AzureAddArgs {
    pub(crate) id: String,
    pub(crate) base_url: String,
    pub(crate) api_version: String,
    pub(crate) key: String,
    pub(crate) model: Option<String>,
    pub(crate) context_window: Option<i64>,
    pub(crate) use_provider: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AzureUseArgs {
    pub(crate) id: String,
    pub(crate) model: Option<String>,
}

pub(crate) struct AzureWriteRequest {
    pub(crate) edits: Vec<ConfigEdit>,
    pub(crate) success_message: String,
}

pub(crate) fn parse_azure_command(input: &str) -> Result<AzureCommand, AzureCommandError> {
    let tokens = split_args(input)?;
    let Some((verb, rest)) = tokens.split_first() else {
        return Err(AzureCommandError::Usage);
    };
    match verb.as_str() {
        "list" => Ok(AzureCommand::List),
        "add" => parse_add(rest),
        "use" => parse_use(rest),
        "remove" | "rm" => parse_remove(rest),
        _ => Err(AzureCommandError::Usage),
    }
}

pub(crate) fn build_write_request(
    command: AzureCommand,
    config: &Config,
) -> Result<AzureWriteRequest, AzureCommandError> {
    match command {
        AzureCommand::List => Err(AzureCommandError::Usage),
        AzureCommand::Add(args) => Ok(build_add_request(args)),
        AzureCommand::Use(args) => Ok(build_use_request(args, config)),
        AzureCommand::Remove { id } => build_remove_request(id, config),
    }
}

pub(crate) fn list_providers(config: &Config) -> String {
    let mut rows = config
        .model_providers
        .iter()
        .filter(|(_, provider)| {
            provider
                .base_url
                .as_deref()
                .is_some_and(|url| url.contains(".openai.azure.com") || url.contains("/openai"))
        })
        .map(|(id, provider)| {
            let active = if id == &config.model_provider_id {
                " active"
            } else {
                ""
            };
            let base_url = provider.base_url.as_deref().unwrap_or("-");
            format!("{id}{active}: {base_url}")
        })
        .collect::<Vec<_>>();
    rows.sort();
    if rows.is_empty() {
        "No Azure providers configured.".to_string()
    } else {
        rows.join("\n")
    }
}

fn parse_add(tokens: &[String]) -> Result<AzureCommand, AzureCommandError> {
    let Some((id, rest)) = tokens.split_first() else {
        return Err(AzureCommandError::Usage);
    };
    validate_provider_id(id)?;
    let mut base_url = None;
    let mut api_version = None;
    let mut key = None;
    let mut model = None;
    let mut context_window = Some(1_050_000);
    let mut use_provider = false;
    let mut idx = 0;
    while idx < rest.len() {
        match rest[idx].as_str() {
            "--base-url" => {
                base_url = Some(require_value(rest, &mut idx, "--base-url")?);
            }
            "--api-version" => {
                api_version = Some(require_value(rest, &mut idx, "--api-version")?);
            }
            "--key" => {
                key = Some(require_value(rest, &mut idx, "--key")?);
            }
            "--model" => {
                model = Some(require_value(rest, &mut idx, "--model")?);
            }
            "--context-window" => {
                let value = require_value(rest, &mut idx, "--context-window")?;
                context_window = Some(
                    value
                        .parse::<i64>()
                        .map_err(|_| AzureCommandError::InvalidContextWindow)?,
                );
            }
            "--no-context-window" => {
                context_window = None;
                idx += 1;
            }
            "--use" => {
                use_provider = true;
                idx += 1;
            }
            _ => return Err(AzureCommandError::Usage),
        }
    }
    Ok(AzureCommand::Add(AzureAddArgs {
        id: id.to_string(),
        base_url: base_url.ok_or(AzureCommandError::MissingBaseUrl)?,
        api_version: api_version.ok_or(AzureCommandError::MissingApiVersion)?,
        key: key.ok_or(AzureCommandError::MissingKey)?,
        model,
        context_window,
        use_provider,
    }))
}

fn parse_use(tokens: &[String]) -> Result<AzureCommand, AzureCommandError> {
    let Some((id, rest)) = tokens.split_first() else {
        return Err(AzureCommandError::Usage);
    };
    validate_provider_id(id)?;
    let mut model = None;
    let mut idx = 0;
    while idx < rest.len() {
        match rest[idx].as_str() {
            "--model" => {
                model = Some(require_value(rest, &mut idx, "--model")?);
            }
            _ => return Err(AzureCommandError::Usage),
        }
    }
    Ok(AzureCommand::Use(AzureUseArgs {
        id: id.to_string(),
        model,
    }))
}

fn parse_remove(tokens: &[String]) -> Result<AzureCommand, AzureCommandError> {
    match tokens {
        [id] => {
            validate_provider_id(id)?;
            Ok(AzureCommand::Remove { id: id.to_string() })
        }
        _ => Err(AzureCommandError::Usage),
    }
}

fn build_add_request(args: AzureAddArgs) -> AzureWriteRequest {
    let mut edits = vec![
        replace_config_value(
            format!("model_providers.{}.name", args.id),
            json!(args.id.clone()),
        ),
        replace_config_value(
            format!("model_providers.{}.base_url", args.id),
            json!(args.base_url),
        ),
        replace_config_value(
            format!("model_providers.{}.experimental_bearer_token", args.id),
            json!(args.key),
        ),
        replace_config_value(
            format!("model_providers.{}.query_params.\"api-version\"", args.id),
            json!(args.api_version),
        ),
    ];
    if let Some(context_window) = args.context_window {
        edits.push(replace_config_value(
            format!("model_providers.{}.model_context_window", args.id),
            json!(context_window),
        ));
    }
    if args.use_provider {
        edits.push(replace_config_value("model_provider", json!(args.id)));
    }
    if let Some(model) = args.model {
        edits.push(replace_config_value("model", json!(model)));
    }
    AzureWriteRequest {
        edits,
        success_message: if args.use_provider {
            format!("Azure provider `{}` added and selected.", args.id)
        } else {
            format!("Azure provider `{}` added.", args.id)
        },
    }
}

fn build_use_request(args: AzureUseArgs, config: &Config) -> AzureWriteRequest {
    let mut edits = vec![replace_config_value("model_provider", json!(args.id))];
    if let Some(model) = args.model {
        edits.push(replace_config_value("model", json!(model)));
    }
    let current_model = config
        .model
        .clone()
        .unwrap_or_else(|| "current model".to_string());
    AzureWriteRequest {
        edits,
        success_message: format!("Azure provider `{}` selected for {current_model}.", args.id),
    }
}

fn build_remove_request(id: String, config: &Config) -> Result<AzureWriteRequest, AzureCommandError> {
    if id == config.model_provider_id {
        return Err(AzureCommandError::ProviderActive(id));
    }
    Ok(AzureWriteRequest {
        edits: vec![clear_config_value(format!("model_providers.{id}"))],
        success_message: format!("Azure provider `{id}` removed."),
    })
}

fn require_value(tokens: &[String], idx: &mut usize, flag: &str) -> Result<String, AzureCommandError> {
    let value_index = *idx + 1;
    let Some(value) = tokens.get(value_index) else {
        return Err(AzureCommandError::MissingFlagValue(flag.to_string()));
    };
    if value.starts_with("--") {
        return Err(AzureCommandError::MissingFlagValue(flag.to_string()));
    }
    *idx += 2;
    Ok(value.clone())
}

fn validate_provider_id(id: &str) -> Result<(), AzureCommandError> {
    if id.is_empty()
        || !id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
    {
        return Err(AzureCommandError::InvalidProviderId);
    }
    Ok(())
}

fn split_args(input: &str) -> Result<Vec<String>, AzureCommandError> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut chars = input.chars();
    let mut quote = None;
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            '"' | '\'' => {
                if quote == Some(ch) {
                    quote = None;
                } else if quote.is_none() {
                    quote = Some(ch);
                } else {
                    current.push(ch);
                }
            }
            ch if ch.is_whitespace() && quote.is_none() => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }
    if quote.is_some() {
        return Err(AzureCommandError::UnterminatedQuote);
    }
    if !current.is_empty() {
        args.push(current);
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn parse_add_accepts_required_azure_fields() {
        let command = parse_azure_command(
            "add azure2 --base-url https://example.openai.azure.com/openai --api-version 2025-04-01-preview --key secret --model gpt-5.5 --use",
        )
        .expect("parse add");

        assert_eq!(
            command,
            AzureCommand::Add(AzureAddArgs {
                id: "azure2".to_string(),
                base_url: "https://example.openai.azure.com/openai".to_string(),
                api_version: "2025-04-01-preview".to_string(),
                key: "secret".to_string(),
                model: Some("gpt-5.5".to_string()),
                context_window: Some(1_050_000),
                use_provider: true,
            })
        );
    }

    #[test]
    fn build_add_request_writes_provider_and_api_version() {
        let request = build_add_request(AzureAddArgs {
            id: "azure2".to_string(),
            base_url: "https://example.openai.azure.com/openai".to_string(),
            api_version: "2025-04-01-preview".to_string(),
            key: "secret".to_string(),
            model: Some("gpt-5.5".to_string()),
            context_window: Some(1_050_000),
            use_provider: true,
        });

        let key_paths = request
            .edits
            .iter()
            .map(|edit| edit.key_path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            key_paths,
            vec![
                "model_providers.azure2.name",
                "model_providers.azure2.base_url",
                "model_providers.azure2.experimental_bearer_token",
                "model_providers.azure2.query_params.\"api-version\"",
                "model_providers.azure2.model_context_window",
                "model_provider",
                "model",
            ]
        );
    }

    #[test]
    fn azure_command_error_messages_match_previous_string_errors() {
        assert_eq!(AzureCommandError::Usage.to_string(), AZURE_USAGE);
        assert_eq!(
            AzureCommandError::MissingBaseUrl.to_string(),
            "Missing --base-url"
        );
        assert_eq!(
            AzureCommandError::MissingApiVersion.to_string(),
            "Missing --api-version"
        );
        assert_eq!(AzureCommandError::MissingKey.to_string(), "Missing --key");
        assert_eq!(
            AzureCommandError::InvalidContextWindow.to_string(),
            "--context-window must be an integer"
        );
        assert_eq!(
            AzureCommandError::InvalidProviderId.to_string(),
            "Provider id may contain only letters, numbers, `_`, and `-`."
        );
        assert_eq!(
            AzureCommandError::MissingFlagValue("--model".to_string()).to_string(),
            "Missing value for --model"
        );
        assert_eq!(
            AzureCommandError::UnterminatedQuote.to_string(),
            "Unterminated quote in /azure command."
        );
        assert_eq!(
            AzureCommandError::ProviderActive("prod".to_string()).to_string(),
            "Provider `prod` is active. Run `/azure use <other-id>` before removing it."
        );
    }
}
```

Note: `AzureCommandError` implements `Error` via `thiserror`'s derive, and every fallible helper (`split_args`, `require_value`, `validate_provider_id`) now returns `Result<_, AzureCommandError>` so the `?` operator in `parse_add`/`parse_use`/`parse_remove`/`split_args`-callers keeps working without `.map_err(...)` boilerplate.

- [ ] **Step 4: Update the call site in `slash_dispatch.rs`**

In `codex-rs/tui/src/chatwidget/slash_dispatch.rs`, replace lines 990-1000:

```rust
            Ok(command) => match azure_command::build_write_request(command, &self.config) {
                Ok(request) => {
                    self.app_event_tx.send(AppEvent::PersistAzureProvider {
                        edits: request.edits,
                        success_message: request.success_message,
                    });
                }
                Err(err) => self.add_error_message(err),
            },
            Err(err) => self.add_error_message(err),
        }
    }
```

with:

```rust
            Ok(command) => match azure_command::build_write_request(command, &self.config) {
                Ok(request) => {
                    self.app_event_tx.send(AppEvent::PersistAzureProvider {
                        edits: request.edits,
                        success_message: request.success_message,
                    });
                }
                Err(err) => self.add_error_message(err.to_string()),
            },
            Err(err) => self.add_error_message(err.to_string()),
        }
    }
```

- [ ] **Step 5: Run tests to verify everything passes**

Run: `cd codex-rs && just test -p codex-tui`
Expected: PASS, including the new `azure_command_error_messages_match_previous_string_errors` test.

- [ ] **Step 6: Format and commit**

```bash
cd codex-rs && just fmt
git add codex-rs/tui/src/azure_command.rs codex-rs/tui/src/chatwidget/slash_dispatch.rs
git commit -m "refactor: use typed AzureCommandError instead of String in azure_command.rs"
```

---

### Task 5: Deduplicate config-path building in `account_processor.rs`

**Files:**
- Modify: `codex-rs/app-server/src/request_processors/account_processor.rs:485-586`

**Interfaces:** none (private helper, no cross-task dependency).

- [ ] **Step 1: Add the helper and use it**

In `codex-rs/app-server/src/request_processors/account_processor.rs`, add this free function near `login_azure_common` (directly above it):

```rust
fn azure_provider_path(field: &str) -> Vec<String> {
    vec![
        "model_providers".to_string(),
        "azure".to_string(),
        field.to_string(),
    ]
}
```

Then replace the `edits` construction inside `login_azure_common` (current lines 527-573):

```rust
        let mut edits = vec![
            // Activate this provider.
            ConfigEdit::SetPath {
                segments: vec!["model_provider".to_string()],
                value: toml_edit::value("azure"),
            },
            // Name must equal "azure" (case-insensitive) for the Azure header logic.
            ConfigEdit::SetPath {
                segments: vec![
                    "model_providers".to_string(),
                    "azure".to_string(),
                    "name".to_string(),
                ],
                value: toml_edit::value("azure"),
            },
            // Azure resource endpoint, e.g. https://my-resource.openai.azure.com
            ConfigEdit::SetPath {
                segments: vec![
                    "model_providers".to_string(),
                    "azure".to_string(),
                    "base_url".to_string(),
                ],
                value: toml_edit::value(endpoint),
            },
            // Embed the API key directly – no env-var lookup needed at request time.
            ConfigEdit::SetPath {
                segments: vec![
                    "model_providers".to_string(),
                    "azure".to_string(),
                    "experimental_bearer_token".to_string(),
                ],
                value: toml_edit::value(api_key),
            },
        ];

        // Only persist the api-version when the user provided a non-empty value.
        if let Some(version) = api_version.filter(|v| !v.trim().is_empty()) {
            edits.push(ConfigEdit::SetPath {
                segments: vec![
                    "model_providers".to_string(),
                    "azure".to_string(),
                    "query_params".to_string(),
                    "api-version".to_string(),
                ],
                value: toml_edit::value(version),
            });
        }
```

with:

```rust
        let mut edits = vec![
            // Activate this provider.
            ConfigEdit::SetPath {
                segments: vec!["model_provider".to_string()],
                value: toml_edit::value("azure"),
            },
            // Name must equal "azure" (case-insensitive) for the Azure header logic.
            ConfigEdit::SetPath {
                segments: azure_provider_path("name"),
                value: toml_edit::value("azure"),
            },
            // Azure resource endpoint, e.g. https://my-resource.openai.azure.com
            ConfigEdit::SetPath {
                segments: azure_provider_path("base_url"),
                value: toml_edit::value(endpoint),
            },
            // Embed the API key directly – no env-var lookup needed at request time.
            ConfigEdit::SetPath {
                segments: azure_provider_path("experimental_bearer_token"),
                value: toml_edit::value(api_key),
            },
        ];

        // Only persist the api-version when the user provided a non-empty value.
        if let Some(version) = api_version.filter(|v| !v.trim().is_empty()) {
            let mut segments = azure_provider_path("query_params");
            segments.push("api-version".to_string());
            edits.push(ConfigEdit::SetPath {
                segments,
                value: toml_edit::value(version),
            });
        }
```

- [ ] **Step 2: Run tests to verify no regression**

Run: `cd codex-rs && just test -p codex-app-server`
Expected: PASS — this is a behavior-preserving change; any existing test asserting the exact `segments` produced by `login_azure_common` (search the crate's test suite for `login_azure_common` or `AzureLogin`/`azureOpenAi` if unsure it's covered) should still pass unchanged since the resulting `Vec<String>` values are identical.

- [ ] **Step 3: Format and commit**

```bash
cd codex-rs && just fmt
git add codex-rs/app-server/src/request_processors/account_processor.rs
git commit -m "refactor: dedupe azure config-path building in account_processor.rs"
```

---

### Task 6: Split the bundled multi-assertion test in `agent_worker_command.rs`

**Files:**
- Modify: `codex-rs/tui/src/agent_worker_command.rs:216-247`

**Interfaces:** none (test-only change).

- [ ] **Step 1: Replace the bundled test with one test per scenario**

Replace this single test (current lines 216-247):

```rust
    #[test]
    fn parse_worker_commands_require_task_text() {
        assert_eq!(
            parse_agent_worker_command("explore map the TUI"),
            Ok(AgentWorkerCommand::Spawn(AgentWorkerKind::Explore))
        );
        assert_eq!(
            parse_agent_worker_command("spawn researcher current RAG papers"),
            Ok(AgentWorkerCommand::Spawn(AgentWorkerKind::Research))
        );
        assert_eq!(
            parse_agent_worker_command("spawn researcher"),
            Err(AGENT_USAGE)
        );
        assert_eq!(
            parse_agent_worker_command("review current diff"),
            Ok(AgentWorkerCommand::Spawn(AgentWorkerKind::Review))
        );
        assert_eq!(
            parse_agent_worker_command("test failing codex-tui tests"),
            Ok(AgentWorkerCommand::Spawn(AgentWorkerKind::Test))
        );
        assert_eq!(
            parse_agent_worker_command("implement /agent workers"),
            Ok(AgentWorkerCommand::Spawn(AgentWorkerKind::Implement))
        );
        assert_eq!(
            parse_agent_worker_command("auto fix the failing parser test"),
            Ok(AgentWorkerCommand::Spawn(AgentWorkerKind::Auto))
        );
        assert_eq!(parse_agent_worker_command("explore"), Err(AGENT_USAGE));
    }
```

with:

```rust
    #[test]
    fn parse_explore_command_returns_explore_kind() {
        assert_eq!(
            parse_agent_worker_command("explore map the TUI"),
            Ok(AgentWorkerCommand::Spawn(AgentWorkerKind::Explore))
        );
    }

    #[test]
    fn parse_spawn_researcher_command_returns_research_kind() {
        assert_eq!(
            parse_agent_worker_command("spawn researcher current RAG papers"),
            Ok(AgentWorkerCommand::Spawn(AgentWorkerKind::Research))
        );
    }

    #[test]
    fn parse_spawn_researcher_without_topic_is_usage_error() {
        assert_eq!(
            parse_agent_worker_command("spawn researcher"),
            Err(AGENT_USAGE)
        );
    }

    #[test]
    fn parse_review_command_returns_review_kind() {
        assert_eq!(
            parse_agent_worker_command("review current diff"),
            Ok(AgentWorkerCommand::Spawn(AgentWorkerKind::Review))
        );
    }

    #[test]
    fn parse_test_command_returns_test_kind() {
        assert_eq!(
            parse_agent_worker_command("test failing codex-tui tests"),
            Ok(AgentWorkerCommand::Spawn(AgentWorkerKind::Test))
        );
    }

    #[test]
    fn parse_implement_command_returns_implement_kind() {
        assert_eq!(
            parse_agent_worker_command("implement /agent workers"),
            Ok(AgentWorkerCommand::Spawn(AgentWorkerKind::Implement))
        );
    }

    #[test]
    fn parse_auto_command_returns_auto_kind() {
        assert_eq!(
            parse_agent_worker_command("auto fix the failing parser test"),
            Ok(AgentWorkerCommand::Spawn(AgentWorkerKind::Auto))
        );
    }

    #[test]
    fn parse_explore_without_task_text_is_usage_error() {
        assert_eq!(parse_agent_worker_command("explore"), Err(AGENT_USAGE));
    }
```

- [ ] **Step 2: Run tests to verify everything passes**

Run: `cd codex-rs && just test -p codex-tui`
Expected: PASS — 8 named tests replace the 1 bundled test, same coverage, each independently diagnosable on failure.

- [ ] **Step 3: Format and commit**

```bash
cd codex-rs && just fmt
git add codex-rs/tui/src/agent_worker_command.rs
git commit -m "test: split bundled agent-worker-command parse test into one test per scenario"
```

---

### Task 7: Full local verification, then push branch and check GitHub Actions

**Files:** none (verification only).

- [ ] **Step 1: Run the full local check for every touched crate**

```bash
cd codex-rs
just fmt
just test -p codex-api
just test -p codex-model-provider
just test -p codex-core
just test -p codex-tui
just test -p codex-app-server
just test -p codex-cli
```

Expected: all PASS.

- [ ] **Step 2: Run lint on touched crates**

```bash
cd codex-rs
just fix -p codex-api
just fix -p codex-model-provider
just fix -p codex-core
just fix -p codex-tui
just fix -p codex-app-server
just fix -p codex-cli
```

Review any changes `just fix` makes before committing them (it can auto-apply clippy suggestions); if it changes anything, run the affected crate's tests again before committing.

- [ ] **Step 3: Push the branch to the user's own fork (never `main`) and verify CI**

Confirm the current branch is `azure-peer-provider-refactor` (not `main`) before pushing:

```bash
git branch --show-current
```

Then push only this branch:

```bash
git push -u origin azure-peer-provider-refactor
```

- [ ] **Step 4: Check the GitHub Actions run**

```bash
gh run list --branch azure-peer-provider-refactor --limit 5
```

If `gh` is not authenticated in this environment, report the branch URL to the user and ask them to check the Actions tab on `Bhaveshmeghwal21/codex-azure` directly. Do not consider this task done until the CI run for this branch is confirmed green (or the user has explicitly reviewed a failure and decided how to proceed).

- [ ] **Step 5: Report status**

Summarize for the user: which tasks landed, local test/lint results, and the CI run outcome (or a link/branch name if `gh` isn't available to check it directly). Do not merge to `main` — leave that decision to the user.

---

## Self-Review Notes

- **Spec coverage:** Idiom fixes (typed errors, enum instead of bool, dedup helper, split test) — Tasks 2, 4, 5, 6. Test coverage (Azure test module comparable to Bedrock) — Task 3 gives Azure wire-compat logic its own module + test file; Task 4 adds error-message tests; Task 1 adds dialect-detection tests. Architectural refactor (single resolved dialect instead of scattered string-matching) — Tasks 1 and 2. GitHub Actions verification per repo owner's request — Task 7. Branch isolation per repo owner's request — enforced in Global Constraints and Task 7 Step 3.
- **Scope adjustment from the design spec:** while pulling exact source during planning, `model-provider-info/src/lib.rs:433` and `model-provider/src/provider.rs:301` turned out to be simple, self-contained boolean OR-expressions for `remote_compaction` capability gating — not scattered re-derivation causing confusion. Converting them to compare against `ProviderDialect` would add a dependency and an indirection without improving readability (the rust-best-practices skill's own guidance: don't extract/introduce a type purely for symmetry). Left unchanged; flagged to the user in chat, not silently dropped.
- **Type consistency check:** `AuthHeaderStyle` (Task 2) is consumed correctly in Task 2's own `auth.rs`/`amazon_bedrock/auth.rs` edits — no other task references it. `ProviderDialect` (Task 1) is consumed by Task 2's `auth.rs` edit and Task 1's own `doctor.rs` edit — signatures match (`detect(name: &str, base_url: Option<&str>) -> ProviderDialect`). `AzureCommandError` (Task 4) is self-contained to Task 4 plus its one call site in `slash_dispatch.rs`. `azure_compat::model_input_for_provider` (Task 3) signature matches its only call site at `client.rs:938`.
- **No placeholders:** every step above contains literal code, not descriptions of code.
