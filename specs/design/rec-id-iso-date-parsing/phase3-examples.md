# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| E1 | query | `-R/data/t.csv/-/ns-rec/to_record-csv/ns-rec/rec_id-20260927` → the 2026-09-27 record (validated) |
| E2 | query | `-R/data/t.csv/-/ns-rec/to_record-csv/ns-rec/rec_id-2026~09~27` → same (validated) |
| T1 | unit (`liquers-records`) | `FieldValue::parse_text(Date, "2026-09-27") == Date(20723)` |
| T2 | unit (`liquers-lib`) | `parse_id_value(Date, "20260927")` and `(Date, "2026-09-27")` → `Date(20723)` |
| T3 | unit | `parse_id_value(Timestamp, "20260927T100000Z")` and `("2026-09-27T10:00:00Z")` agree |
| T4 | unit | `parse_id_value(Date, "20723")` → error mentioning `YYYY~MM~DD` |
| T5 | integration | E1 and E2 through `envref.evaluate` on a stored CSV with a Date `Id` field |

Names: `field_value_parses_iso_date`, `rec_id_accepts_basic_and_extended_dates`,
`rec_id_accepts_basic_and_extended_timestamps`, `rec_id_rejects_epoch_day_numbers`,
`rec_id_selects_by_date_query`.
