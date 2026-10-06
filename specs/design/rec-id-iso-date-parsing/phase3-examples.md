# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| E1 | query | `-R/data/t.csv/-/ns-rec/to_record-csv/ns-rec/rec_id-2026~09~27` (table with `day: Date` Id field) → one record (validated with `liquers-validate`) |
| T1 | unit (`liquers-records`) | `FieldValue::parse_text(Date, "2026-09-27") == Date(20723)`; `(Timestamp, "1970-01-01T00:00:01Z") == Timestamp(1_000_000)` |
| T2 | unit (`liquers-lib` records commands) | `parse_id_value(Date, "2026-09-27")` and `(Date, "20723")` both give `Date(20723)` |
| T3 | unit | `parse_id_value(Date, "yesterday")` → conversion error whose message mentions both spellings |
| T4 | integration | E1 through `envref.evaluate` with a stored CSV whose Id is a Date (declared schema with `Id`, using whatever role marker the schema uses; check `records_schema` docs) |

Names: `field_value_parses_iso_date`, `rec_id_accepts_iso_and_raw_date`,
`rec_id_rejects_unparseable_date`, `rec_id_selects_by_iso_date_query`.
