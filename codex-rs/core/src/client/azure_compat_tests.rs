use super::model_input_for_provider;
use codex_model_provider_info::ModelProviderInfo;
use codex_protocol::ResponseItemId;
use codex_protocol::models::ContentItem;
use codex_protocol::models::FunctionCallOutputContentItem;
use codex_protocol::models::FunctionCallOutputPayload;
use codex_protocol::models::ReasoningItemReasoningSummary;
use codex_protocol::models::ResponseItem;
use pretty_assertions::assert_eq;

const AZURE_PLACEHOLDER: &str = "[encrypted tool output unavailable for Azure provider]";

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

fn user_message() -> ResponseItem {
    ResponseItem::Message {
        id: None,
        role: "user".to_string(),
        content: vec![ContentItem::InputText {
            text: "hello".to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    }
}

fn function_call() -> ResponseItem {
    ResponseItem::FunctionCall {
        id: None,
        name: "web.run".to_string(),
        namespace: None,
        arguments: "{}".to_string(),
        encrypted_function_args: None,
        call_id: "call_1".to_string(),
        internal_chat_message_metadata_passthrough: None,
    }
}

fn custom_tool_call() -> ResponseItem {
    ResponseItem::CustomToolCall {
        id: None,
        status: None,
        call_id: "custom_call_1".to_string(),
        name: "custom-tool".to_string(),
        namespace: None,
        input: "{}".to_string(),
        internal_chat_message_metadata_passthrough: None,
    }
}

fn function_call_output(content: FunctionCallOutputContentItem) -> ResponseItem {
    ResponseItem::FunctionCallOutput {
        id: None,
        call_id: "call_1".to_string(),
        output: FunctionCallOutputPayload::from_content_items(vec![content]),
        internal_chat_message_metadata_passthrough: None,
    }
}

fn custom_tool_call_output(content: FunctionCallOutputContentItem) -> ResponseItem {
    ResponseItem::CustomToolCallOutput {
        id: None,
        call_id: "custom_call_1".to_string(),
        name: Some("custom-tool".to_string()),
        output: FunctionCallOutputPayload::from_content_items(vec![content]),
        internal_chat_message_metadata_passthrough: None,
    }
}

fn encrypted(content: &str) -> FunctionCallOutputContentItem {
    FunctionCallOutputContentItem::EncryptedContent {
        encrypted_content: content.to_string(),
    }
}

fn placeholder_text() -> FunctionCallOutputContentItem {
    FunctionCallOutputContentItem::InputText {
        text: AZURE_PLACEHOLDER.to_string(),
    }
}

#[test]
fn azure_model_input_omits_replayed_encrypted_content_without_mutating_history() {
    let reasoning = ResponseItem::Reasoning {
        id: Some(ResponseItemId::from_server("rs_1".to_string())),
        summary: vec![],
        content: None,
        encrypted_content: Some("stale-reasoning".to_string()),
        internal_chat_message_metadata_passthrough: None,
    };
    let compacted_summary = ResponseItem::Compaction {
        id: None,
        encrypted_content: "stale-compaction".to_string(),
        internal_chat_message_metadata_passthrough: None,
    };
    let input = vec![
        user_message(),
        reasoning,
        compacted_summary,
        function_call(),
        function_call_output(encrypted("stale-tool-output")),
        custom_tool_call(),
        custom_tool_call_output(encrypted("stale-custom-output")),
    ];
    let original = input.clone();

    let projected = model_input_for_provider(&azure_api_provider(), input.clone());

    // Reasoning keeps its id but loses encrypted_content (and gains a stub
    // summary); compaction items are dropped; encrypted tool output is
    // replaced with a readable placeholder.
    assert_eq!(
        projected,
        vec![
            user_message(),
            ResponseItem::Reasoning {
                id: Some(ResponseItemId::from_server("rs_1".to_string())),
                summary: vec![ReasoningItemReasoningSummary::SummaryText {
                    text: String::new(),
                }],
                content: None,
                encrypted_content: None,
                internal_chat_message_metadata_passthrough: None,
            },
            function_call(),
            function_call_output(placeholder_text()),
            custom_tool_call(),
            custom_tool_call_output(placeholder_text()),
        ]
    );
    assert_eq!(input, original, "caller history must not be mutated");
}

#[test]
fn azure_model_input_preserves_reasoning_summary_without_encrypted_content() {
    let input = vec![ResponseItem::Reasoning {
        id: Some(ResponseItemId::from_server("rs_1".to_string())),
        summary: vec![ReasoningItemReasoningSummary::SummaryText {
            text: "readable summary".to_string(),
        }],
        content: None,
        encrypted_content: Some("stale-reasoning".to_string()),
        internal_chat_message_metadata_passthrough: None,
    }];

    let projected = model_input_for_provider(&azure_api_provider(), input);

    assert_eq!(
        projected,
        vec![ResponseItem::Reasoning {
            id: Some(ResponseItemId::from_server("rs_1".to_string())),
            summary: vec![ReasoningItemReasoningSummary::SummaryText {
                text: "readable summary".to_string(),
            }],
            content: None,
            encrypted_content: None,
            internal_chat_message_metadata_passthrough: None,
        }]
    );
}

#[test]
fn non_azure_model_input_preserves_encrypted_content() {
    let input = vec![ResponseItem::Reasoning {
        id: Some(ResponseItemId::from_server("rs_1".to_string())),
        summary: vec![],
        content: None,
        encrypted_content: Some("provider-owned-state".to_string()),
        internal_chat_message_metadata_passthrough: None,
    }];

    assert_eq!(
        model_input_for_provider(&openai_api_provider(), input.clone()),
        input
    );
}
