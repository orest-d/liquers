---
id: STORE-VERSION-BLIND-TO-CHANGES-MADE-OUTSIDE-LIQUERS
kind: issue
title: A stored value's version changes only when Liquers writes it, so edits made by other programs are invisible to dependency checks
status: in_progress
priority: P2
complexity: L
area: [core/store, core/assets]
design: dependency-audit-and-expiry-provenance
created: 2026-09-29
github:
---

## Problem

A keyed asset's **version** is a content hash that Liquers computes when *it* writes the value, and
stores in the metadata (`Version::from_bytes`, via `set_binary`, `set_state` or evaluation:
`liquers-core/src/assets.rs:2022`, `:5746`, `:5860`). Every dependency check reads that recorded
version and never the bytes: fast-track validation, cascades, and the dependency audit
(`AssetManager::version`, `assets.rs:4346`, deliberately "metadata only, never the value").

So when a program other than Liquers changes the data, nothing notices:

- **A file store directory edited by hand or by another tool.** `data/a.csv` is overwritten, and
  its sidecar `data/a.csv.__metadata__` still says version `V1`. `data/report.txt`, computed from
  `V1`, keeps being served as fresh, even by a strict audit.
- **A file dropped in without a sidecar.** It has no version at all, so an audit reads "no version"
  and expires every dependent that recorded one. That is safe, but it recomputes even when nothing
  changed.

## Impact

The dependency machinery is correct only for data that Liquers writes. For stores that other
programs also write (shared folders, object storage fed by pipelines), derived results can be
stale with no signal. `design/dependency-audit-and-expiry-provenance/` makes audits compare
versions correctly and adds a strict on-load policy, but it can only compare the versions it is
given. This issue is the reason that policy is not a guarantee for such stores.

Workaround: write through Liquers (`set_binary` / `set_state`), or remove the sidecar after an
outside edit so the audit treats the key as changed.

## Expected behaviour

*Solution direction set by the project owner on 2026-09-29. It replaces an earlier idea of
backend-specific change tokens.*

**Verify versions on demand by re-hashing the bytes.** When a stored value is read (or when a
verification is requested), hash its bytes and compare the result with the version in its metadata.
Equal means the metadata is truthful. Different means the content was changed outside Liquers.

**Only hash versions can be verified, so a hash version must be recognizable.** Today a `Version`
is an opaque `u128` produced in several ways (`metadata.rs:17-70`):

| Constructor | Kind | Verifiable by re-hashing? |
|---|---|---|
| `from_bytes` | content hash (first 128 bits of blake3) | yes |
| `from_time_now`, `from_specific_time` | nanoseconds since 1970 | no |
| `new_unique` | nanoseconds (high 64 bits) + counter (low 64 bits) | no |
| `unknown()` | `0` | no (means "no fingerprint") |
| `new(v)` | arbitrary (tests, command versions) | no |

Reserve **bit 127 as the hash flag**: `from_bytes` sets it to 1, and `is_hash()` tests it. Two
properties make this safe:

- `unknown()` = 0 has the bit clear, so it can never be mistaken for a hash.
- The time-based and unique kinds do not set it today. A plain nanosecond count is about 2^61 and
  far below 2^127. `new_unique` shifts that count left by 64 (`metadata.rs:86`), so it reaches bit
  127 only when nanoseconds reach 2^63, which is the year 2262. Everything that is not a hash
  therefore reads as "not verifiable" without any change to those constructors. Even so, it is safer
  to have `new_unique` and the time constructors clear bit 127 explicitly, so the rule is enforced
  rather than depending on the date.
- Command metadata versions are already hashes (`calculate_metadata_version` uses `from_bytes`,
  `command_metadata.rs:1233`). They gain the flag automatically, which is harmless: nothing
  re-hashes them.

A hash then carries 127 bits instead of 128, which is still far beyond any collision concern.

**Migration.** Existing stored hashes were produced without the flag, so about half of them have
bit 127 set by chance and half do not. Those with the bit clear read as "not verifiable" and are
simply not checked. Those with it set verify correctly, because the flag leaves them unchanged. When
such a value is next written, its new flagged version differs from the old one, which causes a
single cascade for its dependents. That is a one-time recomputation, not a correctness problem.

**What a mismatch means depends on the asset:**

| Asset | On mismatch |
|---|---|
| `Status::Source` (no recipe; the data *is* the input) | **Always user input.** Accept the content, set its version to the new hash, and cascade to dependents. |
| `Status::Override` (a user-pinned value) | User input, as for `Source`. |
| Has a recipe (`Ready`, `Expired`, …) | **A choice:** (a) *corrupted*: delete the stored copy and recompute on the next request; or (b) *user input*: keep the content, convert it to `Override` with the new hash, and cascade to dependents. |

**Where the check runs.** On read, when the bytes are already in hand (the fast track; the binary
read paths), so the only extra cost is the hash, which blake3 computes at gigabytes per second. It
also runs as an explicit on-demand operation that walks a folder or the whole store. A metadata-only
check (such as today's audit) cannot detect this, because it never sees the bytes.

**Open questions:**

- Where does the choice for recipe-backed assets live: per environment, per recipe, or both?
  Which default? Deleting is destructive, which argues for "user input" as the default.
- A file with *no* metadata (dropped in by another program) has no recorded hash to compare against.
  With no recipe it is already a `Source`. With a recipe, the same choice as above applies.
- Should a mismatch on read also be recorded as an `ExpiryReason` / log entry, such as
  "changed outside Liquers" (see `design/dependency-audit-and-expiry-provenance/` Part C)?

Keep the "metadata kept, data deleted" workflow working (see `DEPENDENCY-AUDIT-POLICY-NOT-EXPRESSIBLE`):
missing bytes are not a mismatch.

## Discovery

Found 2026-09-29 while writing worked examples for `dependency-audit-and-expiry-provenance`. The
first example, "a.csv changed, so report.txt must expire", holds only if a.csv changed through
Liquers.
