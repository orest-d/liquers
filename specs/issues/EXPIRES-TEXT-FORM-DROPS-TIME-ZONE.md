---
id: EXPIRES-TEXT-FORM-DROPS-TIME-ZONE
kind: issue
title: The text form of an expiration drops the time zone of day-of-week and end-of-period rules
status: draft
priority: P3
complexity: M
area: [core/assets]
design:
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
`end of week`, `eom`, `end of month`, `on <day>` all yield `tz_offset: None`). Only `AtTimeOfDay`
round-trips its zone (`at 13:00 UTC`, `at 13:00 +02:00`). So
`Expires::EndOfDay { tz_offset: Some(3600) }` written to metadata reads back as
`EndOfDay { tz_offset: None }` — the system zone of whichever process reads it — and a zoned
combination (`COMBINED-EXPIRES`) serializes to a string that parses back zone-less and then reduces.

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
