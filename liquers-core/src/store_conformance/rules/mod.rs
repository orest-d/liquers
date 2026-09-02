//! The rule inventory: one function per contract claim, grouped by `STORE_SEMANTICS.md` section.
//!
//! Every rule is written against one question: *what implementation change would make this fail?*
//! A rule with no plausible answer is decoration, and a decorative rule in a conformance suite is
//! worse than a missing one — it reports safety it never checked.

pub mod sibling;

use super::Rule;

/// Build a [`Rule`] entry and the boxing shim an `async fn` needs to become a [`super::RuleFn`].
macro_rules! rule {
    ($id:literal, $title:literal, $contract:literal, [$($cap:ident),*], $level:ident, $body:path) => {
        $crate::store_conformance::Rule {
            meta: $crate::store_conformance::RuleMeta {
                id: $id,
                title: $title,
                contract: $contract,
                requires: &[$($crate::store_conformance::Capability::$cap),*],
                min_level: $crate::store_conformance::SafetyLevel::$level,
            },
            run: |fixture| Box::pin($body(fixture)),
        }
    };
}
pub(crate) use rule;

/// Every rule, in execution order.
pub fn all() -> &'static [Rule] {
    RULES
}

static RULES: &[Rule] = &[
    // §1 — the sibling rule.
    rule!(
        "sibling01",
        "removedir on a directory does not touch a sibling whose name extends it",
        "STORE_SEMANTICS.md §1",
        [RemoveDirectories],
        Scratch,
        sibling::sibling01
    ),
    rule!(
        "sibling02",
        "listdir reports nothing belonging to a name-extending sibling",
        "STORE_SEMANTICS.md §1",
        [Directories, Write],
        CreateOnly,
        sibling::sibling02
    ),
    rule!(
        "sibling03",
        "remove on a data key does not touch a key whose name extends it",
        "STORE_SEMANTICS.md §1",
        [Remove],
        Scratch,
        sibling::sibling03
    ),
    rule!(
        "sibling04",
        "a sibling's children do not make a name-extending key look like a directory",
        "STORE_SEMANTICS.md §1",
        [Directories, Write],
        CreateOnly,
        sibling::sibling04
    ),
    rule!(
        "sibling05",
        "a key refused as data is not addressable as a directory either",
        "STORE_SEMANTICS.md §1",
        [Directories],
        ReadOnly,
        sibling::sibling05
    ),
];
