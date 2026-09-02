//! What a run produces. The report *is* the result: no rule panics, and `run_all` returns no
//! `Result`.

use crate::error::{Error, ErrorType};
use crate::query::Key;

use super::{Capability, KeyRequest, SafetyLevel, StoreCapabilities};

/// What happened to one rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "outcome")]
pub enum RuleOutcome {
    /// The store agrees with the contract.
    Passed,
    /// The store disagrees with the contract. This is the only outcome that means a defect.
    Failed { detail: String },
    /// The store does not claim the capability this rule needs, so the rule was not called.
    SkippedCapability { missing: Capability },
    /// The fixture could not supply the keys this rule needs, and said why.
    SkippedPrecondition { request: KeyRequest, reason: String },
    /// The rule needs a higher safety level than this run permits. **Not a pass.**
    NotRunSafetyLevel { required: SafetyLevel },
    /// A known divergence, tracked by an issue. The rule stays in the suite as a named expected
    /// failure rather than being deleted.
    Blocked { issue: String, detail: String },
    /// The store returned an error the rule could not interpret.
    Errored {
        error_type: ErrorType,
        message: String,
    },
}

impl RuleOutcome {
    /// Whether this outcome means the store is wrong.
    pub fn is_defect(&self) -> bool {
        match self {
            RuleOutcome::Failed { .. } | RuleOutcome::Errored { .. } => true,
            RuleOutcome::Passed
            | RuleOutcome::SkippedCapability { .. }
            | RuleOutcome::SkippedPrecondition { .. }
            | RuleOutcome::NotRunSafetyLevel { .. }
            | RuleOutcome::Blocked { .. } => false,
        }
    }
}

/// One rule's line in the report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportEntry {
    pub id: String,
    pub title: String,
    pub contract: String,
    /// The keys this rule was looking at. Without these a failure says *what* disagreed but not
    /// *where*, and rules may not put the key in the message themselves.
    pub subject: Vec<Key>,
    pub outcome: RuleOutcome,
}

/// A rule a given store is permitted to fail, and the issue that permits it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AllowedFailure {
    pub rule: &'static str,
    pub issue: &'static str,
}

/// A tally of outcomes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutcomeCounts {
    pub passed: usize,
    pub failed: usize,
    pub errored: usize,
    pub skipped_capability: usize,
    pub skipped_precondition: usize,
    pub not_run_level: usize,
    pub blocked: usize,
}

impl OutcomeCounts {
    /// Rules that actually executed — the only number a "conformant" claim may be read against.
    pub fn ran(&self) -> usize {
        self.passed + self.failed + self.errored + self.blocked
    }
}

/// The result of running the suite against one store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceReport {
    pub store: String,
    pub capabilities: StoreCapabilities,
    pub level: SafetyLevel,
    pub entries: Vec<ReportEntry>,
    /// Keys the run created, whether or not they survived.
    pub created: Vec<Key>,
    /// Keys still present after `cleanup` — what this run left in the store.
    ///
    /// At [`SafetyLevel::CreateOnly`] this is everything created, by definition: that level cannot
    /// remove anything. A caller that does not surface it is running a slow leak with no record.
    pub residue: Vec<Key>,
}

impl ConformanceReport {
    pub fn counts(&self) -> OutcomeCounts {
        let mut c = OutcomeCounts::default();
        for entry in &self.entries {
            match &entry.outcome {
                RuleOutcome::Passed => c.passed += 1,
                RuleOutcome::Failed { .. } => c.failed += 1,
                RuleOutcome::Errored { .. } => c.errored += 1,
                RuleOutcome::SkippedCapability { .. } => c.skipped_capability += 1,
                RuleOutcome::SkippedPrecondition { .. } => c.skipped_precondition += 1,
                RuleOutcome::NotRunSafetyLevel { .. } => c.not_run_level += 1,
                RuleOutcome::Blocked { .. } => c.blocked += 1,
            }
        }
        c
    }

    pub fn failures(&self) -> impl Iterator<Item = &ReportEntry> {
        self.entries.iter().filter(|e| e.outcome.is_defect())
    }

    /// How many rules did not run at this level, and which level would run them.
    ///
    /// Printed rather than kept internal: a clean run at [`SafetyLevel::ReadOnly`] exercises well
    /// under a third of the suite and misses every rule this suite was built for, so a bare
    /// "conformant" would be misleading.
    pub fn not_run_by_level(&self) -> Vec<(SafetyLevel, usize)> {
        let mut out: Vec<(SafetyLevel, usize)> = Vec::new();
        for entry in &self.entries {
            if let RuleOutcome::NotRunSafetyLevel { required } = &entry.outcome {
                match out.iter_mut().find(|(l, _)| l == required) {
                    Some((_, n)) => *n += 1,
                    None => out.push((*required, 1)),
                }
            }
        }
        out.sort_by_key(|(l, _)| *l);
        out
    }

    /// The assertion a suite makes.
    ///
    /// Fails in **both** directions: a rule that failed and is not in `allowed` is an error, and a
    /// rule in `allowed` that *passed* is also an error, naming the entry to delete. Without the
    /// second half an ignore list written for a good reason outlives the reason, which is the same
    /// staleness this whole suite exists to prevent.
    pub fn assert_conformant(&self, allowed: &[AllowedFailure]) -> Result<(), Error> {
        let mut problems: Vec<String> = Vec::new();

        for entry in self.failures() {
            if !allowed.iter().any(|a| a.rule == entry.id) {
                problems.push(format!(
                    "{} failed ({}): {}",
                    entry.id,
                    entry.contract,
                    match &entry.outcome {
                        RuleOutcome::Failed { detail } => detail.clone(),
                        RuleOutcome::Errored {
                            error_type,
                            message,
                        } => format!("{error_type:?}: {message}"),
                        RuleOutcome::Passed
                        | RuleOutcome::SkippedCapability { .. }
                        | RuleOutcome::SkippedPrecondition { .. }
                        | RuleOutcome::NotRunSafetyLevel { .. }
                        | RuleOutcome::Blocked { .. } => String::new(),
                    }
                ));
            }
        }

        for a in allowed {
            match self.entries.iter().find(|e| e.id == a.rule) {
                Some(entry) if !entry.outcome.is_defect() => problems.push(format!(
                    "{} is listed as an allowed failure citing {}, but it passed — remove the entry",
                    a.rule, a.issue
                )),
                Some(_) => {}
                None => problems.push(format!(
                    "{} is listed as an allowed failure but no such rule exists",
                    a.rule
                )),
            }
        }

        if problems.is_empty() {
            Ok(())
        } else {
            Err(Error::general_error(format!(
                "{} is not conformant:\n  {}",
                self.store,
                problems.join("\n  ")
            )))
        }
    }
}

impl std::fmt::Display for ConformanceReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let c = self.counts();
        writeln!(
            f,
            "{} · {:?} · {}/{} rules run",
            self.store,
            self.level,
            c.ran(),
            self.entries.len()
        )?;
        writeln!(
            f,
            "  passed {} · failed {} · errored {} · blocked {}",
            c.passed, c.failed, c.errored, c.blocked
        )?;
        for entry in &self.entries {
            match &entry.outcome {
                RuleOutcome::Passed => {}
                RuleOutcome::Failed { detail } => {
                    writeln!(f, "  FAILED {} [{}] {}", entry.id, entry.contract, detail)?;
                    if !entry.subject.is_empty() {
                        writeln!(f, "         keys: {:?}", entry.subject)?;
                    }
                }
                RuleOutcome::Errored {
                    error_type,
                    message,
                } => writeln!(f, "  ERROR  {} {:?}: {}", entry.id, error_type, message)?,
                RuleOutcome::Blocked { issue, detail } => {
                    writeln!(f, "  BLOCKED {} ({}): {}", entry.id, issue, detail)?
                }
                RuleOutcome::SkippedPrecondition { request, reason } => {
                    writeln!(f, "  skipped {} — {:?}: {}", entry.id, request, reason)?
                }
                RuleOutcome::SkippedCapability { missing } => {
                    writeln!(f, "  skipped {} — needs {:?}", entry.id, missing)?
                }
                RuleOutcome::NotRunSafetyLevel { required } => {
                    writeln!(f, "  not run {} — needs {:?}", entry.id, required)?
                }
            }
        }
        for (level, n) in self.not_run_by_level() {
            writeln!(f, "  {n} rules need {level:?}")?;
        }
        if !self.residue.is_empty() {
            writeln!(f, "  LEFT BEHIND ({}):", self.residue.len())?;
            for key in &self.residue {
                writeln!(f, "    {}", key.encode())?;
            }
        }
        Ok(())
    }
}
