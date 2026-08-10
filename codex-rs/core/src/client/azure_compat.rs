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
