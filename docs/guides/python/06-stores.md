---
title: Stores
status: ready
sources: [reference/STORE_SEMANTICS.md, guides/STORE_CONFIG_GUIDE.md, guides/PYTHON_GUIDE.md, reference/ENVIRONMENT_CONFIG.md]
reviewed: 2026-11-09
---
# 6. Stores

PURPOSE: The reader can configure where Liquers reads and writes data, read and write it from Python, and write a store of their own.

The *store* is where Liquers keeps data: the files your queries start from, and the results it
saves. This chapter starts with the store you get without doing anything, then puts the data on
disk, and ends with stores you write yourself.

## 6.1 Keys, data and metadata

A store looks like a tree of folders and files. Each file is addressed by a *key*, a path such
as `data/2026/sales.csv`. A key is not a path of your operating system. It always uses `/`, it
never starts with one, and where the file physically is depends on the configuration: under some
directory, in a bucket, or in memory.

For each key the store holds two things: the *data*, a sequence of bytes, and the *metadata*,
which describes the data: its format and media type, its status, and, for a result that Liquers
computed, the query that made it and what it depended on. Because the metadata is kept beside
the data, a stored result is still a full asset after the program restarts.

```python
import liquers as lq

env = lq.Environment()
store = env.store

store.set("data/note.txt", b"remember the milk")

assert store.get("data/note.txt") == b"remember the milk"
assert store.contains("data/note.txt")
assert store.listdir("data") == ["note.txt"]
assert store.is_dir("data")
assert store.get_metadata("data/note.txt").get_asset_info().media_type == "text/plain"

store.remove("data/note.txt")
assert not store.contains("data/note.txt")
env.shutdown()
```

`env.store` is an `lq.Store`. Its methods take a key as text or as an `lq.Key`:

| Method | Does |
|---|---|
| `get(key)` | returns the data as `bytes`; raises `KeyNotFoundError` when there is none |
| `set(key, data, metadata=None)` | writes data; the format is taken from the key's extension unless the metadata names one |
| `get_metadata(key)`, `set_metadata(key, metadata)` | reads and writes the metadata |
| `contains(key)`, `is_dir(key)`, `listdir(key)` | look around |
| `remove(key)`, `makedir(key)`, `removedir(key)` | remove a file, make and remove a folder |
| `is_read_only()` | tells whether the store refuses writes |

Each method that touches data has a form to `await`, with `_async` added to its name:
`await env.store.get_async(key)`.

Reading through the store gives you the bytes exactly as they are stored. Reading through a
query, `env.evaluate("-R/data/note.txt")`, gives you the *value*: text for a text file, parsed
data for JSON (chapter 4). The query also does more: if nothing is stored at the key, it looks
for a recipe that makes it (chapter 7). The store only knows what is stored.

## 6.2 A store on disk

Without configuration, an environment keeps everything in memory, and all of it is gone when
the program ends. To keep data, describe the store in the `config` of the environment:

```python
import pathlib
import tempfile
import liquers as lq

root = pathlib.Path(tempfile.mkdtemp())            # in practice: your project's data folder
(root / "data").mkdir()
(root / "data" / "sales.csv").write_text("region,amount\nEU,120\nUS,80\n")

env = lq.Environment(config={
    "store": {"stores": [
        {"type": "filesystem", "prefix": "data", "config": {"path": str(root)}},
    ]},
})

assert env.evaluate("-R/data/sales.csv/-/ns-rec/to_record/nrows") == 2

env.store.set("data/note.txt", b"kept")
assert (root / "data" / "note.txt").read_text() == "kept"
env.shutdown()
```

The configuration is a list of *mounts*. Each mount has a `type`, which says what kind of storage
it is, a `prefix`, which says which keys belong to it, and a `config` with the settings of that
type. Here every key that begins with `data/` is a file under `root`.

Note where the file is: the key `data/note.txt` is the file `<root>/data/note.txt`. The prefix
is part of the path. Point `path` at the folder that *contains* `data`, not at `data` itself.

Files that you put into the folder with other programs are simply there: Liquers reads
`sales.csv` although it has never seen it before. Beside each file that Liquers writes, it keeps
a second file with the metadata, named like the first with `.__metadata__` added.

## 6.3 Several mounts

A configuration can have as many mounts as you like, and Liquers sends each key to the mount
whose prefix matches it:

```yaml
store:
  stores:
    - type: filesystem
      prefix: data
      config:
        path: /srv/project
    - type: filesystem
      prefix: reference
      read_only: true
      config:
        path: /srv/shared
    - type: memory
      prefix: scratch
```

Queries do not know about mounts. `-R/reference/countries.csv` and `-R/data/sales.csv` look the
same, and you can move a prefix to other storage without changing a query. `read_only: true`
protects a mount from writes.

The types you will use most:

| Type | Stores data | Settings |
|---|---|---|
| `memory` | in the process, until it ends | none |
| `filesystem` | in a local directory | `path` |
| `s3` | in an S3 bucket, or a compatible service | `bucket`, `region`, `endpoint`, credentials |
| `gcs`, `azblob` | in Google Cloud Storage, in Azure Blob Storage | `bucket` or `container`, credentials |
| `http`, `https`, `ftp`, `sftp`, `webdav` | on a server | `endpoint` |

`env.store_types()` returns the full list for your installation, with the settings of each type
and whether it is available.

## 6.4 Configuration in a file

A configuration is usually kept in a YAML file, apart from the code. `config` accepts the YAML
text as well as a dictionary. In YAML, the name of an environment variable
between `${` and `}` stands for the value of that variable, as with `LIQUERS_DATA` below, so paths
and credentials can stay out of the file:

```python
import os
import tempfile
import liquers as lq

os.environ["LIQUERS_DATA"] = tempfile.mkdtemp()
config_text = """
store:
  stores:
    - type: filesystem
      prefix: data
      config:
        path: ${LIQUERS_DATA}
"""
# In practice: config_text = pathlib.Path("liquers.yaml").read_text()
env = lq.Environment(config=config_text)
env.store.set("data/note.txt", b"kept")
env.shutdown()
```

`config` is the text, not the path of the file. Never write a secret into the file; refer to a
variable.

Two kinds of mistakes are reported at different times. Malformed YAML is refused at once, with
`ParseError`. A store that cannot be built, for example one of an unknown type, is reported by
the first use of the environment, because that is when the store is built. A setting Liquers
does not know is ignored, so check the spelling of a setting that seems to have no effect.

## 6.5 Two ways to write

There are two ways to put something at a key, and they differ in an important way.

`env.store.set(key, data)` writes bytes straight into the store. `env.store_set(key, value)`
writes a *value* through Liquers: the value is converted and its type recorded, and Liquers
learns that the key has changed.

```python
import liquers as lq

env = lq.Environment()

@env.command
def shout(text) -> str:
    return text.upper()

env.store_set("data/note.txt", "milk")
assert env.evaluate("-R/data/note.txt/-/shout") == "MILK"

env.store_set("data/note.txt", "bread")                      # through Liquers
assert env.evaluate("-R/data/note.txt/-/shout") == "BREAD"   # computed again

env.store_set("data/limits.json", {"low": 1, "high": 9})     # any value Liquers can store
assert env.evaluate("-R/data/limits.json") == {"low": 1, "high": 9}
env.shutdown()
```

The second `store_set` made the first result out of date, so the query was computed again. A
write with `env.store.set` would not have done that. It changes the store behind the back of the
running environment, which goes on using the value it has already read, exactly as if another
program had changed the file.

So the rule is:

- To *change* data that queries read, use `env.store_set`.
- To *put files in place* before queries read them, as the examples of this guide do, or to copy
  bytes around, `env.store.set` is fine and gives you control over the bytes.

One more difference concerns the format. `store.set("data/sales.csv", data)` records the data as
CSV, because of the extension of the key. `store_set` records what the value is: `bytes` are
recorded as plain bytes whatever the key says. A command that reads a table from them then needs
to be told the format, as in `ns-rec/to_record-csv`.

## 6.6 A store written in Python

When your data lives somewhere Liquers has no store type for, such as an internal service or a
database, write the store yourself. A store is any object with a `get(key)` method that returns
`bytes`, or `None` when it has nothing at that key.

You register a *factory* for it under a type name. From then on a configuration can use that
type like a built-in one. The settings of the mount are passed to the factory as keyword
arguments, and a parameter named `prefix` receives the mount's prefix:

```python
import liquers as lq

class Notes:
    """A read-only store kept in a dict."""
    def __init__(self, prefix, greeting: str = "Hi"):
        self.files = {f"{prefix}/hello.txt": greeting.encode()}
    def get(self, key):
        return self.files.get(str(key))

env = lq.Environment(config={"store": {"stores": [
    {"type": "notes", "prefix": "notes", "config": {"greeting": "Hello"}},
    {"type": "memory", "prefix": "scratch"},
]}})
env.register_store_factory("notes", Notes, doc="Notes kept in a dict")

assert env.evaluate("-R/notes/hello.txt") == "Hello"
assert env.store.get("notes/hello.txt") == b"Hello"
try:
    env.store.set("notes/new.txt", b"x")          # Notes has no `set`
except lq.KeyNotSupportedError:
    pass
env.store.set("scratch/x.txt", b"written")        # the memory mount takes it
env.shutdown()
```

Register the factory before the environment is first used.

A store grows by adding methods. Each may be a `def` or an `async def`, each receives the key as
an `lq.Key`, and Liquers uses whichever are there:

| Method | Gives the store | Without it |
|---|---|---|
| `get(key)` returning `bytes`, `(bytes, metadata)` or `None` | reading | required |
| `listdir(key)` returning names, and `is_dir(key)` | folders that can be listed | the store is flat |
| `set(key, data, metadata)`, `remove(key)` | writing | the store is read-only; a write raises `KeyNotSupportedError` |
| `makedir(key)`, `removedir(key)`, `set_metadata(key, metadata)` | folders and metadata of their own | worked out from the others |
| `get_metadata(key)`, `contains(key)` | a cheaper answer | worked out from `get` |
| `is_supported(key)` (a plain `def`) | refusing some keys | every key under the prefix is accepted |

Metadata can be an `lq.Metadata`, a `dict` or `None`. When it does not name a media type, the
type is taken from the key's extension. To report an error, raise an `lq.LiquersError` such as
`lq.KeyNotFoundError`; any other exception reaches the caller as `KeyReadError` or
`KeyWriteError` with your traceback.

To try a store without an environment, wrap the object directly:

```python
import liquers as lq

class Flat:
    def get(self, key):
        return b"one" if str(key) == "a/one.txt" else None

store = lq.Store.from_object(Flat())
assert store.get("a/one.txt") == b"one"
assert store.contains("a/one.txt") and not store.contains("a/two.txt")
assert store.is_read_only()
store.close()
```
