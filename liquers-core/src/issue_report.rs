//! Structured diagnostics emitted while validating core configuration.

use std::fmt::{Display, Formatter};

use crate::error::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IssueSeverity {
    Debug,
    Info,
    Warning,
    Error,
}

impl Display for IssueSeverity {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Debug => f.write_str("debug"),
            Self::Info => f.write_str("info"),
            Self::Warning => f.write_str("warning"),
            Self::Error => f.write_str("error"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Issue {
    Generic {
        severity: IssueSeverity,
        message: String,
    },
    CommandRegistry {
        severity: IssueSeverity,
        realm: String,
        namespace: String,
        name: String,
        message: String,
    },
}

impl Issue {
    pub fn generic(severity: IssueSeverity, message: impl Into<String>) -> Self {
        Self::Generic {
            severity,
            message: message.into(),
        }
    }

    pub fn command_registry(
        severity: IssueSeverity,
        realm: impl Into<String>,
        namespace: impl Into<String>,
        name: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::CommandRegistry {
            severity,
            realm: realm.into(),
            namespace: namespace.into(),
            name: name.into(),
            message: message.into(),
        }
    }

    pub fn severity(&self) -> IssueSeverity {
        match self {
            Self::Generic { severity, .. } | Self::CommandRegistry { severity, .. } => *severity,
        }
    }

    pub fn is_error(&self) -> bool {
        self.severity() == IssueSeverity::Error
    }

    pub fn message(&self) -> &str {
        match self {
            Self::Generic { message, .. } | Self::CommandRegistry { message, .. } => message,
        }
    }

    fn command_key(&self) -> Option<String> {
        match self {
            Self::Generic { .. } => None,
            Self::CommandRegistry {
                realm,
                namespace,
                name,
                ..
            } => Some(format!("{realm}-{namespace}-{name}")),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueReport {
    issues: Vec<Issue>,
}

impl IssueReport {
    pub fn issues(&self) -> &[Issue] {
        &self.issues
    }
    pub fn append(&mut self, other: Self) {
        self.issues.extend(other.issues);
    }
    pub fn extend<I: IntoIterator<Item = Issue>>(&mut self, issues: I) {
        self.issues.extend(issues);
    }
    pub fn error_count(&self) -> usize {
        self.issues.iter().filter(|issue| issue.is_error()).count()
    }
    pub fn warning_count(&self) -> usize {
        self.issues
            .iter()
            .filter(|issue| issue.severity() == IssueSeverity::Warning)
            .count()
    }
    pub fn has_errors(&self) -> bool {
        self.error_count() != 0
    }
    pub fn is_empty(&self) -> bool {
        self.issues.is_empty()
    }

    pub fn short_summary(&self, subject: &str) -> Option<String> {
        let errors: Vec<&Issue> = self
            .issues
            .iter()
            .filter(|issue| issue.is_error())
            .collect();
        let first = errors.first()?;
        let count = errors.len();
        let count_word = if count == 1 { "error" } else { "errors" };
        let mut summary = format!(
            "{subject} contains {count} {count_word}; first: {}",
            first.message()
        );
        if let Some(key) = first.command_key() {
            summary.push_str(&format!(" on {key}"));
        }

        let mut keys = Vec::new();
        for issue in &errors {
            if let Some(key) = issue.command_key() {
                if !keys.contains(&key) {
                    keys.push(key);
                }
            }
        }
        let first_key = first.command_key();
        let further: Vec<String> = keys
            .into_iter()
            .filter(|key| Some(key) != first_key.as_ref())
            .collect();
        if !further.is_empty() {
            let shown: Vec<&str> = further.iter().take(4).map(String::as_str).collect();
            summary.push_str(&format!(
                ". Further command keys with errors: {}",
                shown.join(", ")
            ));
            if further.len() > shown.len() {
                summary.push_str(&format!(" (and {} more)", further.len() - shown.len()));
            }
        }
        Some(summary)
    }

    pub fn to_error(&self, subject: &str) -> Option<Error> {
        self.short_summary(subject).map(Error::general_error)
    }

    pub fn emit(&self) {
        if self.is_empty() {
            return;
        }
        emit_report(&self.to_string());
    }
}

impl Display for IssueReport {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        for (index, issue) in self.issues.iter().enumerate() {
            if index != 0 {
                f.write_str("\n")?;
            }
            write!(f, "{}: {}", issue.severity(), issue.message())?;
            if let Some(key) = issue.command_key() {
                write!(f, " on {key}")?;
            }
        }
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn emit_report(report: &str) {
    eprintln!("{report}");
}

#[cfg(target_arch = "wasm32")]
fn emit_report(report: &str) {
    web_sys::console::error_1(&wasm_bindgen::JsValue::from_str(report));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_severities_and_error_conversion() {
        let mut report = IssueReport::default();
        report.extend([
            Issue::generic(IssueSeverity::Debug, "debug"),
            Issue::generic(IssueSeverity::Info, "info"),
            Issue::generic(IssueSeverity::Warning, "warning"),
        ]);
        assert_eq!(report.warning_count(), 1);
        assert!(!report.has_errors());
        assert!(report.to_error("registry").is_none());
        report.extend([Issue::generic(IssueSeverity::Error, "error")]);
        assert_eq!(
            report.short_summary("registry"),
            Some("registry contains 1 error; first: error".to_string())
        );
    }
}
