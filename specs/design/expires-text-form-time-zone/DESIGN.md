---
id: EXPIRES-TEXT-FORM-TIME-ZONE
kind: design
title: The text form of an expiration carries the time zone of every zoned rule
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/assets]
issues: [EXPIRES-TEXT-FORM-DROPS-TIME-ZONE]
created: 2026-10-26
---
# The text form of an expiration carries the time zone of every zoned rule

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented.

## Phase 1: High-Level Design

### Purpose

`Expires` is stored as text (metadata, recipes, command `expires:`). The text form loses the time
zone of `on <day>`, `end of day`, `end of week` and `end of month`, and — contrary to the issue's
assumption — also of `at HH:MM` with any zone other than `UTC`/`GMT`. A rule read back is evaluated
in the reading process's system zone, so its deadline moves by the zone difference. A rule must
read back as the rule that was written.

### Problem Example

Verified by reading HEAD (`cd6d5da`, `liquers-core/src/expiration.rs`):

```rust
let e = Expires::EndOfDay { tz_offset: Some(3600) };
e.to_string()                       // "end of day"         -> parses to EndOfDay { None }
let t = Expires::AtTimeOfDay { hour: 13, minute: 0, second: 0, tz_offset: Some(7200) };
t.to_string()                       // "at 13:00 +02:00"    -> parses to tz_offset: None
"at 08:30 CET".parse::<Expires>()   // tz Some(3600), displays "at 08:30 +01:00" -> None on re-read
Expires::AtTimeOfDay { hour: 8, minute: 0, second: 0, tz_offset: Some(-1800) }.to_string()
                                    // "at 08:00 +00:30"    (sign lost: -1800 / 3600 == 0)
"at 08:30 JST".parse::<Expires>()   // Ok, tz None: an unknown zone is silently dropped
```

`parse_time_of_day` has a stub for `+HH:MM` that returns `None`. Should be: every zoned rule
displays with its zone and parses back equal; a zone the parser does not know is an error.

### Scope and Acceptance Criteria

- **AC-1** End-of-period and weekday rules keep their zone
  WHEN `EndOfDay`, `EndOfWeek`, `EndOfMonth` or `OnDayOfWeek` with `tz_offset: Some(x)` is displayed and parsed back
  THEN the result equals the original, for `x` = 0, positive, negative, and with minutes
- **AC-2** Numeric offsets parse
  WHEN `at 13:00 +02:00`, `end of day -05:30`, `on monday UTC`, `eod +01:00` are parsed
  THEN they yield the corresponding `tz_offset` (7200, -19800, 0, 3600)
- **AC-3** Negative sub-hour offsets display correctly
  WHEN an offset in (-3600, 0) is displayed
  THEN the sign is `-` (`-00:30`)
- **AC-4** Zone-less text unchanged
  WHEN any existing zone-less text (`eod`, `end of week`, `on Monday`, `at 18:00`, combinations) is parsed and displayed
  THEN the results are as today (the existing `expiration.rs` tests pass)
- **AC-5** Unknown zone
  WHEN a rule names a zone the parser does not know (`at 08:30 JST`)
  THEN parsing fails with an error naming the zone
- **AC-6** Metadata round trip
  WHEN metadata with a zoned rule is serialized to JSON/YAML and read back
  THEN the rule is equal

Non-goals: IANA zone names (`Europe/Prague`, daylight-saving rules); new abbreviations; the
combination algebra (`expires-cross-timezone-reduction`).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — rule 4: the stored text syntax grows (a serialized-format
  addition), and AC-5 turns accepted text into an error.
- **Leading issue:** **Open design question - an unknown zone token.** Recommended: an error
  (AC-5). Today `at 08:30 JST` is accepted and silently means "system zone", which is a wrong
  deadline nobody is told about. Consequence: a stored or recipe `expires` with an unknown token
  stops parsing — a recipe then fails to plan, and stored metadata fails to load its `expires`.
  Alternative: keep accepting it with a warning-free `None`, as today.
- **Explanation:** the zone grammar already exists for `at` (abbreviations, `UTC`); extending it to
  the four other variants and adding numeric offsets is a parser and formatter change in one file.
  Older binaries cannot read the new zoned text, which only Rust-API-built rules produce.
- **Open questions:**
  1. **Open design question - unknown zone token** (above).
  2. **Proposed resolution - one canonical zone spelling:** `UTC` for 0, `±HH:MM` otherwise; an
     abbreviation is accepted on input and displayed numerically.
  3. **Proposed resolution - keep the `tz_offset` fields** rather than removing them from the four
     variants (the issue's second option): removing them is a `pub` enum change and loses a
     capability the evaluation code already implements.

### Design Dependencies

- `expires-cross-timezone-reduction` — **requires** (ordering; finished Phase 4, not edited): once
  text can carry zones, parsed combinations reach `combine_atomic_pair` with different zones, which
  that design fixes. Land it first. Same file, different functions.
- `COMBINED-EXPIRES` (its source) — the combination text form (`,`) is unaffected: zone tokens
  contain no comma.

### Consolidated Findings

- The parser lowercases its input before matching; zone tokens are matched case-insensitively
  (`tz_abbreviation_to_offset` already upper-cases).
- `on <day> <zone>`: the day name is the first word after `on`, the zone the optional second; a
  third word is an error, as today any non-day text is.
- End-of-period keywords may be multi-word (`end of week`), so the zone is recognised as a trailing
  token after a known keyword, not by word position.
- Display must compute the sign from the offset, not from the truncated hour (AC-3).

## Phase 2: Architecture

### Solution

All in `liquers-core/src/expiration.rs`:

1. `fn parse_zone(token: &str) -> Result<i32, Error>` (private): an abbreviation through
   `tz_abbreviation_to_offset`, or `+HH:MM` / `-HH:MM` / `+HH` / `-HH` with `HH <= 14`,
   `MM < 60`; otherwise `Err(Error::general_error("Unknown time zone '…'"))`.
2. `fn format_zone(offset: i32) -> String` (private): `"UTC"` for 0, else sign from
   `offset < 0`, then `{:02}:{:02}` of `offset.abs() / 3600` and `offset.abs() % 3600 / 60`.
3. `parse_atomic_expires`: for the keywords `eod`, `end of day`, `eow`, `end of week`, `eom`,
   `end of month`, accept the keyword alone or followed by one space and a zone token
   (`strip_prefix(keyword)` then the remainder, trimmed, through `parse_zone`); `on <day> [zone]`
   likewise. `parse_time_of_day` uses `parse_zone` for its optional second part (replacing the
   stub), so an unknown token is an error (open question 1).
4. `impl Display for Expires`: the four variants and `AtTimeOfDay` append
   `" " + format_zone(offset)` when `tz_offset` is `Some`. Explicit match arms as today.

Rejected: removing `tz_offset` from the four variants (open question 3); IANA names (needs a zone
database dependency, `chrono-tz`).

### Changes

- `liquers-core/src/expiration.rs`: `parse_zone`, `format_zone`, `parse_atomic_expires`,
  `parse_time_of_day`, `Display`. No `pub` signature change; `Serialize`/`Deserialize` unchanged
  (they go through `Display` / `FromStr`).
- `specs/reference/RECIPES_AND_PLANS.md` `expires` row: the optional zone (`eod UTC`,
  `on Monday +02:00`, `at 18:00 CET`) and that an unknown zone is an error. History row,
  `reviewed:`. `COMMAND_DESIGN_GUIDE.md` / `REGISTER_COMMAND_FSD.md` `expires:` text if it lists the
  syntax (check at implementation).

### Risks

| Risk | Handling | Certainty |
|---|---|---|
| Existing recipes with an unknown zone stop parsing | open question 1; grep the repository's recipes and examples for `at .* [A-Z]{3}` at implementation | medium |
| Older processes cannot read new zoned text | only Rust-built zoned rules produce it; noted in the reference | high |
| `eod` prefix matching swallows other words (`eodx`) | match keyword then require end-of-string or a space | high |
| Combination reduction with zones | `expires-cross-timezone-reduction` first | high |

## Phase 3: Examples and Tests

### Examples

The Problem Example. Recipe text after the change: `expires: "eod UTC"`, `expires: "on Monday
+02:00"`, `expires: "at 18:00 CET, eom +01:00"`.

### Tests

Unit tests in `liquers-core/src/expiration.rs` `tests`:

- `zoned_end_of_period_round_trips` — every one of the four variants × offsets
  {0, 3600, -18000, 19800, -1800}: `to_string().parse() == original`. Covers AC-1.
- `parse_numeric_and_named_zones` — the four strings of AC-2. Covers AC-2.
- `display_negative_sub_hour_offset` — `-1800` → `at 08:00 -00:30`. Covers AC-3.
- Existing `test_parse_*`, `test_display_*`, combination tests unchanged. Covers AC-4.
- `unknown_zone_is_an_error` — `at 08:30 JST`, `eod XYZ`. Covers AC-5.
- `zoned_expires_metadata_round_trip` — a `MetadataRecord` with `expires` = `EndOfWeek {
  Some(7200) }` through `serde_json` and `serde_yaml`. Covers AC-6.

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib expiration`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `expiration.rs` tests above; AC-1, AC-2, AC-3, AC-5, AC-6 fail at HEAD — `cargo test -p liquers-core --lib expiration`
- [ ] 2. `parse_zone`, `format_zone`; `parse_time_of_day` uses `parse_zone` — AC-2 (`at`), AC-5
- [ ] 3. `parse_atomic_expires` zone suffixes; `Display` arms — AC-1, AC-3, AC-6 pass — `cargo test -p liquers-core --lib expiration`
- [ ] 4. Grep recipes, examples and tests for zone tokens the parser now rejects; fix or report —
  `grep -rnE "expires.*at [0-9:]+ [A-Za-z]+" --include=*.yaml --include=*.rs .`
- [ ] 5. `RECIPES_AND_PLANS.md` (History, `reviewed:`); issue closed — `python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests` (commands with
`expires:`); `cargo run -p liquers-lib --features cli --bin export-command-registry -- --format yaml
-o specs/command_registry.yaml` must produce no diff (no command's `expires` text changes).
