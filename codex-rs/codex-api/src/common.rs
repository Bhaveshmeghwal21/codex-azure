use crate::error::ApiError;
use codex_protocol::config_types::ReasoningSummary as ReasoningSummaryConfig;
use codex_protocol::config_types::Verbosity as VerbosityConfig;
use codex_protocol::models::ResponseItem;
use codex_protocol::openai_models::ReasoningEffort as ReasoningEffortConfig;
use codex_protocol::protocol::ModelVerification;
use codex_protocol::protocol::RateLimitSnapshot;
use codex_protocol::protocol::TokenUsage;
use codex_protocol::protocol::TurnModerationMetadataEvent;
use codex_protocol::protocol::W3cTraceContext;
use futures::Stream;
use serde::Deserialize;
use serde::Serialize;
use serde::Serializer;
use serde::ser::SerializeStruct;
use serde_json::Value;
use std::collections::HashMap;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;
use tokio::sync::mpsc;

pub const WS_REQUEST_HEADER_TRACEPARENT_CLIENT_METADATA_KEY: &str = "ws_request_header_traceparent";
pub const WS_REQUEST_HEADER_TRACESTATE_CLIENT_METADATA_KEY: &str = "ws_request_header_tracestate";

/// Canonical input payload for the compaction endpoint.
#[derive(Debug, Clone)]
pub struct CompactionInput<'a> {
    pub model: &'a str,
    pub input: &'a [ResponseItem],
    pub instructions: &'a str,
    pub tools: Vec<Value>,
    pub parallel_tool_calls: bool,
    pub reasoning: Option<Reasoning>,
    pub service_tier: Option<&'a str>,
    pub prompt_cache_key: Option<&'a str>,
    pub text: Option<TextControls>,
    pub omit_null_encrypted_content: bool,
}

impl Serialize for CompactionInput<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut fields = 4;
        if !self.instructions.is_empty() {
            fields += 1;
        }
        if self.reasoning.is_some() {
            fields += 1;
        }
        if self.service_tier.is_some() {
            fields += 1;
        }
        if self.prompt_cache_key.is_some() {
            fields += 1;
        }
        if self.text.is_some() {
            fields += 1;
        }

        let mut state = serializer.serialize_struct("CompactionInput", fields)?;
        state.serialize_field("model", &self.model)?;
        state.serialize_field(
            "input",
            &serialize_input_items(self.input, self.omit_null_encrypted_content)
                .map_err(serde::ser::Error::custom)?,
        )?;
        if !self.instructions.is_empty() {
            state.serialize_field("instructions", &self.instructions)?;
        }
        state.serialize_field("tools", &self.tools)?;
        state.serialize_field("parallel_tool_calls", &self.parallel_tool_calls)?;
        if let Some(reasoning) = &self.reasoning {
            state.serialize_field("reasoning", reasoning)?;
        }
        if let Some(service_tier) = &self.service_tier {
            state.serialize_field("service_tier", service_tier)?;
        }
        if let Some(prompt_cache_key) = &self.prompt_cache_key {
            state.serialize_field("prompt_cache_key", prompt_cache_key)?;
        }
        if let Some(text) = &self.text {
            state.serialize_field("text", text)?;
        }
        state.end()
    }
}

/// Canonical input payload for the memory summarize endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct MemorySummarizeInput {
    pub model: String,
    #[serde(rename = "traces")]
    pub raw_memories: Vec<RawMemory>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Reasoning>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RawMemory {
    pub id: String,
    pub metadata: RawMemoryMetadata,
    pub items: Vec<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RawMemoryMetadata {
    pub source_path: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct MemorySummarizeOutput {
    #[serde(rename = "trace_summary", alias = "raw_memory")]
    pub raw_memory: String,
    pub memory_summary: String,
}

#[derive(Debug)]
pub enum ResponseEvent {
    Created,
    OutputItemDone(ResponseItem),
    OutputItemAdded(ResponseItem),
    /// Emitted when the server includes `OpenAI-Model` on the stream response.
    /// This can differ from the requested model when backend safety routing applies.
    ServerModel(String),
    /// Emitted when the server recommends additional account verification.
    ModelVerifications(Vec<ModelVerification>),
    /// Emitted when the server includes moderation metadata for first-party turn presentation.
    TurnModerationMetadata(TurnModerationMetadataEvent),
    /// Emitted when `X-Reasoning-Included: true` is present on the response,
    /// meaning the server already accounted for past reasoning tokens and the
    /// client should not re-estimate them.
    ServerReasoningIncluded(bool),
    Completed {
        response_id: String,
        token_usage: Option<TokenUsage>,
        /// Did the model affirmatively end its turn? Some providers do not set this,
        /// so we rely on fallback logic when this is `None`.
        end_turn: Option<bool>,
    },
    OutputTextDelta(String),
    ToolCallInputDelta {
        item_id: String,
        call_id: Option<String>,
        delta: String,
    },
    ReasoningSummaryDelta {
        delta: String,
        summary_index: i64,
    },
    ReasoningContentDelta {
        delta: String,
        content_index: i64,
    },
    ReasoningSummaryPartAdded {
        summary_index: i64,
    },
    RateLimits(RateLimitSnapshot),
    ModelsEtag(String),
}

#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningContext {
    Auto,
    CurrentTurn,
    AllTurns,
}

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct Reasoning {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<ReasoningEffortConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<ReasoningSummaryConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<ReasoningContext>,
}

#[derive(Debug, Serialize, Default, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TextFormatType {
    #[default]
    JsonSchema,
}

#[derive(Debug, Serialize, Default, Clone, PartialEq)]
pub struct TextFormat {
    /// Format type used by the OpenAI text controls.
    pub r#type: TextFormatType,
    /// When true, the server is expected to strictly validate responses.
    pub strict: bool,
    /// JSON schema for the desired output.
    pub schema: Value,
    /// Friendly name for the format, used in telemetry/debugging.
    pub name: String,
}

/// Controls the `text` field for the Responses API, combining verbosity and
/// optional JSON schema output formatting.
#[derive(Debug, Serialize, Default, Clone, PartialEq)]
pub struct TextControls {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verbosity: Option<OpenAiVerbosity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<TextFormat>,
}

#[derive(Debug, Serialize, Default, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum OpenAiVerbosity {
    Low,
    #[default]
    Medium,
    High,
}

impl From<VerbosityConfig> for OpenAiVerbosity {
    fn from(v: VerbosityConfig) -> Self {
        match v {
            VerbosityConfig::Low => OpenAiVerbosity::Low,
            VerbosityConfig::Medium => OpenAiVerbosity::Medium,
            VerbosityConfig::High => OpenAiVerbosity::High,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResponsesApiRequest {
    pub model: String,
    pub instructions: String,
    pub input: Vec<ResponseItem>,
    pub tools: Vec<serde_json::Value>,
    pub tool_choice: String,
    pub parallel_tool_calls: bool,
    pub reasoning: Option<Reasoning>,
    pub store: bool,
    pub stream: bool,
    pub include: Vec<String>,
    pub service_tier: Option<String>,
    pub prompt_cache_key: Option<String>,
    pub text: Option<TextControls>,
    pub client_metadata: Option<HashMap<String, String>>,
    pub omit_null_encrypted_content: bool,
}

impl Serialize for ResponsesApiRequest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut fields = 9;
        if !self.instructions.is_empty() {
            fields += 1;
        }
        if self.service_tier.is_some() {
            fields += 1;
        }
        if self.prompt_cache_key.is_some() {
            fields += 1;
        }
        if self.text.is_some() {
            fields += 1;
        }
        if self.client_metadata.is_some() {
            fields += 1;
        }

        let mut state = serializer.serialize_struct("ResponsesApiRequest", fields)?;
        state.serialize_field("model", &self.model)?;
        if !self.instructions.is_empty() {
            state.serialize_field("instructions", &self.instructions)?;
        }
        state.serialize_field(
            "input",
            &serialize_input_items(&self.input, self.omit_null_encrypted_content)
                .map_err(serde::ser::Error::custom)?,
        )?;
        state.serialize_field("tools", &self.tools)?;
        state.serialize_field("tool_choice", &self.tool_choice)?;
        state.serialize_field("parallel_tool_calls", &self.parallel_tool_calls)?;
        state.serialize_field("reasoning", &self.reasoning)?;
        state.serialize_field("store", &self.store)?;
        state.serialize_field("stream", &self.stream)?;
        state.serialize_field("include", &self.include)?;
        if let Some(service_tier) = &self.service_tier {
            state.serialize_field("service_tier", service_tier)?;
        }
        if let Some(prompt_cache_key) = &self.prompt_cache_key {
            state.serialize_field("prompt_cache_key", prompt_cache_key)?;
        }
        if let Some(text) = &self.text {
            state.serialize_field("text", text)?;
        }
        if let Some(client_metadata) = &self.client_metadata {
            state.serialize_field("client_metadata", client_metadata)?;
        }
        state.end()
    }
}

impl From<&ResponsesApiRequest> for ResponseCreateWsRequest {
    fn from(request: &ResponsesApiRequest) -> Self {
        Self {
            model: request.model.clone(),
            instructions: request.instructions.clone(),
            previous_response_id: None,
            input: request.input.clone(),
            tools: request.tools.clone(),
            tool_choice: request.tool_choice.clone(),
            parallel_tool_calls: request.parallel_tool_calls,
            reasoning: request.reasoning.clone(),
            store: request.store,
            stream: request.stream,
            include: request.include.clone(),
            service_tier: request.service_tier.clone(),
            prompt_cache_key: request.prompt_cache_key.clone(),
            text: request.text.clone(),
            generate: None,
            client_metadata: request.client_metadata.clone(),
            omit_null_encrypted_content: request.omit_null_encrypted_content,
        }
    }
}

#[derive(Debug)]
pub struct ResponseCreateWsRequest {
    pub model: String,
    pub instructions: String,
    pub previous_response_id: Option<String>,
    pub input: Vec<ResponseItem>,
    pub tools: Vec<Value>,
    pub tool_choice: String,
    pub parallel_tool_calls: bool,
    pub reasoning: Option<Reasoning>,
    pub store: bool,
    pub stream: bool,
    pub include: Vec<String>,
    pub service_tier: Option<String>,
    pub prompt_cache_key: Option<String>,
    pub text: Option<TextControls>,
    pub generate: Option<bool>,
    pub client_metadata: Option<HashMap<String, String>>,
    pub omit_null_encrypted_content: bool,
}

impl Serialize for ResponseCreateWsRequest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut fields = 9;
        if !self.instructions.is_empty() {
            fields += 1;
        }
        if self.previous_response_id.is_some() {
            fields += 1;
        }
        if self.service_tier.is_some() {
            fields += 1;
        }
        if self.prompt_cache_key.is_some() {
            fields += 1;
        }
        if self.text.is_some() {
            fields += 1;
        }
        if self.generate.is_some() {
            fields += 1;
        }
        if self.client_metadata.is_some() {
            fields += 1;
        }

        let mut state = serializer.serialize_struct("ResponseCreateWsRequest", fields)?;
        state.serialize_field("model", &self.model)?;
        if !self.instructions.is_empty() {
            state.serialize_field("instructions", &self.instructions)?;
        }
        if let Some(previous_response_id) = &self.previous_response_id {
            state.serialize_field("previous_response_id", previous_response_id)?;
        }
        state.serialize_field(
            "input",
            &serialize_input_items(&self.input, self.omit_null_encrypted_content)
                .map_err(serde::ser::Error::custom)?,
        )?;
        state.serialize_field("tools", &self.tools)?;
        state.serialize_field("tool_choice", &self.tool_choice)?;
        state.serialize_field("parallel_tool_calls", &self.parallel_tool_calls)?;
        state.serialize_field("reasoning", &self.reasoning)?;
        state.serialize_field("store", &self.store)?;
        state.serialize_field("stream", &self.stream)?;
        state.serialize_field("include", &self.include)?;
        if let Some(service_tier) = &self.service_tier {
            state.serialize_field("service_tier", service_tier)?;
        }
        if let Some(prompt_cache_key) = &self.prompt_cache_key {
            state.serialize_field("prompt_cache_key", prompt_cache_key)?;
        }
        if let Some(text) = &self.text {
            state.serialize_field("text", text)?;
        }
        if let Some(generate) = &self.generate {
            state.serialize_field("generate", generate)?;
        }
        if let Some(client_metadata) = &self.client_metadata {
            state.serialize_field("client_metadata", client_metadata)?;
        }
        state.end()
    }
}

fn serialize_input_items(
    input: &[ResponseItem],
    omit_null_encrypted_content: bool,
) -> serde_json::Result<Vec<Value>> {
    input
        .iter()
        .map(|item| {
            let mut value = serde_json::to_value(item)?;
            if omit_null_encrypted_content
                && let Some(object) = value.as_object_mut()
                && object.get("type").and_then(Value::as_str) == Some("reasoning")
            {
                if object.get("encrypted_content").is_some_and(Value::is_null) {
                    object.remove("encrypted_content");
                }
                if object.get("content").is_some_and(Value::is_null) {
                    object.remove("content");
                }
            }
            Ok(value)
        })
        .collect()
}

pub fn response_create_client_metadata(
    client_metadata: Option<HashMap<String, String>>,
    trace: Option<&W3cTraceContext>,
) -> Option<HashMap<String, String>> {
    let mut client_metadata = client_metadata.unwrap_or_default();

    if let Some(traceparent) = trace.and_then(|trace| trace.traceparent.as_deref()) {
        client_metadata.insert(
            WS_REQUEST_HEADER_TRACEPARENT_CLIENT_METADATA_KEY.to_string(),
            traceparent.to_string(),
        );
    }
    if let Some(tracestate) = trace.and_then(|trace| trace.tracestate.as_deref()) {
        client_metadata.insert(
            WS_REQUEST_HEADER_TRACESTATE_CLIENT_METADATA_KEY.to_string(),
            tracestate.to_string(),
        );
    }

    (!client_metadata.is_empty()).then_some(client_metadata)
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
#[allow(clippy::large_enum_variant)]
pub enum ResponsesWsRequest {
    #[serde(rename = "response.create")]
    ResponseCreate(ResponseCreateWsRequest),
}

pub fn create_text_param_for_request(
    verbosity: Option<VerbosityConfig>,
    output_schema: &Option<Value>,
    output_schema_strict: bool,
) -> Option<TextControls> {
    if verbosity.is_none() && output_schema.is_none() {
        return None;
    }

    Some(TextControls {
        verbosity: verbosity.map(std::convert::Into::into),
        format: output_schema.as_ref().map(|schema| TextFormat {
            r#type: TextFormatType::JsonSchema,
            strict: output_schema_strict,
            schema: schema.clone(),
            name: "codex_output_schema".to_string(),
        }),
    })
}

pub struct ResponseStream {
    pub rx_event: mpsc::Receiver<Result<ResponseEvent, ApiError>>,
    /// Server-assigned `x-request-id` response header, when present.
    pub upstream_request_id: Option<String>,
}

impl Stream for ResponseStream {
    type Item = Result<ResponseEvent, ApiError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx_event.poll_recv(cx)
    }
}
