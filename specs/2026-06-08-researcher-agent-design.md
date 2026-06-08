# Researcher Agent Design

## Summary

Add a code-enforced `researcher` agent role for literature research. The role should not stop after a few web searches. It should search by year bucket, track what it has covered, and stop only when recent searches show saturation: repeated papers, no new relevant approaches, no new datasets, no new benchmarks, and no new material claims.

The first implementation should focus on enforcing research discipline around the agent's research process. It must not assume Azure OpenAI provides native hosted `web_search`. Search must be reached through an explicit provider boundary: existing hosted `web_search` when the configured provider supports it, standalone `web.run` when available, or a custom search function backed by Bing Search API, an academic API, or another configured backend.

## Goals

- Support `/agent spawn researcher <topic>` as the main interface.
- Make the researcher search current-year papers first, previous-year papers next, and older high-relevance work last.
- Track a research ledger for each year bucket.
- Prevent or steer premature final answers when coverage is incomplete.
- Require the final answer to include coverage, key findings, limitations, and stop reasons.
- Require structured ledger updates after each research search so enforcement is not based only on free-form prose.
- Make search availability explicit for Azure deployments.

## Non-Goals

- Do not add a new `/research` slash command in the first version.
- Do not assume standalone `web.run` or hosted `web_search` works on Azure without explicit configuration.
- Do not build a full academic database index.
- Do not enforce fixed paper quotas as the main stopping rule.

## Agent Role

The built-in `researcher` role should define instructions that require:

- Search planning before synthesis.
- Year-bucketed literature review.
- Preference for primary sources such as papers, preprints, official proceedings, DOI pages, arXiv pages, and publisher pages.
- Deduplication by title, DOI, arXiv ID, or clear title match.
- A visible ledger in the final answer.
- Explicit limitations and uncertainty.

The role may still use normal Codex tools, but research enforcement should only activate when the active agent role is `researcher`.

## Search Provider Boundary

Research mode needs a concrete search provider before it can run. The implementation should resolve one of these paths at turn start:

- Hosted `web_search`, only when provider capabilities say it is available.
- Standalone `web.run`, only when the extension is installed and configured for the active provider or for a separate OpenAI-backed search provider.
- A custom search tool, for Azure-first setups, backed by Bing Search API, Semantic Scholar, arXiv, Crossref, OpenAlex, or another configured backend.

If no search provider is available, the researcher agent should fail early with a clear setup message instead of pretending it searched.

This is the first blocker to solve for Azure: decide whether the fork will use Azure model calls plus a separate search backend, or whether Azure has already been wired to an external web-search service.

## Ledger Update Tool

Add a researcher-only function tool named `research.record`, exposed through Codex's normal tool plumbing, for structured ledger updates after each search or paper-inspection step.

The tool input should include:

- `bucket`: current year, previous year, or older work.
- `query`: the search query or inspection action.
- `papers`: normalized candidate papers with title, year, URL or DOI/arXiv ID when available, and relevance notes.
- `opened_sources`: sources the agent actually inspected.
- `new_concepts`: newly discovered methods, datasets, benchmarks, metrics, or claims.
- `duplicates_or_repeats`: repeated papers or repeated concepts.
- `novelty`: high, medium, or low.
- `proposed_stop_reason`: optional, only when the agent believes the bucket is saturated.

Native hosted `web_search` events are useful for confirming that searches happened, but they are not enough by themselves because they may not expose all result metadata. The guard should enforce against structured `research.record` calls first and use search events as supporting evidence.

## Research Ledger

The implementation should maintain structured state for each research task:

- Topic and normalized search focus.
- Current year, previous year, and older-work buckets.
- Queries run per bucket.
- Candidate papers found per bucket.
- Sources opened or inspected.
- New concepts found, such as method names, datasets, benchmarks, architectures, metrics, or key claims.
- Duplicate or repeated hits.
- Bucket status: not started, searching, saturated, or complete.
- Stop reason for each completed bucket.

The ledger is internal state first. The final answer should render a concise version for the user.

## Ledger Persistence

The ledger should live in Codex-managed state, not only in the model's context. The first version should persist it as JSON under the active thread/session state so it survives multi-turn agent work and can be compacted into context when needed.

The context sent back to the model should contain a compact ledger summary, not the full raw ledger. Full paper lists and repeated result details should stay in persisted state and only be summarized into the prompt.

## Saturation Rule

A bucket should not close just because a fixed count was reached. It should close when all conditions are true:

- The minimum floor has been satisfied so the agent cannot stop after one weak search.
- At least two consecutive structured ledger updates for that bucket report low novelty.
- The proposed stop reason is concrete and references the repeated or exhausted result pattern.

Low novelty means most results are duplicates, irrelevant, or already represented, and no materially new papers, approaches, datasets, benchmarks, or claims appear.

Novelty must not rely only on the model's self-report. The guard should compute basic novelty signals from structured records:

- exact or normalized-title duplicate detection
- DOI/arXiv ID duplicate detection
- repeated method, dataset, and benchmark names
- source-opened coverage versus search-only candidates

Embedding-based similarity for titles and abstracts is a later improvement. The first version should at least prevent obvious duplicate papers from counting as new novelty.

The first-version minimum floor should be conservative:

- Current year: at least two distinct searches before saturation can close the bucket.
- Previous year: at least two distinct searches before saturation can close the bucket.
- Older work: at least one broad search and one targeted follow-up unless the first search already returns only clearly duplicated foundational work.

These are floors, not quotas. If new relevant material keeps appearing, the bucket must remain open.

## Guard Location And Behavior

The research guard should live in Codex's outer orchestration layer, not in the model prompt. It should observe tool calls and model messages for the active researcher thread, update persisted ledger state from `research.record`, and decide whether the turn may finish.

It should also compare ledger updates with search activity when the active search provider emits observable events. If the assistant searches without recording findings, or attempts a final answer before required buckets are complete, the guard should steer the agent back to research instead of accepting a weak conclusion.

The steering message should be direct and machine-actionable, for example:

```text
Research coverage is incomplete. Continue searching the 2026 bucket. Recent ledger state: 2 searches, 5 candidate papers, no stop reason.
```

The guard should avoid blocking intermediate summaries, planning notes, or requests for clarification. It should only block or steer final synthesis when the task is still incomplete.

## Year Buckets

The default buckets should be based on the current calendar year:

- Current year: deep search first.
- Previous year: deep search second.
- Older work: high-relevance papers, foundational methods, and frequently cited or benchmark-defining work.

For 2026, that means `2026`, then `2025`, then older work.

## Final Answer Requirements

The researcher final answer should include:

- Year-by-year coverage summary.
- Key papers grouped by theme or approach.
- What changed over time.
- Strongest current conclusions.
- Open gaps and limitations.
- Stop reason for each bucket.

If coverage was impossible because search results were unavailable, blocked, or too sparse, the final answer must say that explicitly.

## Implementation Approach

Recommended first phase:

- Add or ship a built-in `researcher` agent role.
- Add research state types in a focused module instead of expanding large orchestration files.
- Add the `research.record` tool and hook research tracking into that structured tool.
- Add a search-provider resolution step and fail early when no search path is available.
- Persist the ledger in Codex-managed thread/session state and feed compact summaries back to the model.
- Cross-check existing web-search item/event handling where practical.
- Add guard logic that only applies to `researcher` role turns.
- Keep the first version independent of Azure-native hosted web search assumptions.

Later phases can add:

- Academic-specific search providers.
- Better paper identity extraction.
- Configurable saturation thresholds.
- Embedding-based semantic deduplication.
- UI rendering for the live ledger.

## Testing

Add focused tests for:

- Saturation decisions from repeated result sets.
- Continued searching when new papers or concepts appear.
- Requiring `research.record` after search activity.
- Premature final answer steering.
- Successful final answer when all buckets have stop reasons.
- No guard activation for non-researcher agents.
- Clear failure when no configured search provider exists.

Use mocked search events, mocked `research.record` calls, and mocked model outputs so the tests are deterministic.
