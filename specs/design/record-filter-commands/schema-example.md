# What `schema` returns — worked example (AC-14)

`schema` answers "what are the columns of this table, and what are they". The outputs below were
produced on 2026-10-13 by evaluating the queries in a scratch test. `rec/schema` did not exist
yet, so its alias `records_schema` was used.

**Input:** `data/orders.csv`

```
order_id,placed_at,customer,amount,tags
A-1,2026-10-01T09:15:00Z,Alice Smith,120.5,"[""gift""]"
A-2,2026-10-02T14:00:00Z,Bob Jones,35.0,[]
A-3,2026-10-03T08:30:00Z,,99.9,"[""bulk"",""b2b""]"
```

**1. Inferred.** `-R/data/orders.csv/-/ns-rec/to_record-csv/schema` returns this schema (the
command emits it as JSON text; it is shown here as YAML):

```
fields:
  - { name: order_id,  data_type: Text,      nullable: false }
  - { name: placed_at, data_type: Timestamp, nullable: false }   # ISO text recognised
  - { name: customer,  data_type: Text,      nullable: true }    # row A-3 is empty
  - { name: amount,    data_type: Float,     nullable: false }
  - { name: tags,      data_type: Text,      nullable: false }
type_identifier: null
```

Every field has `key: None` and an empty role. Inference finds types and nulls. It cannot know
which column is the Id, what is searchable, or that `tags` holds JSON.

**2. Declared.** A person writes what inference cannot know, and links it in:

```yaml
type_identifier: shop.Order
fields:
  - name: order_id
    data_type: Text
    nullable: false
    key: Id                          # at most one; gets role {indexed: [Exact], stored: true}
  - name: placed_at
    data_type: Timestamp
    nullable: false
    role: { indexed: [Range], stored: true, fast: true }
  - name: customer
    label: Customer name
    data_type: Text
    role: { indexed: [Exact, !FullText { analyzer: Simple, positions: true }], stored: true, fast: false }
  - name: amount
    data_type: Float
    description: Order total in EUR
    role: { indexed: [Range], stored: true, fast: true }
  - name: tags
    data_type: Text
    media_type: application/json     # cells hold JSON arrays; filters match any element (AC-8)
```

```
-R/data/orders.csv/-/ns-rec/to_record-csv-t-~X~-R/data/orders.schema.txt~E/schema
```

This returns the declared schema back, with defaults filled in: `label: order id`,
`description: ""`, `nullable: true` where omitted, and the Id's role. It is the form that
`to_record`, `from_json` and a manifest's `uniform_schema` accept. In YAML, an index kind with
settings is a tag (`!FullText { … }`), and a `role` must currently list `indexed`, `stored` and
`fast`. `RecordSchema::new` refuses a duplicate name, or a second `Id` or `Source` field.

**3. polars.** `-R/data/orders.csv/-/ns-pl/schema` returns an object of polars type names:

```
{amount: "Float64", customer: "String", order_id: "String", placed_at: "String", tags: "String"}
```

The polars CSV reader leaves `placed_at` as a string, while `ns-rec` reads it as a Timestamp. There
are no nullability, keys, roles or labels. Phase 1 Open Question 1 is whether to keep this form.

**Defects found by running these, now covered by Phase 1 AC-14:**
- The declared schema linked as `orders.schema.yaml` fails: "Can't convert 'bytes' to string".
  A `.yaml` resource arrives as bytes, and the `schema: String` argument refuses bytes. The same
  text stored as `.txt` works.
- `schema`'s JSON output cannot be linked back. The argument is parsed with `serde_yaml`, which
  wants `!FullText` and refuses JSON's `{"FullText": {…}}`. Every `catalog` schema hits this.
- A `role` that omits `stored` or `fast` is refused, because `FieldRole` has no field defaults.

