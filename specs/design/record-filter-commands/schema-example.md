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

**1. Inferred.** `-R/data/orders.csv/-/ns-rec/to_record-csv/schema` returns this JSON text. It
is the actual output, with one field per line:

```json
{
  "fields": [
    {"name": "order_id",  "label": "order id",  "description": "", "data_type": "Text",      "nullable": false, "key": "None", "role": {"indexed": [], "stored": false, "fast": false}},
    {"name": "placed_at", "label": "placed at", "description": "", "data_type": "Timestamp", "nullable": false, "key": "None", "role": {"indexed": [], "stored": false, "fast": false}},
    {"name": "customer",  "label": "customer",  "description": "", "data_type": "Text",      "nullable": true,  "key": "None", "role": {"indexed": [], "stored": false, "fast": false}},
    {"name": "amount",    "label": "amount",    "description": "", "data_type": "Float",     "nullable": false, "key": "None", "role": {"indexed": [], "stored": false, "fast": false}},
    {"name": "tags",      "label": "tags",      "description": "", "data_type": "Text",      "nullable": false, "key": "None", "role": {"indexed": [], "stored": false, "fast": false}}
  ],
  "type_identifier": null
}
```

Every field always carries all eight attributes, `key` and `role` included. Inference fills in
the type (`placed_at` is recognised as a Timestamp), the nullability (`customer` is empty in
row A-3) and the label. It assigns no key and no role: `FieldRole::default()` means "not indexed,
not stored, not fast". Roles state what the data affords, so inference does not guess them. They
come from a declared schema (case 2) or from the command that built the records (`catalog`
declares `key` as Id and `filename` as full-text).

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

This returns the declared schema back in the same eight-attribute JSON form as case 1, with
defaults filled in: `label: order id`, `description: ""`, `nullable: true` where omitted,
`role: {indexed: [], stored: false, fast: false}` for `tags`, and the Id's role
`{indexed: [Exact], stored: true, fast: false}`. For `customer`, for example:

```json
{"name": "customer", "label": "Customer name", "description": "", "data_type": "Text", "nullable": true, "key": "None",
 "role": {"indexed": ["Exact", {"FullText": {"analyzer": "Simple", "positions": true}}], "stored": true, "fast": false}}
```

That `{"FullText": …}` map is exactly what cannot be linked back today (see Defects). It is the form that
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

