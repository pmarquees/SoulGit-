//! SoulGit's refs-only proposal model.
//!
//! Proposal state is encoded in ordinary Git refs so it is committed by the
//! existing Walgit manifest CAS, replicated by the WAL, and reconstructable
//! without a database or object materialisation.

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

pub const PROPOSAL_PREFIX: &str = "refs/soulgit/proposals/";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProposalState {
    #[default]
    Open,
    Reviewing,
    ChangesRequested,
    Approved,
    Merging,
    Merged,
    Rejected,
    Superseded,
    Expired,
}

impl ProposalState {
    pub const ALL: [Self; 9] = [
        Self::Open,
        Self::Reviewing,
        Self::ChangesRequested,
        Self::Approved,
        Self::Merging,
        Self::Merged,
        Self::Rejected,
        Self::Superseded,
        Self::Expired,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Reviewing => "reviewing",
            Self::ChangesRequested => "changes-requested",
            Self::Approved => "approved",
            Self::Merging => "merging",
            Self::Merged => "merged",
            Self::Rejected => "rejected",
            Self::Superseded => "superseded",
            Self::Expired => "expired",
        }
    }
}

impl fmt::Display for ProposalState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ProposalState {
    type Err = ParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|state| state.as_str() == value)
            .ok_or_else(|| ParseError::UnknownState(value.to_string()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewDecision {
    Approved,
    ChangesRequested,
}

impl ReviewDecision {
    pub const ALL: [Self; 2] = [Self::Approved, Self::ChangesRequested];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Approved => "approved",
            Self::ChangesRequested => "changes-requested",
        }
    }
}

impl fmt::Display for ReviewDecision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ReviewDecision {
    type Err = ParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|decision| decision.as_str() == value)
            .ok_or_else(|| ParseError::UnknownReview(value.to_string()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CheckResult {
    Pending,
    Passed,
    Failed,
    Skipped,
}

impl CheckResult {
    pub const ALL: [Self; 4] = [Self::Pending, Self::Passed, Self::Failed, Self::Skipped];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }
}

impl fmt::Display for CheckResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for CheckResult {
    type Err = ParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|result| result.as_str() == value)
            .ok_or_else(|| ParseError::UnknownCheck(value.to_string()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Review {
    pub reviewer: String,
    pub decision: ReviewDecision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Check {
    pub actor: String,
    pub name: String,
    pub result: CheckResult,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MergeStrategy {
    #[default]
    FastForward,
    MergeCommit,
    Squash,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MergeRules {
    pub min_approvals: usize,
    pub required_checks: Vec<String>,
    pub allow_author_approval: bool,
    pub strategy: MergeStrategy,
}

impl Default for MergeRules {
    fn default() -> Self {
        Self {
            min_approvals: 1,
            required_checks: Vec::new(),
            allow_author_approval: false,
            strategy: MergeStrategy::FastForward,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AgentGrant {
    pub principal: String,
    pub checks: Vec<String>,
    pub review: bool,
    pub merge: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SoulGitConfig {
    pub version: u32,
    pub merge: MergeRules,
    pub agents: Vec<AgentGrant>,
}

impl Default for SoulGitConfig {
    fn default() -> Self {
        Self {
            version: 1,
            merge: MergeRules::default(),
            agents: Vec::new(),
        }
    }
}

impl SoulGitConfig {
    pub fn parse(text: &str) -> Result<Self, String> {
        let config: Self = toml::from_str(text).map_err(|e| e.to_string())?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err(format!("unsupported SoulGit config version {}", self.version));
        }
        if self.merge.min_approvals > 100 {
            return Err("merge.min_approvals must be at most 100".into());
        }
        let mut principals = std::collections::HashSet::new();
        for agent in &self.agents {
            validate_actor(&agent.principal).map_err(|e| e.to_string())?;
            if !principals.insert(agent.principal.as_str()) {
                return Err(format!("duplicate agent principal `{}`", agent.principal));
            }
            for check in &agent.checks {
                validate_actor(check).map_err(|_| format!("invalid check name `{check}`"))?;
            }
        }
        for check in &self.merge.required_checks {
            validate_actor(check).map_err(|_| format!("invalid required check `{check}`"))?;
        }
        Ok(())
    }

    pub fn agent(&self, principal: &str) -> Option<&AgentGrant> {
        self.agents.iter().find(|agent| agent.principal == principal)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Readiness {
    pub ready: bool,
    pub approvals: usize,
    pub approvals_required: usize,
    pub missing_checks: Vec<String>,
    pub failed_checks: Vec<String>,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposal {
    pub id: String,
    pub head: String,
    pub target: String,
    pub author: String,
    pub state: ProposalState,
    pub reviews: Vec<Review>,
    pub checks: Vec<Check>,
    pub merge_oid: Option<String>,
    pub healthy: bool,
    pub issues: Vec<String>,
}

impl Proposal {
    pub fn approvals(&self) -> usize {
        self.reviews
            .iter()
            .filter(|review| review.decision == ReviewDecision::Approved)
            .count()
    }

    pub fn checks_passed(&self) -> usize {
        self.checks
            .iter()
            .filter(|check| check.result == CheckResult::Passed)
            .count()
    }

    pub fn readiness(&self, config: &SoulGitConfig) -> Readiness {
        let approvals = self
            .reviews
            .iter()
            .filter(|review| {
                review.decision == ReviewDecision::Approved
                    && (config.merge.allow_author_approval || review.reviewer != self.author)
                    && config
                        .agent(&review.reviewer)
                        .is_none_or(|agent| agent.review)
            })
            .count();
        let mut missing_checks = Vec::new();
        let mut failed_checks = Vec::new();
        for required in &config.merge.required_checks {
            let matching: Vec<&Check> = self
                .checks
                .iter()
                .filter(|check| {
                    check.name == *required
                        && config.agent(&check.actor).is_some_and(|agent| {
                            agent.checks.iter().any(|name| name == required)
                        })
                })
                .collect();
            if matching.is_empty()
                || matching
                    .iter()
                    .all(|check| check.result != CheckResult::Passed)
            {
                missing_checks.push(required.clone());
            }
            if matching
                .iter()
                .any(|check| check.result == CheckResult::Failed)
            {
                failed_checks.push(required.clone());
            }
        }
        let mut blockers = self.issues.clone();
        for review in &self.reviews {
            if review.decision == ReviewDecision::ChangesRequested
                && config
                    .agent(&review.reviewer)
                    .is_none_or(|agent| agent.review)
            {
                blockers.push(format!("changes requested by {}", review.reviewer));
            }
        }
        if approvals < config.merge.min_approvals {
            blockers.push(format!(
                "{} more approval(s) required",
                config.merge.min_approvals - approvals
            ));
        }
        if !missing_checks.is_empty() {
            blockers.push(format!("missing checks: {}", missing_checks.join(", ")));
        }
        if !failed_checks.is_empty() {
            blockers.push(format!("failed checks: {}", failed_checks.join(", ")));
        }
        let ready = blockers.is_empty()
            && !matches!(
                self.state,
                ProposalState::Merged
                    | ProposalState::Rejected
                    | ProposalState::Superseded
                    | ProposalState::Expired
            );
        Readiness {
            ready,
            approvals,
            approvals_required: config.merge.min_approvals,
            missing_checks,
            failed_checks,
            blockers,
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("invalid proposal id `{0}`")]
    InvalidId(String),
    #[error("invalid actor `{0}`")]
    InvalidActor(String),
    #[error("invalid target branch `{0}`")]
    InvalidTarget(String),
    #[error("unknown proposal state `{0}`")]
    UnknownState(String),
    #[error("unknown review decision `{0}`")]
    UnknownReview(String),
    #[error("unknown check result `{0}`")]
    UnknownCheck(String),
}

pub fn validate_id(value: &str) -> Result<(), ParseError> {
    if value.is_empty()
        || value.len() > 64
        || value.starts_with('.')
        || value.ends_with('.')
        || value.contains("..")
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    {
        return Err(ParseError::InvalidId(value.to_string()));
    }
    Ok(())
}

pub fn validate_actor(value: &str) -> Result<(), ParseError> {
    if value.is_empty()
        || value.len() > 128
        || value.starts_with('.')
        || value.ends_with('.')
        || value.contains("..")
        || !value.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'@' | b'+')
        })
    {
        return Err(ParseError::InvalidActor(value.to_string()));
    }
    Ok(())
}

pub fn validate_target(value: &str) -> Result<(), ParseError> {
    if value.is_empty()
        || value.len() > 255
        || value.starts_with('/')
        || value.ends_with('/')
        || value.ends_with('.')
        || value.ends_with(".lock")
        || value.contains("..")
        || value.contains("//")
        || value.bytes().any(|b| {
            b.is_ascii_control()
                || b == b' '
                || matches!(b, b'~' | b'^' | b':' | b'?' | b'*' | b'[' | b'\\')
        })
    {
        return Err(ParseError::InvalidTarget(value.to_string()));
    }
    Ok(())
}

pub fn head_ref(id: &str) -> Result<String, ParseError> {
    validate_id(id)?;
    Ok(format!("{PROPOSAL_PREFIX}{id}/head"))
}

pub fn target_ref(id: &str, target: &str) -> Result<String, ParseError> {
    validate_id(id)?;
    validate_target(target)?;
    Ok(format!("{PROPOSAL_PREFIX}{id}/target/{target}"))
}

pub fn author_ref(id: &str, author: &str) -> Result<String, ParseError> {
    validate_id(id)?;
    validate_actor(author)?;
    Ok(format!("{PROPOSAL_PREFIX}{id}/author/{author}"))
}

pub fn state_ref(id: &str, state: ProposalState) -> Result<String, ParseError> {
    validate_id(id)?;
    Ok(format!("{PROPOSAL_PREFIX}{id}/state/{state}"))
}

pub fn review_ref(
    id: &str,
    reviewer: &str,
    decision: ReviewDecision,
) -> Result<String, ParseError> {
    validate_id(id)?;
    validate_actor(reviewer)?;
    Ok(format!(
        "{PROPOSAL_PREFIX}{id}/reviews/{reviewer}/{decision}"
    ))
}

pub fn check_ref(
    id: &str,
    actor: &str,
    name: &str,
    result: CheckResult,
) -> Result<String, ParseError> {
    validate_id(id)?;
    validate_actor(actor)?;
    validate_actor(name)?;
    Ok(format!(
        "{PROPOSAL_PREFIX}{id}/checks/{actor}/{name}/{result}"
    ))
}

pub fn result_ref(id: &str) -> Result<String, ParseError> {
    validate_id(id)?;
    Ok(format!("{PROPOSAL_PREFIX}{id}/result"))
}

#[derive(Default)]
struct Builder {
    head: Option<String>,
    targets: Vec<(String, String)>,
    authors: Vec<(String, String)>,
    states: Vec<(ProposalState, String)>,
    reviews: Vec<(Review, String)>,
    checks: Vec<(Check, String)>,
    results: Vec<String>,
}

/// Project proposal summaries from `(full ref name, oid)` pairs.
///
/// Metadata refs only count when they point at the current proposal head. This
/// makes reviews and checks revision-safe: updating `head` automatically makes
/// attestations for the previous revision stale without deleting history.
pub fn project<'a>(refs: impl IntoIterator<Item = (&'a str, &'a str)>) -> Vec<Proposal> {
    let mut builders: BTreeMap<String, Builder> = BTreeMap::new();
    for (name, oid) in refs {
        let Some(rest) = name.strip_prefix(PROPOSAL_PREFIX) else {
            continue;
        };
        let Some((id, tail)) = rest.split_once('/') else {
            continue;
        };
        if validate_id(id).is_err() {
            continue;
        }
        let b = builders.entry(id.to_string()).or_default();
        if tail == "head" {
            b.head = Some(oid.to_string());
        } else if let Some(target) = tail.strip_prefix("target/") {
            if validate_target(target).is_ok() {
                b.targets.push((target.to_string(), oid.to_string()));
            }
        } else if let Some(author) = tail.strip_prefix("author/") {
            if validate_actor(author).is_ok() {
                b.authors.push((author.to_string(), oid.to_string()));
            }
        } else if let Some(state) = tail.strip_prefix("state/") {
            if let Ok(state) = state.parse() {
                b.states.push((state, oid.to_string()));
            }
        } else if let Some(review) = tail.strip_prefix("reviews/") {
            if let Some((reviewer, decision)) = review.split_once('/') {
                if validate_actor(reviewer).is_ok() {
                    if let Ok(decision) = decision.parse() {
                        b.reviews.push((
                            Review {
                                reviewer: reviewer.to_string(),
                                decision,
                            },
                            oid.to_string(),
                        ));
                    }
                }
            }
        } else if let Some(check) = tail.strip_prefix("checks/") {
            let parts: Vec<&str> = check.split('/').collect();
            if let [actor, name, result] = parts.as_slice() {
                if validate_actor(actor).is_ok() && validate_actor(name).is_ok() {
                    if let Ok(result) = result.parse() {
                        b.checks.push((
                            Check {
                                actor: (*actor).to_string(),
                                name: (*name).to_string(),
                                result,
                            },
                            oid.to_string(),
                        ));
                    }
                }
            }
        } else if tail == "result" {
            b.results.push(oid.to_string());
        }
    }

    builders
        .into_iter()
        .filter_map(|(id, mut b)| {
            let head = b.head?;
            b.targets.retain(|(_, oid)| oid == &head);
            b.authors.retain(|(_, oid)| oid == &head);
            b.states.retain(|(_, oid)| oid == &head);
            b.reviews.retain(|(_, oid)| oid == &head);
            b.checks.retain(|(_, oid)| oid == &head);

            b.targets.sort_by(|a, b| a.0.cmp(&b.0));
            b.authors.sort_by(|a, b| a.0.cmp(&b.0));
            b.states.sort_by_key(|(state, _)| state.as_str());
            b.reviews.sort_by(|a, b| {
                a.0.reviewer
                    .cmp(&b.0.reviewer)
                    .then_with(|| a.0.decision.as_str().cmp(b.0.decision.as_str()))
            });
            b.checks.sort_by(|a, b| {
                a.0.actor
                    .cmp(&b.0.actor)
                    .then_with(|| a.0.name.cmp(&b.0.name))
                    .then_with(|| a.0.result.as_str().cmp(b.0.result.as_str()))
            });

            let mut issues = Vec::new();
            if b.targets.len() > 1 {
                issues.push("multiple current target refs".to_string());
            }
            if b.authors.len() > 1 {
                issues.push("multiple current author refs".to_string());
            }
            if b.states.len() > 1 {
                issues.push("multiple current state refs".to_string());
            }
            for pair in b.reviews.windows(2) {
                if pair[0].0.reviewer == pair[1].0.reviewer {
                    issues.push(format!(
                        "multiple current reviews from {}",
                        pair[0].0.reviewer
                    ));
                }
            }
            for pair in b.checks.windows(2) {
                if pair[0].0.actor == pair[1].0.actor
                    && pair[0].0.name == pair[1].0.name
                {
                    issues.push(format!(
                        "multiple current {} checks from {}",
                        pair[0].0.name, pair[0].0.actor
                    ));
                }
            }
            if b.results.len() > 1 {
                issues.push("multiple merge result refs".to_string());
            }

            let target = b
                .targets
                .first()
                .map(|(target, _)| target.clone())
                .unwrap_or_else(|| "main".to_string());
            let author = b
                .authors
                .first()
                .map(|(author, _)| author.clone())
                .unwrap_or_else(|| "unknown".to_string());
            let state = b
                .states
                .first()
                .map(|(state, _)| *state)
                .unwrap_or_default();
            let reviews = b.reviews.into_iter().map(|(review, _)| review).collect();
            let checks = b.checks.into_iter().map(|(check, _)| check).collect();
            let merge_oid = (state == ProposalState::Merged)
                .then(|| b.results.into_iter().next())
                .flatten();

            Some(Proposal {
                id,
                head,
                target,
                author,
                state,
                reviews,
                checks,
                merge_oid,
                healthy: issues.is_empty(),
                issues,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn projects_current_revision_and_ignores_stale_attestations() {
        let refs = [
            ("refs/heads/main", A),
            ("refs/soulgit/proposals/p-1/head", B),
            ("refs/soulgit/proposals/p-1/target/main", B),
            ("refs/soulgit/proposals/p-1/author/alice@example.com", B),
            ("refs/soulgit/proposals/p-1/state/reviewing", B),
            ("refs/soulgit/proposals/p-1/reviews/bob@example.com/approved", B),
            ("refs/soulgit/proposals/p-1/checks/ci@example.com/tests/passed", B),
            ("refs/soulgit/proposals/p-1/checks/ci@example.com/security/failed", A),
        ];
        let proposals = project(refs);
        assert_eq!(proposals.len(), 1);
        let p = &proposals[0];
        assert_eq!(p.id, "p-1");
        assert_eq!(p.head, B);
        assert_eq!(p.target, "main");
        assert_eq!(p.author, "alice@example.com");
        assert_eq!(p.state, ProposalState::Reviewing);
        assert_eq!(p.approvals(), 1);
        assert_eq!(p.checks_passed(), 1);
        assert!(p.healthy);
        assert!(p.readiness(&SoulGitConfig::default()).ready);
    }

    #[test]
    fn detects_conflicting_current_metadata() {
        let refs = [
            ("refs/soulgit/proposals/p-2/head", A),
            ("refs/soulgit/proposals/p-2/state/open", A),
            ("refs/soulgit/proposals/p-2/state/approved", A),
        ];
        let p = project(refs).pop().unwrap();
        assert!(!p.healthy);
        assert_eq!(p.issues, ["multiple current state refs"]);
    }

    #[test]
    fn validates_ref_components() {
        assert!(head_ref("update-deps").is_ok());
        assert!(head_ref("../escape").is_err());
        assert!(target_ref("p", "feature/agent-hooks").is_ok());
        assert!(target_ref("p", "bad..branch").is_err());
        assert!(author_ref("p", "agent+review@example.com").is_ok());
        assert!(author_ref("p", "team/agent").is_err());
    }

    #[test]
    fn readiness_requires_configured_checks_and_non_author_approval() {
        let p = Proposal {
            id: "p".into(),
            head: A.into(),
            target: "main".into(),
            author: "alice@example.com".into(),
            state: ProposalState::Reviewing,
            reviews: vec![
                Review {
                    reviewer: "alice@example.com".into(),
                    decision: ReviewDecision::Approved,
                },
                Review {
                    reviewer: "bob@example.com".into(),
                    decision: ReviewDecision::Approved,
                },
            ],
            checks: vec![Check {
                actor: "ci@example.com".into(),
                name: "tests".into(),
                result: CheckResult::Passed,
            }],
            merge_oid: None,
            healthy: true,
            issues: Vec::new(),
        };
        let cfg = SoulGitConfig {
            merge: MergeRules {
                min_approvals: 1,
                required_checks: vec!["tests".into()],
                ..Default::default()
            },
            agents: vec![AgentGrant {
                principal: "ci@example.com".into(),
                checks: vec!["tests".into()],
                ..Default::default()
            }],
            ..Default::default()
        };
        let readiness = p.readiness(&cfg);
        assert!(readiness.ready);
        assert_eq!(readiness.approvals, 1);
    }

    #[test]
    fn parses_versioned_rules_and_scoped_agent_grants() {
        let cfg = SoulGitConfig::parse(
            r#"
                version = 1
                [merge]
                min_approvals = 2
                required_checks = ["tests"]
                strategy = "squash"
                [[agents]]
                principal = "svc-ci"
                checks = ["tests"]
            "#,
        )
        .unwrap();
        assert_eq!(cfg.merge.min_approvals, 2);
        assert_eq!(cfg.merge.strategy, MergeStrategy::Squash);
        assert_eq!(cfg.agent("svc-ci").unwrap().checks, ["tests".to_string()]);
    }
}
