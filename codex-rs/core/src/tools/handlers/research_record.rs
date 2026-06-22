use crate::function_tool::FunctionCallError;
use crate::research::RESEARCH_RECORD_TOOL_NAME;
use crate::research::RESEARCH_RECORD_TOOL_NAMESPACE;
use crate::research::ResearchLedger;
use crate::research::ResearchRecord;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::context::boxed_tool_output;
use crate::tools::handlers::parse_arguments;
use crate::tools::handlers::research_record_spec::create_research_record_tool;
use crate::tools::registry::CoreToolRuntime;
use crate::tools::registry::ToolExecutor;
use codex_tools::ToolName;
use codex_tools::ToolSpec;
use serde::Serialize;
use std::sync::Mutex;
use std::sync::PoisonError;

pub(crate) struct ResearchRecordHandler;

impl ToolExecutor<ToolInvocation> for ResearchRecordHandler {
    fn tool_name(&self) -> ToolName {
        ToolName::namespaced(RESEARCH_RECORD_TOOL_NAMESPACE, RESEARCH_RECORD_TOOL_NAME)
    }

    fn spec(&self) -> ToolSpec {
        create_research_record_tool()
    }

    fn handle(&self, invocation: ToolInvocation) -> codex_tools::ToolExecutorFuture<'_> {
        Box::pin(async move {
            let ToolInvocation {
                session, payload, ..
            } = invocation;
            let ToolPayload::Function { arguments } = payload else {
                return Err(FunctionCallError::RespondToModel(
                    "research.record received unsupported payload".to_string(),
                ));
            };
            let record: ResearchRecord = parse_arguments(&arguments)?;
            let state = session
                .services
                .thread_extension_data
                .get_or_init(ResearchLedgerState::default);
            let result = state.record(record);
            let response = ResearchRecordResponse {
                status: "success",
                summary: format!(
                    "Recorded {:?}: {} distinct searches, {} candidate papers, {} opened sources.",
                    result.bucket,
                    result.distinct_searches,
                    result.candidate_papers,
                    result.opened_sources
                ),
                next_actions: vec![result.next_action.clone()],
                artifacts: vec!["thread research ledger".to_string()],
                result,
            };
            let content = serde_json::to_string(&response).map_err(|err| {
                FunctionCallError::RespondToModel(format!(
                    "failed to serialize research.record response: {err}"
                ))
            })?;
            Ok(boxed_tool_output(FunctionToolOutput::from_text(
                content,
                Some(true),
            )))
        })
    }
}

impl CoreToolRuntime for ResearchRecordHandler {}

#[derive(Default)]
struct ResearchLedgerState {
    ledger: Mutex<ResearchLedger>,
}

impl ResearchLedgerState {
    fn record(&self, record: ResearchRecord) -> crate::research::ResearchRecordResult {
        let mut ledger = self.ledger.lock().unwrap_or_else(PoisonError::into_inner);
        ledger.record(record)
    }
}

#[derive(Serialize)]
struct ResearchRecordResponse {
    status: &'static str,
    summary: String,
    next_actions: Vec<String>,
    artifacts: Vec<String>,
    result: crate::research::ResearchRecordResult,
}
