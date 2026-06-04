//! Parser and prompt templates for `/agent` worker commands.
//!
//! The TUI already has a subagent runtime and picker. This module keeps the
//! user-facing command family small and deterministic, then asks the active
//! agent to use the existing subagent tools instead of introducing a second
//! orchestration path.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentWorkerCommand {
    Picker,
    Help,
    Spawn(AgentWorkerKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentWorkerKind {
    General,
    Explore,
    Review,
    Test,
    Implement,
    Auto,
}

pub(crate) const AGENT_USAGE: &str =
    "Usage: /agent [list|spawn|explore|review|test|implement|auto] <task>";

pub(crate) fn parse_agent_worker_command(input: &str) -> Result<AgentWorkerCommand, &'static str> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(AgentWorkerCommand::Picker);
    }
    if trimmed.eq_ignore_ascii_case("list") || trimmed.eq_ignore_ascii_case("help") {
        return Ok(AgentWorkerCommand::Help);
    }

    let (verb, task) = split_once_whitespace(trimmed).ok_or(AGENT_USAGE)?;
    let task = task.trim();
    if task.is_empty() {
        return Err(AGENT_USAGE);
    }

    let kind = match verb.to_ascii_lowercase().as_str() {
        "spawn" => AgentWorkerKind::General,
        "explore" => AgentWorkerKind::Explore,
        "review" => AgentWorkerKind::Review,
        "test" => AgentWorkerKind::Test,
        "implement" => AgentWorkerKind::Implement,
        "auto" => AgentWorkerKind::Auto,
        _ => return Err(AGENT_USAGE),
    };
    Ok(AgentWorkerCommand::Spawn(kind))
}

pub(crate) fn build_agent_worker_prompt(kind: AgentWorkerKind, task: &str) -> String {
    let task = task.trim();
    let role = kind.role_name();
    let autonomy = kind.autonomy_policy();

    format!(
        "Start a bounded autonomous subagent worker for this task.\n\
\n\
Worker role: {role}\n\
Task: {task}\n\
\n\
Use the existing subagent tools to spawn exactly one worker unless the task clearly requires \
multiple independent workers. Give the worker the role-specific instructions below. Keep the \
main thread focused on coordination and final review.\n\
\n\
Role-specific instructions:\n\
{autonomy}\n\
\n\
Guardrails:\n\
- Inherit the current sandbox, approval, model, and provider settings.\n\
- Do not bypass approval prompts or sandbox restrictions.\n\
- Keep the worker task bounded; stop once it has a useful result or a clear blocker.\n\
- Return a compact summary with files inspected or changed, commands run, test results, and \
remaining risks.\n\
- If edits are made, leave final integration decisions to the main thread."
    )
}

impl AgentWorkerKind {
    fn role_name(self) -> &'static str {
        match self {
            Self::General => "general worker",
            Self::Explore => "read-only explorer",
            Self::Review => "read-only reviewer",
            Self::Test => "test diagnosis worker",
            Self::Implement => "implementation worker",
            Self::Auto => "autonomous implementation worker",
        }
    }

    fn autonomy_policy(self) -> &'static str {
        match self {
            Self::General => {
                "- Work autonomously on the delegated task.\n\
- Prefer reading, searching, and focused verification before proposing edits.\n\
- Ask the main thread for direction if the task scope is ambiguous or risky."
            }
            Self::Explore => {
                "- Read and search only; do not edit files.\n\
- Identify relevant files, functions, data flow, risks, and open questions.\n\
- Return concise findings and exact file paths for the main thread to inspect next."
            }
            Self::Review => {
                "- Review only; do not edit files.\n\
- Look for correctness bugs, regressions, missing tests, security issues, and unsafe assumptions.\n\
- Return findings ordered by severity with file/line references when available."
            }
            Self::Test => {
                "- Diagnose tests autonomously.\n\
- Run focused build, lint, or test commands when allowed by the current sandbox and approvals.\n\
- Do not edit files unless the main thread explicitly follows up asking for a fix.\n\
- Return the failing command, observed error, likely root cause, and recommended next change."
            }
            Self::Implement => {
                "- Implement the requested change in a focused way.\n\
- Run the smallest relevant verification commands after editing.\n\
- Stop for main-thread review after producing a patch and verification summary."
            }
            Self::Auto => {
                "- Run a full bounded loop: plan, edit, verify, summarize.\n\
- Keep edits limited to the requested task and existing codebase patterns.\n\
- Stop after one coherent implementation pass; do not keep retrying indefinitely."
            }
        }
    }
}

fn split_once_whitespace(input: &str) -> Option<(&str, &str)> {
    let idx = input.find(char::is_whitespace)?;
    Some(input.split_at(idx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn parse_empty_opens_picker_and_list_shows_help() {
        assert_eq!(
            parse_agent_worker_command(""),
            Ok(AgentWorkerCommand::Picker)
        );
        assert_eq!(
            parse_agent_worker_command(" list "),
            Ok(AgentWorkerCommand::Help)
        );
    }

    #[test]
    fn parse_worker_commands_require_task_text() {
        assert_eq!(
            parse_agent_worker_command("explore map the TUI"),
            Ok(AgentWorkerCommand::Spawn(AgentWorkerKind::Explore))
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

    #[test]
    fn build_read_only_review_prompt_uses_existing_subagent_tools() {
        let prompt = build_agent_worker_prompt(AgentWorkerKind::Review, "check Azure compact");

        assert!(prompt.contains("Start a bounded autonomous subagent worker"));
        assert!(prompt.contains("Worker role: read-only reviewer"));
        assert!(prompt.contains("Task: check Azure compact"));
        assert!(prompt.contains("Use the existing subagent tools"));
        assert!(prompt.contains("Review only; do not edit files."));
        assert!(prompt.contains("Do not bypass approval prompts or sandbox restrictions."));
    }
}
