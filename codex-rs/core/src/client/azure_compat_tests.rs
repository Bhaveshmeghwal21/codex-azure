use super::model_input_for_provider;
use super::tools_json_for_provider;
use super::tools_raw_json_for_provider;
use codex_model_provider_info::ModelProviderInfo;
use codex_protocol::ResponseItemId;
use codex_protocol::models::ContentItem;
use codex_protocol::models::FunctionCallOutputContentItem;
use codex_protocol::models::FunctionCallOutputPayload;
use codex_protocol::models::ReasoningItemReasoningSummary;
use codex_protocol::models::ResponseItem;
use codex_tools::JsonSchema;
use codex_tools::ResponsesApiNamespace;
use codex_tools::ResponsesApiNamespaceTool;
use codex_tools::ResponsesApiTool;
use codex_tools::ToolSpec;
use codex_tools::create_tools_json_for_responses_lite;
use pretty_assertions::assert_eq;
use serde_json::json;

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

    // Azure replay strips server-issued ids, removes encrypted reasoning content (and gains a
    // stub summary); compaction items are dropped; encrypted tool output is replaced with a
    // readable placeholder.
    assert_eq!(
        projected,
        vec![
            user_message(),
            ResponseItem::Reasoning {
                id: None,
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
            id: None,
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
fn azure_model_input_drops_server_item_ids() {
    let input = vec![
        ResponseItem::Reasoning {
            id: Some(ResponseItemId::from_server("rs_old".to_string())),
            summary: vec![ReasoningItemReasoningSummary::SummaryText {
                text: "summary".to_string(),
            }],
            content: None,
            encrypted_content: None,
            internal_chat_message_metadata_passthrough: None,
        },
        ResponseItem::Message {
            id: Some(ResponseItemId::from_server("msg_old".to_string())),
            role: "assistant".to_string(),
            content: vec![ContentItem::OutputText {
                text: "answer".to_string(),
            }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        },
    ];

    let projected = model_input_for_provider(&azure_api_provider(), input);

    assert_eq!(
        projected,
        vec![
            ResponseItem::Reasoning {
                id: None,
                summary: vec![ReasoningItemReasoningSummary::SummaryText {
                    text: "summary".to_string(),
                }],
                content: None,
                encrypted_content: None,
                internal_chat_message_metadata_passthrough: None,
            },
            ResponseItem::Message {
                id: None,
                role: "assistant".to_string(),
                content: vec![ContentItem::OutputText {
                    text: "answer".to_string(),
                }],
                phase: None,
                internal_chat_message_metadata_passthrough: None,
            },
        ]
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

fn function_tool(name: &str, description: &str) -> ResponsesApiTool {
    ResponsesApiTool {
        name: name.to_string(),
        description: description.to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::default(),
        output_schema: None,
    }
}

#[test]
fn azure_tools_json_replaces_the_empty_default_namespace_description() {
    // This is the exact payload that made every responses-lite turn fail with
    // "Invalid 'input[0].tools[0].description': empty string": the synthetic
    // `functions` namespace is described by an empty string.
    let tools = create_tools_json_for_responses_lite(&[ToolSpec::Function(function_tool(
        "shell",
        "Run a shell command.",
    ))])
    .expect("responses-lite tools should serialize");
    assert_eq!(tools[0]["description"], json!(""), "precondition");

    let tools = tools_json_for_provider(&azure_api_provider(), tools);

    assert_eq!(
        tools,
        vec![json!({
            "type": "namespace",
            "name": "functions",
            "description": "Tools in the functions namespace.",
            "tools": [{
                "type": "function",
                "name": "shell",
                "description": "Run a shell command.",
                "strict": false,
                "parameters": {},
            }],
        })]
    );
}

#[test]
fn azure_tools_json_replaces_empty_descriptions_on_nested_tools() {
    let tools = create_tools_json_for_responses_lite(&[ToolSpec::Function(function_tool(
        "undocumented-mcp-tool",
        "",
    ))])
    .expect("responses-lite tools should serialize");

    let tools = tools_json_for_provider(&azure_api_provider(), tools);

    assert_eq!(
        tools[0]["tools"][0]["description"],
        json!("The undocumented-mcp-tool tool.")
    );
}

#[test]
fn non_azure_tools_json_keeps_empty_descriptions() {
    let tools = create_tools_json_for_responses_lite(&[ToolSpec::Function(function_tool(
        "shell",
        "Run a shell command.",
    ))])
    .expect("responses-lite tools should serialize");

    assert_eq!(
        tools_json_for_provider(&openai_api_provider(), tools.clone()),
        tools
    );
}

#[test]
fn azure_tools_raw_json_replaces_empty_descriptions() {
    // The classic (non-lite) request shape sends tools at the top level, where
    // a namespace built by `default_namespace_description` hits the same wall.
    let tools = [ToolSpec::Namespace(ResponsesApiNamespace {
        name: "functions".to_string(),
        description: String::new(),
        tools: vec![ResponsesApiNamespaceTool::Function(function_tool(
            "shell", "",
        ))],
    })];

    let raw = tools_raw_json_for_provider(&azure_api_provider(), &tools)
        .expect("azure tools should serialize");
    let parsed: serde_json::Value =
        serde_json::from_str(raw.get()).expect("raw tools should be valid json");

    assert_eq!(
        parsed,
        json!([{
            "type": "namespace",
            "name": "functions",
            "description": "Tools in the functions namespace.",
            "tools": [{
                "type": "function",
                "name": "shell",
                "description": "The shell tool.",
                "strict": false,
                "parameters": {},
            }],
        }])
    );
}

#[test]
fn non_azure_tools_raw_json_keeps_empty_descriptions() {
    let tools = [ToolSpec::Function(function_tool("shell", ""))];

    let raw = tools_raw_json_for_provider(&openai_api_provider(), &tools)
        .expect("openai tools should serialize");
    let parsed: serde_json::Value =
        serde_json::from_str(raw.get()).expect("raw tools should be valid json");

    assert_eq!(parsed[0]["description"], json!(""));
}
