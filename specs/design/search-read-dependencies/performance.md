# Performance assessment — search-read-dependencies

Written 2026-10-17 at the maintainer's request: does recording search reads, and inheriting a cut
predecessor's records (Q1 a), repeat the `EVALUATING-A-LONG-DEPENDENCY-CHAIN-GETS-SUPER-LINEARLY-SLOW`
incident (`design/dependency-chain-analysis-cost/`), or create other performance problems?

## The incident, and how this differs

The incident was O(n⁴) in recipe-chain length. Four factors multiplied:
- each keyed link recorded everything upstream;
- the expiry pass re-ran a recursive **recipe analysis** for every record;
- three analyses ran per evaluation;
- each recipe lookup re-parsed `recipes.yaml`.

Its fix made records direct: "the nearest addressable dependency". A keyed `GetAsset` stops the
record set. Anonymous `Evaluate` steps pass their dependencies through.

Inheritance (Q1 a) follows that rule and does not reintroduce the incident's factors:
- **It stops at keys.** A keyed predecessor is not inherited, so records do not grow along a
  chain of keyed recipes. A run of *L* anonymous cut steps carries the same set at each step, and
  that set is bounded by what the run reads.
- **It does not drive plan analysis.** Inherited records go to `pending_dependencies` and to
  metadata. `analyze_plan_dependencies` never walks them, so no recipe lookup is made per record.
- **It equals the expanded plan.** With `cut_predecessors: false` the same records already land on
  the dependent.

The cost that does scale is the **number of records per asset**, *N*: one per file a catalog lists.
The incident had a handful of records per link. Here one asset can have thousands, and that exposed
the problems below.

## Measurements

Debug build, cold, one process. Fixture: *N* Markdown files of about 0.8 KB in 10 subfolders.
The chain is `-R-key/docs/-/ns-search/catalog-t-t/search-wombat-f-f-0/search-doc-f-f-0/search-text-f-f-0`
(three cut steps after the catalog). The prototype and the bench are throwaway and not committed.

**Memory store, N = 5000** (`catalog-t-f`, then the 3-step chain over `catalog-t-t`):

| Configuration | catalog | chain, cold | chain, warm | write | recompute after write | records on the chain |
|---|---|---|---|---|---|---|
| HEAD (stale results) | 154 ms | 36 ms | 1.2 ms | 0.2 ms | 1.2 ms (served stale) | 5 |
| Part 1 only (record reads) | 614 ms | 44 ms | 1.6 ms | 2.6 ms | 1.6 ms (still stale) | 5 |
| Part 1 + Q1 a, as prototyped | 570 ms | **819 ms** | 1.9 ms | 2.5 ms | **1.64 s** | 5019 |
| Part 1 + Q1 a, linear merges | 308 ms | **85 ms** | 1.9 ms | 2.6 ms | 522 ms (correct) | 5019 |

At N = 1000 the linear version costs 67 ms for the catalog (HEAD 32 ms), 28 ms for the chain
(HEAD 22 ms) and 92 ms to recompute. At N = 100 the differences are within noise.

**File store (`AsyncFileStore`), N = 5000:**

| Configuration | catalog | keyed index, cold | keyed index after restart |
|---|---|---|---|
| HEAD | 1.80 s | 4.2 s | 6 ms (5 records) |
| Records, listing through `-R-dir/` as designed | 4.55 s | 7.4 s | 1.6 s (5017 records) |
| Records, listing infos reused from `-R-dir/` | 3.15 s | 5.4 s | 1.7 s |
| Records, listing without building infos | 2.48 s | — | 1.6 s |

## Findings

1. **Two quadratic merges (fixed in the prototype).** `Context::add_dependency` and
   `MetadataRecord::add_dependency` each do a linear `find` per insert. Merging *N* records is
   therefore O(N²), and this happens once per asset level. At N = 5000 this was about 260 ms per
   cut step, so inheritance looked expensive when only the merge was. A merge that builds one
   `HashMap` index makes it linear: about 15 ms per step. These are the same kind of hidden
   super-linear costs as in the incident, and the linear merge must ship with Q1 a.
2. **The `-R-dir/` route doubles the info pass.** `Step::GetAssetDirectory` builds an `AssetInfo`
   for every name (`asset_info_of_names`, sequentially). The catalog then reads every info again.
   On a file store that is +150 % for the catalog. Reusing the listing's infos brings it to +75 %.
   A core helper that registers and records a listing without building infos brings it to +38 %.
   See Q3.
3. **One version read per file.** `AssetManager::version` is a `contains` plus a `get_metadata`. On a
   file store this is most of the remaining +38 %. Reading versions concurrently changed little.
   Avoiding the read would need the version in `AssetInfo` (an API change). Not proposed.
4. **Restart cost for a keyed index.** The fast track checks the stored status of every recorded
   dependency, one level deep (`AssetRef::dependency_blocks_fast_track`). That is *N* metadata reads
   the first time a process loads the index: 1.6 s at N = 5000 on a file store, against 6 ms today.
   It is paid once per process per key. Under `OnLoad`, the stored-records walk does the same reads.
5. **Memory grows with the number of distinct queries.** Each non-keyed asset in the chain registers
   a weak reference on each of its *N* dependencies (`DependencyManager::add_dependent_asset`).
   Those lists are only emptied when the dependency changes; dead references are never pruned.
   *Q* distinct searches over *N* files hold about *Q·N·(L+1)* weak references, plus each cached
   asset's *N*-record metadata. For example, 1 000 distinct agent searches over 5 000 files, with
   one search step, is about 10 M references. The unpruned lists predate this design. Recording
   reads makes them large. See Q4.
6. **Writes stay cheap.** A write costs 2.6 ms at N = 5000: one listing refresh plus expiring the
   weak dependents.

## Consequences elsewhere (Q1 a)

- **Every command that records at run time inside a cut predecessor becomes correct.** Today any
  `Context::evaluate` or `get_dependency_state` inside a cut prefix is lost to the dependent. This
  is not specific to search.
- **Links and command-level `Context::evaluate` have the same gap.** `materialize_link_json`
  (`interpreter.rs`) and `Context::get_dependency_state` record only the query of a non-keyed
  dependency, never what that dependency read. Inheriting in `Step::Evaluate` alone leaves them
  stale. The general place for the fix is the point where a non-keyed dependency's state is
  received, `Context::get_dependency_state` (through `wait_for_dependency_recording`). That covers
  `Evaluate`, links and commands in one place, and makes the search-side record copying in
  `listing` unnecessary. Q1 a is refined to this.
- **Keyed results built through a cut persist more records.** The format is unchanged. Audits and
  `OnLoad` then see real dependencies instead of an unresolvable query record, which also makes
  restart freshness work for indexes. The price is finding 4.
- **The plan analysis is unaffected.** It never reads inherited records.
- **Volatility and expiry are unaffected.** They come from the plan analysis, not from records.
- **Existing tests.** All 48 `liquers-core` suites and the `search_commands` suite passed with
  inheritance. No test asserts a cut dependent's exact record list.

## Alternative considered: cascade through query nodes instead of inheriting

Each dependent keeps one record, for the predecessor's query. When a non-keyed asset expires, the
dependency manager also expires the weak dependents registered under that asset's query. Memory
then grows as O(N + L) instead of O(L·N), and links and `Context::evaluate` are covered too. It
fails for keyed results across restarts, though. A persisted index would record only a query, which
an audit cannot resolve, so after a restart it would be served stale until something it read is
written. Inheritance keeps the records that audits need. A hybrid is possible: cascade for
non-keyed dependents, inheritance only into keyed ones. It is more code; listed as Q4's option (b).

## As implemented (Phase 4 step 6)

`bench_search_dependency_cost` in `liquers-lib/tests/search_commands.rs` (`-- --ignored`), run three
times alternately on the commit before the change and on the implementation; debug build, cold, 5 000
files. Its chain runs `catalog-t-t` cold, so it is not comparable with the table above, whose chain
reused a cached catalog.

| | HEAD (3 runs) | Implemented (3 runs) | Median ratio |
|---|---|---|---|
| Memory, `catalog-t-f` | 142, 143, 138 ms | 207, 194, 211 ms | 1.46x |
| Memory, catalog + 3-step chain | 242, 238, 206 ms | 410, 450, 512 ms | 1.89x (limit 3x) |
| Memory, write | 0.3 ms | 2.4–3.2 ms | |
| Memory, recompute after the write | 1.1 ms (stale) | 414–481 ms (correct) | |
| File store, `catalog-t-f` | 1.69, 1.58, 1.73 s | 2.61, 2.53, 2.37 s | 1.54x (limit 1.5x) |

The file-store catalog is at the limit the design set. With the per-file version read disabled it
took 1.89 and 1.92 s (1.1x), so that read is about four fifths of the added cost: it reads the same
metadata `get_asset_info` has just read. Filed as `SEARCH-CATALOG-READS-EACH-FILE-METADATA-TWICE`.
