use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

pub(crate) const RESEARCH_RECORD_TOOL_NAMESPACE: &str = "research";
pub(crate) const RESEARCH_RECORD_TOOL_NAME: &str = "record";
pub(crate) const RESEARCHER_ROLE_NAME: &str = "researcher";

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ResearchBucket {
    CurrentYear,
    PreviousYear,
    OlderWork,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ResearchNovelty {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ResearchBucketStatus {
    NotStarted,
    Searching,
    Saturated,
    Complete,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct ResearchPaper {
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) year: Option<i32>,
    #[serde(default)]
    pub(crate) url: Option<String>,
    #[serde(default)]
    pub(crate) doi: Option<String>,
    #[serde(default)]
    pub(crate) arxiv_id: Option<String>,
    #[serde(default)]
    pub(crate) relevance: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct ResearchRecord {
    pub(crate) bucket: ResearchBucket,
    pub(crate) query: String,
    #[serde(default)]
    pub(crate) papers: Vec<ResearchPaper>,
    #[serde(default)]
    pub(crate) opened_sources: Vec<String>,
    #[serde(default)]
    pub(crate) new_concepts: Vec<String>,
    #[serde(default)]
    pub(crate) duplicates_or_repeats: Vec<String>,
    pub(crate) novelty: ResearchNovelty,
    #[serde(default)]
    pub(crate) proposed_stop_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct ResearchRecordResult {
    pub(crate) bucket: ResearchBucket,
    pub(crate) bucket_status: ResearchBucketStatus,
    pub(crate) distinct_searches: usize,
    pub(crate) candidate_papers: usize,
    pub(crate) opened_sources: usize,
    pub(crate) new_unique_papers: usize,
    pub(crate) new_unique_concepts: usize,
    pub(crate) low_novelty_streak: usize,
    pub(crate) stop_reason: Option<String>,
    pub(crate) next_action: String,
}

#[derive(Debug, Default, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct ResearchLedger {
    buckets: BTreeMap<ResearchBucket, ResearchBucketState>,
}

impl ResearchLedger {
    pub(crate) fn record(&mut self, record: ResearchRecord) -> ResearchRecordResult {
        let bucket = record.bucket;
        let state = self.buckets.entry(bucket).or_default();
        state.status = ResearchBucketStatus::Searching;
        state.queries.insert(normalize_text(&record.query));
        state.opened_sources.extend(record.opened_sources);

        let mut new_unique_papers = 0;
        for paper in record.papers {
            if state.insert_paper(paper) {
                new_unique_papers += 1;
            }
        }
        let new_unique_concepts = record
            .new_concepts
            .into_iter()
            .filter(|concept| state.concepts.insert(normalize_text(concept)))
            .count();
        let proposed_stop_reason = concrete_stop_reason(record.proposed_stop_reason);
        state
            .duplicates_or_repeats
            .extend(record.duplicates_or_repeats);

        let has_new_material = new_unique_papers > 0 || new_unique_concepts > 0;
        if record.novelty == ResearchNovelty::Low && !has_new_material {
            state.low_novelty_streak = state.low_novelty_streak.saturating_add(1);
        } else {
            state.low_novelty_streak = 0;
        }

        if can_saturate(bucket, state, new_unique_papers, new_unique_concepts)
            && let Some(reason) = proposed_stop_reason
        {
            state.status = ResearchBucketStatus::Saturated;
            state.stop_reason = Some(reason);
        }

        ResearchRecordResult {
            bucket,
            bucket_status: state.status,
            distinct_searches: state.queries.len(),
            candidate_papers: state.papers.len(),
            opened_sources: state.opened_sources.len(),
            new_unique_papers,
            new_unique_concepts,
            low_novelty_streak: state.low_novelty_streak,
            stop_reason: state.stop_reason.clone(),
            next_action: next_action(bucket, state),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct ResearchBucketState {
    queries: BTreeSet<String>,
    papers: Vec<ResearchPaper>,
    paper_identities: BTreeSet<String>,
    opened_sources: BTreeSet<String>,
    concepts: BTreeSet<String>,
    duplicates_or_repeats: Vec<String>,
    low_novelty_streak: usize,
    status: ResearchBucketStatus,
    stop_reason: Option<String>,
}

impl Default for ResearchBucketState {
    fn default() -> Self {
        Self {
            queries: BTreeSet::new(),
            papers: Vec::new(),
            paper_identities: BTreeSet::new(),
            opened_sources: BTreeSet::new(),
            concepts: BTreeSet::new(),
            duplicates_or_repeats: Vec::new(),
            low_novelty_streak: 0,
            status: ResearchBucketStatus::NotStarted,
            stop_reason: None,
        }
    }
}

impl ResearchBucketState {
    fn insert_paper(&mut self, paper: ResearchPaper) -> bool {
        let identities = paper_identities(&paper);
        if identities.is_empty() {
            return false;
        };
        if identities
            .iter()
            .any(|identity| self.paper_identities.contains(identity))
        {
            return false;
        }
        self.paper_identities.extend(identities);
        self.papers.push(paper);
        true
    }
}

fn concrete_stop_reason(proposed_stop_reason: Option<String>) -> Option<String> {
    proposed_stop_reason
        .map(|reason| reason.trim().to_string())
        .filter(|reason| !reason.is_empty())
}

fn can_saturate(
    bucket: ResearchBucket,
    state: &ResearchBucketState,
    new_unique_papers: usize,
    new_unique_concepts: usize,
) -> bool {
    state.queries.len() >= minimum_search_floor(bucket)
        && state.low_novelty_streak >= 2
        && new_unique_papers == 0
        && new_unique_concepts == 0
}

fn minimum_search_floor(bucket: ResearchBucket) -> usize {
    match bucket {
        ResearchBucket::CurrentYear | ResearchBucket::PreviousYear | ResearchBucket::OlderWork => 2,
    }
}

fn next_action(bucket: ResearchBucket, state: &ResearchBucketState) -> String {
    if state.status == ResearchBucketStatus::Saturated {
        return "move to the next year bucket or synthesize if all buckets are saturated"
            .to_string();
    }
    let remaining = minimum_search_floor(bucket).saturating_sub(state.queries.len());
    if remaining > 0 {
        return format!(
            "continue this bucket; {remaining} distinct search(es) remain before saturation can be considered"
        );
    }
    if state.low_novelty_streak < 2 {
        return "continue this bucket until two consecutive low-novelty records establish saturation"
            .to_string();
    }
    "provide a concrete repeated-result stop reason or continue searching".to_string()
}

fn paper_identities(paper: &ResearchPaper) -> BTreeSet<String> {
    let mut identities = BTreeSet::new();
    if let Some(doi) = paper.doi.as_deref().map(normalize_identifier)
        && !doi.is_empty()
    {
        identities.insert(format!("doi:{doi}"));
    }
    if let Some(arxiv_id) = paper.arxiv_id.as_deref().map(normalize_identifier)
        && !arxiv_id.is_empty()
    {
        identities.insert(format!("arxiv:{arxiv_id}"));
    }
    let title = normalize_text(&paper.title);
    if !title.is_empty() {
        identities.insert(format!("title:{title}"));
    }
    identities
}

fn normalize_identifier(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn normalize_text(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn is_researcher_role(role: Option<&str>) -> bool {
    role.is_some_and(|role| role.eq_ignore_ascii_case(RESEARCHER_ROLE_NAME))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn paper(title: &str) -> ResearchPaper {
        ResearchPaper {
            title: title.to_string(),
            year: Some(2026),
            url: None,
            doi: None,
            arxiv_id: None,
            relevance: Some("relevant".to_string()),
        }
    }

    fn low_record(query: &str, title: &str) -> ResearchRecord {
        ResearchRecord {
            bucket: ResearchBucket::CurrentYear,
            query: query.to_string(),
            papers: vec![paper(title)],
            opened_sources: vec!["https://arxiv.org/abs/2601.00001".to_string()],
            new_concepts: Vec::new(),
            duplicates_or_repeats: vec![title.to_string()],
            novelty: ResearchNovelty::Low,
            proposed_stop_reason: Some("repeated papers and no new concepts".to_string()),
        }
    }

    #[test]
    fn current_year_bucket_saturates_after_floor_and_two_low_novelty_records() {
        let mut ledger = ResearchLedger::default();

        assert_eq!(
            ledger
                .record(low_record("2026 agent papers", "Paper One"))
                .bucket_status,
            ResearchBucketStatus::Searching
        );
        assert_eq!(
            ledger
                .record(low_record("2026 agent papers follow up", "Paper One"))
                .bucket_status,
            ResearchBucketStatus::Searching
        );
        assert_eq!(
            ledger
                .record(low_record("2026 agent papers final", "Paper One"))
                .bucket_status,
            ResearchBucketStatus::Saturated
        );
    }

    #[test]
    fn repeated_title_does_not_count_as_new_material() {
        let mut ledger = ResearchLedger::default();
        let first = ResearchRecord {
            novelty: ResearchNovelty::High,
            new_concepts: vec!["new benchmark".to_string()],
            proposed_stop_reason: None,
            ..low_record("2026 agent papers", "Same Paper")
        };
        let duplicate = ResearchRecord {
            novelty: ResearchNovelty::High,
            new_concepts: vec!["new benchmark".to_string()],
            proposed_stop_reason: None,
            ..low_record("2026 agent papers duplicate", "same paper")
        };

        let first_result = ledger.record(first);
        let duplicate_result = ledger.record(duplicate);

        assert_eq!(first_result.new_unique_papers, 1);
        assert_eq!(duplicate_result.new_unique_papers, 0);
        assert_eq!(
            duplicate_result.bucket_status,
            ResearchBucketStatus::Searching
        );
    }

    #[test]
    fn previous_year_bucket_does_not_saturate_before_search_floor() {
        let mut ledger = ResearchLedger::default();
        let record = ResearchRecord {
            bucket: ResearchBucket::PreviousYear,
            ..low_record("2025 agent papers", "Paper One")
        };

        let result = ledger.record(record);

        assert_eq!(result.bucket_status, ResearchBucketStatus::Searching);
    }

    #[test]
    fn low_novelty_record_with_new_paper_keeps_bucket_open() {
        let mut ledger = ResearchLedger::default();

        ledger.record(low_record("2026 agent papers", "Paper One"));
        let result = ledger.record(low_record("2026 agent papers follow up", "Paper Two"));

        assert_eq!(result.new_unique_papers, 1);
        assert_eq!(result.bucket_status, ResearchBucketStatus::Searching);
    }

    #[test]
    fn paper_with_missing_doi_deduplicates_by_title() {
        let mut ledger = ResearchLedger::default();
        let with_doi = ResearchRecord {
            papers: vec![ResearchPaper {
                doi: Some("10.1145/example".to_string()),
                ..paper("Same Paper")
            }],
            novelty: ResearchNovelty::High,
            proposed_stop_reason: None,
            ..low_record("2026 agent papers", "Same Paper")
        };
        let without_doi = ResearchRecord {
            novelty: ResearchNovelty::High,
            proposed_stop_reason: None,
            ..low_record("2026 agent papers duplicate", "same paper")
        };

        let first_result = ledger.record(with_doi);
        let duplicate_result = ledger.record(without_doi);

        assert_eq!(first_result.new_unique_papers, 1);
        assert_eq!(duplicate_result.new_unique_papers, 0);
    }

    #[test]
    fn low_novelty_with_new_material_does_not_advance_saturation_streak() {
        let mut ledger = ResearchLedger::default();
        let novel_low = ResearchRecord {
            papers: vec![paper("Paper One")],
            new_concepts: vec!["new benchmark".to_string()],
            novelty: ResearchNovelty::Low,
            ..low_record("2026 agent papers", "Paper One")
        };
        let duplicate_low = low_record("2026 agent papers follow up", "Paper One");

        let first_result = ledger.record(novel_low);
        let duplicate_result = ledger.record(duplicate_low);

        assert_eq!(first_result.low_novelty_streak, 0);
        assert_eq!(duplicate_result.low_novelty_streak, 1);
        assert_eq!(
            duplicate_result.bucket_status,
            ResearchBucketStatus::Searching
        );
    }
}
