---
title: Recipes, assets and caching
status: ready
sources: [reference/ASSETS.md, reference/ASSET_LIFECYCLE.md, reference/RECIPES_AND_PLANS.md, guides/DEPENDENCY_CONSISTENCY_GUIDE.md, reference/ENVIRONMENT_CONFIG.md, guides/PYTHON_GUIDE.md]
reviewed: 2026-11-09
---
# 7. Recipes, assets and caching

PURPOSE: The reader knows when a result is computed, cached, reused and recomputed, can watch and cancel a computation, and can name results with recipes.

Evaluating a query returns a value. Behind that simple call, Liquers does the things that make
the value dependable: it keeps the result so it is not computed twice, it remembers what the
result was made from, and it computes the result again when that has changed. The first four
sections explain these ideas in the words shared by all three guides. Sections 7.5 to 7.9 show
them at work in Python.

{{> ../_shared/assets.md}}

## 7.5 Seeing the cache work

A counter shows when a command really runs:

```python
import liquers as lq

env = lq.Environment()
runs = []

@env.command
def shout(text) -> str:
    runs.append(1)
    return text.upper()

env.store_set("data/note.txt", "milk")

assert env.evaluate("-R/data/note.txt/-/shout") == "MILK"
assert env.evaluate("-R/data/note.txt/-/shout") == "MILK"
assert len(runs) == 1                   # the second call reused the asset

env.store_set("data/note.txt", "bread")
assert env.evaluate("-R/data/note.txt/-/shout") == "BREAD"
assert len(runs) == 2                   # the input changed, so it ran again
env.shutdown()
```

Nothing in this program mentions a cache. The second evaluation found the asset of the first.
Writing a new note made that asset out of date, because the asset had recorded that it was made
from `data/note.txt`.

A result is also computed again when the command that made it changes. Liquers records a
fingerprint of each Python command, taken from its code. Edit the function, and the results
that the old version made, stored ones included, are no longer used.

The dependencies are in the log of the metadata, one line each, so you can see what a result
was made from:

```python
import liquers as lq

env = lq.Environment()

@env.command
def shout(text) -> str:
    return text.upper()

env.store_set("data/note.txt", "milk")
state = env.evaluate_state("-R/data/note.txt/-/shout")
for entry in state.metadata.log():
    print(entry.message)
env.shutdown()
```

## 7.6 Commands that must not be cached

Reuse is wrong for a command whose result should be different each time. Declare it volatile:

```python
import random
import liquers as lq

env = lq.Environment()

@env.first_command(volatile=True)
def dice() -> int:
    return random.randint(1, 6)

rolls = {env.evaluate("dice") for _ in range(50)}
assert len(rolls) > 1                   # evaluated every time
env.shutdown()
```

Volatility spreads: every query that contains `dice` is volatile too, since its result depends
on the roll.

`cached=False` is a different thing. It is for a command that is cheap and whose result is not
worth the memory, such as taking the first rows of a large table. Its result is not kept, but it
is not wrong to reuse.

## 7.7 Watching and cancelling

`evaluate` waits for the value. For a long computation you may want the asset itself, at once.
`get_asset` returns an `lq.AssetRef`:

```python
import time
import liquers as lq

env = lq.Environment()

@env.first_command
def slow(context) -> str:
    for _ in range(200):
        if context.is_cancelled():
            return "stopped"
        time.sleep(0.01)
    return "done"

asset = env.get_asset("slow")           # returns at once
print(asset.status)                     # for example Submitted or Processing
assert asset.get() == "done"            # waits for the value
assert asset.status == "Ready"
env.shutdown()
```

| `AssetRef` | Gives |
|---|---|
| `status` | the status now, an `lq.Status`; it compares equal to its name |
| `metadata` | the metadata now, with the log so far |
| `get()` | the value, waiting for it if necessary; raises the error if the evaluation failed |
| `get_state()` | the same, as an `lq.State` |
| `cancel()` | asks the computation to stop |

Cancelling is cooperative. `cancel()` marks the asset as cancelled, and a command finds out by
calling `context.is_cancelled()`. A command that never asks runs to its end, and its result is
thrown away. An async command is interrupted at its next `await` (chapter 5).

While `evaluate` or `get()` waits, other Python threads keep running, and Ctrl-C interrupts the
wait with `KeyboardInterrupt`. Interrupting a wait does not stop the computation; `cancel()`
does.

A failed evaluation is not final. The error is kept with the asset, and the next request for the
same query computes it again. So after you fix a command or an input, you simply ask again.

## 7.8 Recipes in practice

Section 7.4 introduced `recipes.yaml`. Here is the whole cycle with a store on disk: a recipe
names a result, the first request computes and stores it, and the file is there afterwards.

```python
import pathlib
import tempfile
import time
import liquers as lq

root = pathlib.Path(tempfile.mkdtemp())
(root / "data").mkdir()
(root / "data" / "sales.csv").write_text("region,month,amount\nEU,jan,120\nUS,jan,80\nEU,feb,95\n")
(root / "data" / "recipes.yaml").write_text("""
recipes:
  - query: -R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/eu.csv
    title: Sales in the EU
  - query: -R/./sales.csv/-/ns-rec/to_record/nrows/count.txt
    title: Number of sales
""")

env = lq.Environment(config={
    "store": {"stores": [{"type": "filesystem", "prefix": "data", "config": {"path": str(root)}}]},
})

assert not (root / "data" / "eu.csv").exists()          # not made yet
assert env.evaluate("-R/data/count.txt") == 3
assert len(env.evaluate("-R/data/eu.csv")) == 2

time.sleep(0.5)                                         # the file is written in the background
print((root / "data" / "eu.csv").read_text())           # now a real CSV file
assert "eu.csv" in env.store.listdir("data")
env.shutdown()
```

The result was written in the format of its file name: `eu.csv` is a CSV file that any other
program can read. Liquers writes it just after it has handed you the value, in the background,
which is why the example waits a moment before it looks at the disk. A program that ends, or
calls `shutdown()`, immediately after an evaluation can lose that write; the result is then
computed again the next time. The second recipe uses a relative key, `-R/./sales.csv`, which means "the
`sales.csv` in the folder of this `recipes.yaml`".

`env.recipes` answers questions about recipes without evaluating anything:

```python
import liquers as lq

env = lq.Environment()
env.store.set("data/recipes.yaml", b"recipes:\n  - query: hello/greeting.txt\n    title: A greeting\n")

@env.first_command
def hello() -> str:
    return "hello"

recipe = env.recipes.recipe("data/greeting.txt")
assert recipe.query == "hello/greeting.txt"
assert recipe.title == "A greeting"
assert env.recipes.can_make("data/greeting.txt")
assert env.recipes.recipe_opt("data/other.txt") is None
assert env.evaluate("-R/data/greeting.txt") == "hello"
env.shutdown()
```

## 7.9 Recipes from Python

A `recipes.yaml` lists its results one by one. When the names follow a pattern instead, "for
every file `x.csv` there is a summary `x.summary.txt`", write a *recipe provider*: an object
whose `recipe_opt(key)` returns the recipe for a key, or `None` when the key is not its
business. The recipe is an `lq.Recipe` or just the text of a query.

As with a store, you register a factory under a type name. The `recipes` list of the
configuration then names that type, and the other settings of the entry are passed to the
factory:

```python
import liquers as lq

class Reports:
    def __init__(self, folder):
        self.folder = folder
    def recipe_opt(self, key):
        if str(key.parent()) != self.folder:
            return None                                   # not mine: answer quickly
        if key.filename() == "fresh.txt":
            return lq.Recipe("hello", volatile=True)
        return f"hello/{key.filename()}"

env = lq.Environment(config={"recipes": ["recipes_yaml", {"type": "reports", "folder": "reports"}]})
env.register_recipe_provider_factory("reports", Reports)

@env.first_command
def hello() -> str:
    return "Hello"

assert env.evaluate("-R/reports/summary.txt") == "Hello"
assert env.recipes.recipe_opt("elsewhere/x.txt") is None
env.shutdown()
```

The `recipes` setting lists where recipes come from. Left out, it means `recipes.yaml` files and
table manifests (chapter 9). A list replaces that default, which is why the example names
`recipes_yaml` as well as its own type. A key may have only one recipe: if two entries of the
list both answer for a key, that is a `ConfigurationError` which names both.

| `recipes` | Recipes come from |
|---|---|
| left out, or `"default"` | `recipes.yaml` files and manifests |
| `["recipes_yaml"]` | `recipes.yaml` files only |
| `["manifest"]` | manifests only |
| `[]`, `"none"` | nowhere: `-R/` reads only what is stored |
| a list with `{"type": …}` entries | the listed sources, including your own |

A provider may be asked often, so `recipe_opt` should answer quickly for keys that are not its
own. It may be an `async def`. If its results should show up when a folder is listed, give it a
method `assets_with_recipes(folder)` that returns the names it offers in that folder.

`lq.Recipe(query, title="", description="", *, cwd=None, volatile=False, expires=None)` builds a
recipe; `expires` is an `lq.Expires`.

## 7.10 Settings that change caching

The `assets` section of the configuration tunes how results are kept. The defaults suit
interactive work. The settings you are most likely to change:

| Setting | Values | Meaning |
|---|---|---|
| `job_capacity` | a number; 4 by default | how many commands run at the same time (chapter 8) |
| `query_cache_strategy` | `all`, `result`, `none` | what is kept of a query you evaluate: the result and its intermediate steps, the result only, or nothing |
| `recipe_cache_strategy` | `all`, `result`, `none` | the same for results made by recipes |
| `dependency_audit` | `explicit`, `on_load` | how carefully a stored result is checked after a restart; `on_load` checks everything it was made from before using it |

```python
import liquers as lq

env = lq.Environment(config={"assets": {"query_cache_strategy": "result"}})
assert env.evaluate("commands_doc").startswith("# Commands")
env.shutdown()
```

With `explicit`, the default, a restarted program trusts the results it finds in the store until
something touches their inputs. That is fast, and right for exploration. A service that must
never serve an out-of-date result uses `on_load`.
