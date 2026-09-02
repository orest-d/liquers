//! The rule inventory: one function per contract claim, grouped by `STORE_SEMANTICS.md` section.
//!
//! Every rule is written against one question: *what implementation change would make this fail?*
//! A rule with no plausible answer is decoration, and a decorative rule in a conformance suite is
//! worse than a missing one — it reports safety it never checked.

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

static RULES: &[Rule] = &[];
