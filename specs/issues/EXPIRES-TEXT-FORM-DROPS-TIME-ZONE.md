---
id: EXPIRES-TEXT-FORM-DROPS-TIME-ZONE
kind: issue
title: The text form of an expiration drops the time zone of day-of-week and end-of-period rules
status: draft
priority: P3
complexity: S
area: [core/assets]
design: expires-text-form-time-zone
created: 2026-10-20
github:
---
## Problem

`Expires` is persisted as a string (`impl Serialize for Expires` and `Display`,
`liquers-core/src/expiration.rs:671-735`), but `Display` ignores `tz_offset` for four variants:

```rust
Expires::OnDayOfWeek { day, tz_offset: _ } => write!(f, "on {}", weekday_name(*day)),
Expires::EndOfDay { tz_offset: _ } => write!(f, "end of day"),
Expires::EndOfWeek { tz_offset: _ } => write!(f, "end of week"),
Expires::EndOfMonth { tz_offset: _ } => write!(f, "end of month"),
```

and `parse_atomic_expires` (`:483`) has no syntax for a zone on them (`eod`, `end of day`, `eow`,
`end of week`, `eom`, `end of month`, `on <day>` all yield `tz_offset: None`). `AtTimeOfDay`
round-trips only `UTC` (and `GMT`): `Display` writes any other offset as `+HH:MM`, which
`parse_time_of_day` does not parse (its `+HH:MM` branch is a stub returning `None`), so
`at 13:00 +02:00` and an abbreviation such as `CET` read back zone-less. `Display` also loses the
sign of an offset between -1 h and 0 (`-1800` → `+00:30`), and an unknown zone token
(`at 08:30 JST`) is accepted silently as the system zone. So
`Expires::EndOfDay { tz_offset: Some(3600) }` written to metadata reads back as
`EndOfDay { tz_offset: None }` — the system zone of whichever process reads it — and a zoned
combination (`COMBINED-EXPIRES`) serializes to a string that parses back zone-less and then reduces.

## Problem Example

```rust
Expires::EndOfDay { tz_offset: Some(3600) }.to_string()      // "end of day" -> EndOfDay { None }
"at 13:00 +02:00".parse::<Expires>()                          // tz_offset: None
"at 08:30 JST".parse::<Expires>()                             // Ok, tz_offset: None
```

Re-checked at `cd6d5da`, 2026-10-26: the `at` cases above were found during bulk design; the issue
originally assumed `at` round-tripped.

## Impact

Low. Only rules built through the Rust API carry a zone; a command's or recipe's `expires:` is
text and is zone-less already. Such a rule's deadline moves by the zone difference after a store
round trip or between processes in different system zones.

## Expected behaviour

Either the text form carries the zone for these variants (for example `end of day UTC`,
`on monday +02:00`, mirroring `AtTimeOfDay`), with the parser accepting it, or the variants lose
their `tz_offset` field so the Rust API cannot express what the text form cannot store. The first
is a syntax addition (a serialized-format change); the second a `pub` enum change.

## Discovery

Bulk-design triage of `COMBINED-EXPIRES`, 2026-10-20, reading `expiration.rs` at commit 18732dd.
Related design: `expires-cross-timezone-reduction` (fixes the combination algebra; does not touch
the text form).
