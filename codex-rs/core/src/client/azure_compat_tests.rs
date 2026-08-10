use super::model_input_for_provider;
use codex_model_provider_info::ModelProviderInfo;
use codex_protocol::models::ContentItem;
use codex_protocol::models::FunctionCallOutputContentItem;
use codex_protocol::models::FunctionCallOutputPayload;
use codex_protocol::models::ReasoningItemReasoningSummary;
use codex_protocol::ResponseItemId;
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
        id: Some(ResponseItemId::from_server("rs_1".to_string())),
        summary: vec![],
        content: None,
        encrypted_content: Some("stale-reasoning".to_string()),
        internal_chat_message_metadata_passthrough: None,
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
