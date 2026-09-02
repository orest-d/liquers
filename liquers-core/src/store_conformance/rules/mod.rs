//! The rule inventory: one function per contract claim, grouped by `STORE_SEMANTICS.md` section.
//!
//! Every rule is written against one question: *what implementation change would make this fail?*
//! A rule with no plausible answer is decoration, and a decorative rule in a conformance suite is
//! worse than a missing one — it reports safety it never checked.

pub mod absence;
pub mod enumerate;
pub mod directories;
pub mod explicit;
pub mod keyshape;
pub mod prefix;
pub mod removal;
pub mod sibling;
pub mod sidecar;
pub(crate) mod support;

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
    // §2 — directories on a backend that has none, and the data round trip.
    rule!(
        "dir01",
        "a directory holding children is addressable by is_dir and contains",
        "STORE_SEMANTICS.md §2",
        [Directories, Write],
        CreateOnly,
        directories::dir01
    ),
    rule!(
        "dir02",
        "is_dir on an absent key is Ok(false), never an error",
        "STORE_SEMANTICS.md §2",
        [Directories],
        ReadOnly,
        directories::dir02
    ),
    rule!(
        "dir03",
        "every entry listdir calls a directory answers is_dir == true",
        "STORE_SEMANTICS.md §2",
        [Directories, Write],
        CreateOnly,
        directories::dir03
    ),
    rule!(
        "dir04",
        "a directory's metadata is directory-shaped and carries its key",
        "STORE_SEMANTICS.md §2",
        [Directories, Write],
        CreateOnly,
        directories::dir04
    ),
    rule!(
        "dir05",
        "contains falls back to is_dir",
        "STORE_SEMANTICS.md §2",
        [Directories, Write],
        CreateOnly,
        directories::dir05
    ),
    rule!(
        "dir06",
        "a key answering is_dir appears in its parent's listing",
        "STORE_SEMANTICS.md §2",
        [Directories, Write],
        CreateOnly,
        directories::dir06
    ),
    rule!(
        "dir07",
        "directory metadata does not populate children",
        "STORE_SEMANTICS.md §2",
        [Directories, Write],
        CreateOnly,
        directories::dir07
    ),
    rule!(
        "data01",
        "set then get returns the same bytes",
        "STORE_SEMANTICS.md §2",
        [Write],
        CreateOnly,
        directories::data01
    ),
    // §3 — derived and explicit directories are different things.
    rule!(
        "explicit01",
        "makedir creates a directory that exists, is empty, and persists",
        "STORE_SEMANTICS.md §3",
        [ExplicitDirectories],
        CreateOnly,
        explicit::explicit01
    ),
    rule!(
        "explicit02",
        "a derived directory retires when its last child goes",
        "STORE_SEMANTICS.md §3",
        [Directories, DerivedDirectories, Write, Remove],
        Scratch,
        explicit::explicit02
    ),
    rule!(
        "explicit03",
        "a recursive removedir takes explicit descendants with it",
        "STORE_SEMANTICS.md §3",
        [ExplicitDirectories, RemoveDirectories],
        Scratch,
        explicit::explicit03
    ),
    // §4 — absence is not an error.
    rule!(
        "absence01",
        "reading an absent key gives KeyNotFound, from all three read methods",
        "STORE_SEMANTICS.md §4",
        [],
        ReadOnly,
        absence::absence01
    ),
    rule!(
        "absence02",
        "contains on an absent key is Ok(false), not an error",
        "STORE_SEMANTICS.md §4",
        [],
        ReadOnly,
        absence::absence02
    ),
    rule!(
        "absence03",
        "removedir on a directory that does not exist returns Ok(())",
        "STORE_SEMANTICS.md §4",
        [RemoveDirectories],
        CreateOnly,
        absence::absence03
    ),
    // §5 — removal.
    rule!(
        "remove01",
        "after removedir returns Ok, the directory does not exist",
        "STORE_SEMANTICS.md §5",
        [RemoveDirectories, Write],
        Scratch,
        removal::remove01
    ),
    rule!(
        "remove02",
        "removedir is recursive: no child survives it",
        "STORE_SEMANTICS.md §5",
        [RemoveDirectories, Write],
        Scratch,
        removal::remove02
    ),
    rule!(
        "remove03",
        "remove deletes data and metadata together",
        "STORE_SEMANTICS.md §5",
        [Remove, Write],
        Scratch,
        removal::remove03
    ),
    rule!(
        "data02",
        "writing a key that already exists replaces its content",
        "STORE_SEMANTICS.md §5",
        [Write],
        Scratch,
        removal::data02
    ),
    // §6 — keys, prefixes and routing.
    rule!("prefix01", "key_prefix() reports the prefix the store was configured with",
        "STORE_SEMANTICS.md §6", [], ReadOnly, prefix::prefix01),
    rule!("prefix02", "is_supported is false for a key outside this store's prefix",
        "STORE_SEMANTICS.md §6", [], ReadOnly, prefix::prefix02),
    rule!("prefix03", "is_supported is false for a key whose shape the store cannot address",
        "STORE_SEMANTICS.md §6", [], ReadOnly, prefix::prefix03),
    rule!("prefix04", "is_supported is true for a key inside the prefix the store can address",
        "STORE_SEMANTICS.md §6", [], ReadOnly, prefix::prefix04),
    // §7 — key shape.
    rule!("keyshape01", "every fallible key-taking method refuses a relative key with KeyNotAbsolute",
        "STORE_SEMANTICS.md §7", [], CreateOnly, keyshape::keyshape01),
    // §8 — metadata sidecars.
    rule!("sidecar01", "a key that would collide with another key's metadata path is refused",
        "STORE_SEMANTICS.md §8", [], ReadOnly, sidecar::sidecar01),
    rule!("sidecar02", "metadata written with set_metadata reads back",
        "STORE_SEMANTICS.md §8", [StoredMetadata, Write], CreateOnly, sidecar::sidecar02),
    // §9 — what keys() returns.
    rule!("keys01", "every key keys() returns starts with the store's prefix",
        "STORE_SEMANTICS.md §9", [EnumerateKeys], ReadOnly, enumerate::keys01),
    rule!("keys02", "keys() returns data keys, the directories above them, and the prefix itself",
        "STORE_SEMANTICS.md §9", [EnumerateKeys, Write], CreateOnly, enumerate::keys02),
];
