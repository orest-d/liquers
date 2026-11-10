EDITOR: shared fragment (assets and recipes, the same in every guide), included by chapter 7 of all three guides. Keep it language-neutral: queries and YAML only, no host-language code.

## 7.1 What an asset is

When you evaluate a query, Liquers does not just call some functions and hand you the result. It
creates an *asset*: the value, together with a record of how it is doing and how it was made. An
asset has three parts:

- the **value**, once there is one;
- a **status**, which says where the asset is in its life: waiting, being computed, ready, failed;
- **metadata**: the query that made it, the type of the value, a log of what happened while it
  was computed, and the other assets it was made from.

The asset exists before its value does. That is what lets you ask for a result that takes a
minute, look at its status and its log while it runs, and cancel it. It is also what makes caching
safe: an asset knows what it depends on, so Liquers can tell whether it is still valid.

Stored files and computed results are both assets, and they share one space of names. When you
ask for `-R/data/report.csv`, you do not need to know whether that file was uploaded by somebody
or is computed on demand. Liquers finds it, reuses it, or makes it, and the metadata records
which.

## 7.2 The life of an asset

The status of an asset changes as it is evaluated. These are the statuses you will see:

| Status | Meaning |
|---|---|
| `Recipe` | Liquers knows how to make the value, but has not made it yet |
| `Submitted` | Waiting in the queue for a free slot |
| `Dependencies` | Waiting for other assets it needs |
| `Processing` | Being computed |
| `Partial` | Being computed, with a preliminary value already available |
| `Ready` | Computed; the value is available |
| `Error` | The computation failed; the metadata holds the error |
| `Cancelled` | The computation was stopped before it produced a value |
| `Expired` | There is a value, but it is stale |
| `Source` | The value was put there from outside; nothing computes it |
| `Override` | A value put there from outside, in place of the one a recipe would compute |
| `Directory` | A folder of the store |

A typical evaluation goes `Submitted`, `Processing`, `Ready`. A failed one ends in `Error`, and
asking for its value raises that error.

## 7.3 Caching and dependencies

An asset that is `Ready` is reused. Ask for the same query again and you get the same asset back,
without running any command. The query is the cache key, so there is nothing to configure and no
key to invent.

Reuse works on parts of a query too. A query is a path, and every prefix of it is a query of its
own:

```text
-R/data/sales.csv/-/ns-pl/from_csv
-R/data/sales.csv/-/ns-pl/from_csv/eq-region-EU
-R/data/sales.csv/-/ns-pl/from_csv/eq-region-US
```

The second and the third query share their beginning with the first one. Liquers can compute
that common part once and use it for both.

A cached value must not outlive the things it was made from. While an asset is computed, Liquers
records its *dependencies*: the file the query read, the assets a command asked for, and the
version of each command that ran. When one of them changes, the asset becomes `Expired` and is
computed again the next time somebody asks for it. So if you write a new `data/sales.csv` through
Liquers, every result made from it is recomputed on demand, and nothing else is.

Three more things end an asset's validity:

- **Volatile commands.** A command whose result is different every time, such as the current
  time or a random number, is declared *volatile*. A query that contains one is never reused.
- **Expiration.** A command or a recipe can say how long its result stays valid: `in 5 min`,
  `eod` (end of day), `at 18:00`.
- **A change of the command itself.** A command can carry a version. Results made by an older
  version are recomputed.

One limit is worth knowing. Liquers sees the changes that go through it. A file edited by another
program while Liquers is running is noticed when Liquers next reads it and finds that its content
no longer matches the recorded version, not at the moment of the edit.

## 7.4 Recipes

A *recipe* gives the result of a query a key in the store, so that it can be read like a file.
Recipes are listed in a file named `recipes.yaml`, placed in the folder where the results should
appear:

```yaml
recipes:
  - query: -R/data/sales.csv/-/ns-pl/from_csv/eq-region-EU/eu.csv
    title: Sales in the EU
  - query: -R/data/sales.csv/-/ns-pl/from_csv/describe/summary.csv
    title: Summary of all sales
    expires: eod
```

With this file at `data/recipes.yaml`, the folder `data` holds two more assets, `data/eu.csv` and
`data/summary.csv`. The name of each comes from the file name at the end of its query. Nothing is
computed when the file is read. The first request for `-R/data/eu.csv` runs the query, stores the
result, and serves it; later requests reuse it, until `data/sales.csv` changes.

This is the idea of `make`, applied to data: you describe what a folder should contain and how
each file is made, and Liquers produces the files on demand and keeps them up to date. It also
means that a reader of `data/eu.csv` needs to know nothing about the query behind it, and that
you can change the query without changing any reader.

The fields of a recipe:

| Field | Meaning |
|---|---|
| `query` | The query to run. Its file name is the name of the result. Required |
| `title`, `description` | Text for people, shown where the asset is listed |
| `volatile` | `true` when the result must be computed on every request |
| `expires` | How long the result stays valid, for example `in 1 h`, `eod`, `never` |
| `stored` | `false` to keep the result out of the store; it is then computed again after a restart |
| `cached` | How much is kept for reuse: `all` (the result and its intermediate steps), `result` or `none` |
| `arguments` | Values for the arguments of the query's last action, by name; useful for long text |

A relative key in a recipe's query, such as `-R/./sales.csv`, is resolved against the folder of
the `recipes.yaml` file, so a folder with its recipes can be moved as a whole.
