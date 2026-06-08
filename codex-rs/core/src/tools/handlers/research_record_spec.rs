use crate::research::RESEARCH_RECORD_TOOL_NAME;
use crate::research::RESEARCH_RECORD_TOOL_NAMESPACE;
use codex_tools::JsonSchema;
use codex_tools::ResponsesApiNamespace;
use codex_tools::ResponsesApiNamespaceTool;
use codex_tools::ResponsesApiTool;
use codex_tools::ToolSpec;
use serde_json::json;
use std::collections::BTreeMap;

pub(crate) fn create_research_record_tool() -> ToolSpec {
    let paper_properties = BTreeMap::from([
        (
            "title".to_string(),
            JsonSchema::string(Some("Paper title.".to_string())),
        ),
        (
            "year".to_string(),
            JsonSchema::integer(Some("Publication year when known.".to_string())),
        ),
        (
            "url".to_string(),
            JsonSchema::string(Some(
                "URL inspected or discovered for the paper.".to_string(),
            )),
        ),
        (
            "doi".to_string(),
            JsonSchema::string(Some("DOI when available.".to_string())),
        ),
        (
            "arxiv_id".to_string(),
            JsonSchema::string(Some("arXiv identifier when available.".to_string())),
        ),
        (
            "relevance".to_string(),
            JsonSchema::string(Some("Short relevance note.".to_string())),
        ),
    ]);
    let paper_schema = JsonSchema::object(
        paper_properties,
        Some(vec!["title".to_string()]),
        Some(false.into()),
    );
    let string_array = |description: &str| {
        JsonSchema::array(
            JsonSchema::string(/*description*/ None),
            Some(description.to_string()),
        )
    };

    let properties = BTreeMap::from([
        (
            "bucket".to_string(),
            JsonSchema::string_enum(
                vec![
                    json!("current_year"),
                    json!("previous_year"),
                    json!("older_work"),
                ],
                Some("Year bucket being recorded.".to_string()),
            ),
        ),
        (
            "query".to_string(),
            JsonSchema::string(Some("Search query or paper-inspection action.".to_string())),
        ),
        (
            "papers".to_string(),
            JsonSchema::array(
                paper_schema,
                Some("Normalized candidate papers found in this step.".to_string()),
            ),
        ),
        (
            "opened_sources".to_string(),
            string_array("Sources actually opened or inspected."),
        ),
        (
            "new_concepts".to_string(),
            string_array("New methods, datasets, benchmarks, metrics, or claims."),
        ),
        (
            "duplicates_or_repeats".to_string(),
            string_array("Repeated papers or repeated concepts."),
        ),
        (
            "novelty".to_string(),
            JsonSchema::string_enum(
                vec![json!("high"), json!("medium"), json!("low")],
                Some("Novelty of this record relative to the ledger.".to_string()),
            ),
        ),
        (
            "proposed_stop_reason".to_string(),
            JsonSchema::string(Some(
                "Concrete stop reason when this bucket appears saturated.".to_string(),
            )),
        ),
    ]);

    ToolSpec::Namespace(ResponsesApiNamespace {
        name: RESEARCH_RECORD_TOOL_NAMESPACE.to_string(),
        description: "Tools for researcher agents.".to_string(),
        tools: vec![ResponsesApiNamespaceTool::Function(ResponsesApiTool {
            name: RESEARCH_RECORD_TOOL_NAME.to_string(),
            description: "Record a structured researcher ledger update after each search or paper-inspection step. Use only for the active researcher role; do not use it to synthesize final answers."
                .to_string(),
            strict: false,
            defer_loading: None,
            parameters: JsonSchema::object(
                properties,
                Some(vec![
                    "bucket".to_string(),
                    "query".to_string(),
                    "papers".to_string(),
                    "opened_sources".to_string(),
                    "new_concepts".to_string(),
                    "duplicates_or_repeats".to_string(),
                    "novelty".to_string(),
                ]),
                Some(false.into()),
            ),
            output_schema: None,
        })],
    })
}
