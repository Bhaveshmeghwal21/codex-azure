//! Wire-format adjustments needed when talking to Azure OpenAI's Responses
//! API deployment, as opposed to OpenAI's own. Kept separate from
//! `client.rs` so the Azure-specific surface area is easy to find, review,
//! and test in isolation.

use codex_protocol::models::FunctionCallOutputContentItem;
use codex_protocol::models::ReasoningItemReasoningSummary;
use codex_protocol::models::ResponseItem;
use codex_tools::ToolSpec;
use codex_tools::create_tools_json_for_responses_api;
use codex_tools::create_tools_raw_json_for_responses_api;
use serde_json::Map;
use serde_json::Value;
use serde_json::value::RawValue;
use std::sync::Arc;

const AZURE_ENCRYPTED_TOOL_OUTPUT_UNAVAILABLE: &str =
    "[encrypted tool output unavailable for Azure provider]";

/// Used when a tool carries an empty description and has no name to fall back
/// on. Azure only requires a non-empty string; the text itself is incidental.
const MISSING_TOOL_DESCRIPTION: &str = "No description provided.";

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

/// Tool list for the responses-lite request shape, where tools ride along in an
/// `AdditionalTools` input item rather than the top-level `tools` field.
pub(crate) fn tools_json_for_provider(
    provider: &codex_api::Provider,
    mut tools: Vec<Value>,
) -> Vec<Value> {
    if provider.is_azure_responses_endpoint() {
        fill_empty_tool_descriptions(&mut tools);
    }
    tools
}

/// Tool list for the classic request shape, which sends tools as raw JSON in
/// the top-level `tools` field. Non-Azure providers keep the zero-copy path.
pub(crate) fn tools_raw_json_for_provider(
    provider: &codex_api::Provider,
    tools: &[ToolSpec],
) -> Result<Arc<RawValue>, serde_json::Error> {
    if !provider.is_azure_responses_endpoint() {
        return create_tools_raw_json_for_responses_api(tools);
    }
    let mut tools_json = create_tools_json_for_responses_api(tools)?;
    fill_empty_tool_descriptions(&mut tools_json);
    serde_json::value::to_raw_value(&tools_json).map(Arc::from)
}

/// Azure rejects `""` for a tool description with
/// `invalid_request_error` / `empty_string` ("Expected a string with minimum
/// length 1"), while OpenAI accepts it. Codex emits exactly that for the
/// default `functions` namespace -- `default_namespace_description` returns an
/// empty string for it on purpose -- so without this every responses-lite turn
/// against Azure fails on `input[0].tools[0].description` before the model is
/// ever reached. MCP servers that ship tools without a description hit the same
/// wall. Substitute a placeholder rather than dropping the key, since the field
/// is required.
///
/// Only tool objects and their nested `tools` arrays are visited; parameter
/// schemas are left untouched so a tool's declared contract is not rewritten.
fn fill_empty_tool_descriptions(tools: &mut [Value]) {
    for tool in tools {
        let Some(tool) = tool.as_object_mut() else {
            continue;
        };
        if tool
            .get("description")
            .and_then(Value::as_str)
            .is_some_and(str::is_empty)
        {
            let description = fallback_tool_description(tool);
            tool.insert("description".to_string(), Value::String(description));
        }
        if let Some(nested) = tool.get_mut("tools").and_then(Value::as_array_mut) {
            fill_empty_tool_descriptions(nested);
        }
    }
}

fn fallback_tool_description(tool: &Map<String, Value>) -> String {
    let Some(name) = tool.get("name").and_then(Value::as_str) else {
        return MISSING_TOOL_DESCRIPTION.to_string();
    };
    match tool.get("type").and_then(Value::as_str) {
        // Matches what `default_namespace_description` produces for every
        // namespace other than the default one.
        Some("namespace") => format!("Tools in the {name} namespace."),
        _ => format!("The {name} tool."),
    }
}

fn azure_compatible_input_item(mut item: ResponseItem) -> Option<ResponseItem> {
    // Azure deployments do not share stored response-item state across model or deployment
    // changes. Replaying a server-issued id therefore makes Azure try to resolve an item that
    // does not exist in the active deployment, producing "Item with id 'rs_...' not found".
    // Keep the item contents, but make the replay self-contained.
    item.set_id(/*new_id*/ None);

    match &mut item {
        ResponseItem::Reasoning {
            id: _,
            summary,
            content,
            encrypted_content,
            internal_chat_message_metadata_passthrough: _,
        } => {
            *encrypted_content = None;
            // Keep a reasoning item whenever its paired message item appears in the input.
            // Azure rejects a paired message without its reasoning item, so an empty historical
            // reasoning item gets a minimal placeholder summary instead of being dropped.
            if summary.is_empty() && content.as_ref().is_none_or(Vec::is_empty) {
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
