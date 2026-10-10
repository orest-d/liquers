//! How much of an evaluation is kept for reuse.
//!
//! A [`CacheStrategy`] is a property of an asset — *how it was created* — and is read when an
//! asset is constructed or registered, never by the plan. A plan is a function of its query, the
//! command metadata and the `cut_predecessors` switch; whether a predecessor boundary is then
//! registered for reuse is decided when the boundary step executes.
//!
//! Where an asset's strategy comes from (`specs/design/plan-policy/`):
//!
//! | Asset | Strategy |
//! |---|---|
//! | keyed (a recipe) | the recipe's `cached:`, else the manager's `recipe_cache_strategy` |
//! | top-level non-keyed query | the manager's `query_cache_strategy` |
//! | non-keyed dependency (a boundary, a link, a `context.evaluate`) | the strategy of the asset that created it |
//!
//! An asset that already exists is reused whatever the strategy; the strategy only decides what a
//! new asset is registered as. A command's `cached: false` restricts further: its output is never
//! a boundary, and a non-keyed query ending with it is not registered.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};

/// How much of an evaluation is kept for reuse.
///
/// Closed set: a new value is a compile error at every match. Never glob-import the variants —
/// `CacheStrategy::None` would shadow `Option::None`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheStrategy {
    /// Nothing new is registered: neither the result nor an intermediate. An existing
    /// intermediate is still reused.
    None,
    /// The result is registered; a missing intermediate is evaluated unregistered. An existing
    /// intermediate is still reused.
    Result,
    /// The result and every intermediate are registered. The default, and the behaviour before
    /// strategies existed.
    #[default]
    All,
}

impl CacheStrategy {
    /// Whether an asset created under this strategy registers its own result for reuse.
    pub fn keeps_result(self) -> bool {
        match self {
            CacheStrategy::None => false,
            CacheStrategy::Result => true,
            CacheStrategy::All => true,
        }
    }

    /// Whether a new intermediate (a non-keyed dependency) created under this strategy is
    /// registered for reuse.
    pub fn keeps_intermediates(self) -> bool {
        match self {
            CacheStrategy::None => false,
            CacheStrategy::Result => false,
            CacheStrategy::All => true,
        }
    }

    /// True for the default; used by `skip_serializing_if`.
    pub fn is_all(&self) -> bool {
        matches!(self, CacheStrategy::All)
    }

    /// The serialized name: `none`, `result` or `all`.
    pub fn as_str(self) -> &'static str {
        match self {
            CacheStrategy::None => "none",
            CacheStrategy::Result => "result",
            CacheStrategy::All => "all",
        }
    }

    fn from_word(word: &str) -> Option<Self> {
        match word {
            "none" => Some(CacheStrategy::None),
            "result" => Some(CacheStrategy::Result),
            "all" => Some(CacheStrategy::All),
            _ => None,
        }
    }
}

/// `true` is [`CacheStrategy::All`] and `false` is [`CacheStrategy::None`] — the meaning a
/// boolean `cached:` had before strategies existed, extended to intermediates.
impl From<bool> for CacheStrategy {
    fn from(cached: bool) -> Self {
        if cached {
            CacheStrategy::All
        } else {
            CacheStrategy::None
        }
    }
}

impl fmt::Display for CacheStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What a `cached:` value may be written as.
#[derive(Deserialize)]
#[serde(untagged)]
enum StrategyForm {
    Bool(bool),
    Word(String),
}

const ACCEPTED: &str = "expected none, result, all, true or false";

impl<'de> Deserialize<'de> for CacheStrategy {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match StrategyForm::deserialize(deserializer)? {
            StrategyForm::Bool(cached) => Ok(CacheStrategy::from(cached)),
            StrategyForm::Word(word) => CacheStrategy::from_word(&word).ok_or_else(|| {
                serde::de::Error::custom(format!("unknown cache strategy '{word}': {ACCEPTED}"))
            }),
        }
    }
}

/// `deserialize_with` for an `Option<CacheStrategy>` field that also accepts `default`, meaning
/// "the manager's default" — the same as leaving the field out.
///
/// Pair it with `#[serde(default)]`, which covers the absent field.
pub fn deserialize_optional_strategy<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<CacheStrategy>, D::Error> {
    match Option::<StrategyForm>::deserialize(deserializer)? {
        None => Ok(None),
        Some(StrategyForm::Bool(cached)) => Ok(Some(CacheStrategy::from(cached))),
        Some(StrategyForm::Word(word)) if word == "default" => Ok(None),
        Some(StrategyForm::Word(word)) => CacheStrategy::from_word(&word)
            .map(Some)
            .ok_or_else(|| {
                serde::de::Error::custom(format!(
                    "unknown cache strategy '{word}': {ACCEPTED} or default"
                ))
            }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Deserialize, Serialize, PartialEq)]
    struct Holder {
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            deserialize_with = "deserialize_optional_strategy"
        )]
        cached: Option<CacheStrategy>,
    }

    fn holder(yaml: &str) -> Result<Holder, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }

    #[test]
    fn strategy_keeps_result_and_intermediates() {
        assert!(!CacheStrategy::None.keeps_result());
        assert!(!CacheStrategy::None.keeps_intermediates());
        assert!(CacheStrategy::Result.keeps_result());
        assert!(!CacheStrategy::Result.keeps_intermediates());
        assert!(CacheStrategy::All.keeps_result());
        assert!(CacheStrategy::All.keeps_intermediates());
        assert_eq!(CacheStrategy::default(), CacheStrategy::All);
    }

    #[test]
    fn recipe_cached_bool_and_words_deserialize() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(holder("cached: true")?.cached, Some(CacheStrategy::All));
        assert_eq!(holder("cached: false")?.cached, Some(CacheStrategy::None));
        assert_eq!(holder("cached: none")?.cached, Some(CacheStrategy::None));
        assert_eq!(holder("cached: result")?.cached, Some(CacheStrategy::Result));
        assert_eq!(holder("cached: all")?.cached, Some(CacheStrategy::All));
        assert_eq!(holder("cached: default")?.cached, None);
        assert_eq!(holder("{}")?.cached, None);

        let json: Holder = serde_json::from_str(r#"{"cached":false}"#)?;
        assert_eq!(json.cached, Some(CacheStrategy::None));

        // Written as the word, and an absent value is not written at all.
        let written = serde_json::to_string(&Holder {
            cached: Some(CacheStrategy::Result),
        })?;
        assert_eq!(written, r#"{"cached":"result"}"#);
        assert_eq!(serde_json::to_string(&Holder { cached: None })?, "{}");
        Ok(())
    }

    #[test]
    fn unknown_strategy_word_is_rejected() {
        let error = holder("cached: some").expect_err("an unknown word is an error");
        let message = error.to_string();
        assert!(message.contains("some"), "{message}");
        assert!(message.contains("result"), "names the accepted values: {message}");

        let plain: Result<CacheStrategy, _> = serde_yaml::from_str("default");
        assert!(plain.is_err(), "`default` exists only on an optional field");
    }
}
