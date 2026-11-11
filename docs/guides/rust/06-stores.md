---
title: Stores
status: ready
sources: [reference/STORE_SEMANTICS.md, guides/STORE_CONFIG_GUIDE.md, guides/STORE_IMPLEMENTATION_GUIDE.md, guides/STORE_FACTORY_GUIDE.md, reference/ENVIRONMENT_CONFIG.md]
reviewed: 2026-11-10
---
# 6. Stores

PURPOSE: The reader can configure where Liquers reads and writes data, use a store from Rust, knows what a store guarantees, and can write a store of their own.

The *store* is where Liquers keeps data: the files your queries start from, and the results it
saves. This chapter starts with the trait behind every store, then puts the data on disk and in
the cloud through a configuration, and ends with a store you write yourself.

## 6.1 Keys, data and metadata

A store looks like a tree of folders and files. Each file is addressed by a *key*, a path such
as `data/2026/sales.csv`. A key is not a path of your operating system. It always uses `/`, it
never starts with one, and where the file physically is depends on the configuration: under some
directory, in a bucket, or in memory.

For each key the store holds two things: the *data*, a sequence of bytes, and the *metadata*,
which describes the data: its format and media type, its status, and, for a result that Liquers
computed, the query that made it and what it depended on. Because the metadata is kept beside
the data, a stored result is still a full asset after the program restarts.

Every store implements one trait, `liquers_core::store::AsyncStore`:

```rust
let store = AsyncMemoryStore::new(&Key::new());
let key = parse_key("data/note.txt")?;
let folder = parse_key("data")?;

store.set(&key, b"remember the milk", &Metadata::new()).await?;

let (data, metadata) = store.get(&key).await?;
assert_eq!(data, b"remember the milk");
assert_eq!(metadata.get_media_type(), "text/plain"); // from the extension of the key
assert!(store.contains(&key).await?);
assert!(store.is_dir(&folder).await?);
assert_eq!(store.listdir(&folder).await?, vec!["note.txt"]);

store.remove(&key).await?;
let error = store.get(&key).await.err().expect("nothing there any more");
assert_eq!(error.error_type, ErrorType::KeyNotFound);
```

| Method | Does |
|---|---|
| `get(&key)` | returns the data and the metadata; `KeyNotFound` when there is none |
| `get_bytes(&key)`, `get_metadata(&key)` | one of the two |
| `set(&key, data, &metadata)` | writes both. Fields the metadata leaves open, such as the media type, are filled in from the key |
| `set_metadata(&key, &metadata)` | writes the metadata only |
| `contains(&key)`, `is_dir(&key)`, `listdir(&key)` | look around; `listdir` returns names, `listdir_keys` whole keys |
| `remove(&key)`, `makedir(&key)`, `removedir(&key)` | remove a file, make and remove a folder |
| `keys()` | every key in the store |
| `is_read_only()` | tells whether the store refuses writes |

All of them are `async`. There is no synchronous store in Liquers: a store may be a bucket on
the other side of the world, and one trait for all stores is what lets a query ignore the
difference.

A store is used through `Arc<dyn AsyncStore>`. `env.get_async_store()` returns the store of an
environment. Reading through the store gives you the bytes exactly as they are stored. Reading
through a query, `env.evaluate("-R/data/note.txt")`, gives you the *value* (section 4.4), and it
does more: if nothing is stored at the key, it looks for a recipe that makes it (chapter 7). The
store only knows what is stored.

## 6.2 Stores from a configuration

`with_async_store` gives the builder one store that you made. More often the store is described
in a *configuration*, a YAML document that names the kinds of storage and where each one is:

```rust
    use liquers_core::environment_config::EnvironmentConfig;
    use liquers_core::store_factory::default_store_factory;
    use liquers_lib::environment::default_environment_builder;

    let io = |e: std::io::Error| Error::from_error(ErrorType::General, e);
    let root = std::env::temp_dir().join(format!("liquers-guide-{}", std::process::id()));
    std::fs::create_dir_all(root.join("data")).map_err(io)?;
    std::fs::write(root.join("data/sales.csv"), "region,amount\nEU,120\nUS,80\n").map_err(io)?;
    std::env::set_var("LIQUERS_DATA", &root);

    let config = EnvironmentConfig::from_yaml(
        "
store:
  stores:
    - type: filesystem
      prefix: data
      config:
        path: ${LIQUERS_DATA}
    - type: memory
      prefix: scratch
",
    )?;
    let mut builder = default_environment_builder::<Value, ()>()
        .with_config(config, Box::new(default_store_factory()));
    register_library(&mut builder.command_registry)?;
    let env = builder.build()?;

    let rows = env.evaluate("-R/data/sales.csv/-/ns-rec/to_record/nrows").await?.get().await?;
    assert_eq!(rows.try_into_string()?, "2");

    let store = env.get_async_store();
    store.set(&parse_key("data/note.txt")?, b"kept", &Metadata::new()).await?;
    assert_eq!(std::fs::read_to_string(root.join("data/note.txt")).map_err(io)?, "kept");
    store.set(&parse_key("scratch/note.txt")?, b"not kept", &Metadata::new()).await?;
    assert!(!root.join("scratch").exists());
    std::fs::remove_dir_all(&root).map_err(io)?;
```

The configuration is a list of *mounts*. Each mount has a `type`, which says what kind of storage
it is, a `prefix`, which says which keys belong to it, and a `config` with the settings of that
type. Here every key that begins with `data/` is a file under the temporary folder, and every
key that begins with `scratch/` lives in memory.

Note where the file is: the key `data/note.txt` is the file `<root>/data/note.txt`. The prefix
is part of the path. Point `path` at the folder that *contains* `data`, not at `data` itself.

Behind the scenes the mounts become one store, an `AsyncStoreRouter`, which sends each key to
the mount whose prefix matches it. Queries do not know about mounts. `-R/scratch/x.csv` and
`-R/data/sales.csv` look the same, and you can move a prefix to other storage without changing a
query.

The name of an environment variable between `${` and `}` stands for the value of that variable. Use variables for paths and
credentials, and never write a secret into the document. A variable that is not set is an error
when the environment is built.

Files that other programs put into the folder are simply there: Liquers reads `sales.csv`
although it has never seen it before. Beside each file that Liquers writes, it keeps a second
file with the metadata, named like the first with `.__metadata__` added.

A mount can carry more settings. `read_only: true` protects it from writes. A `metadata` section
says how the metadata of its files is found when the storage holds plain files that Liquers did
not write: taken from defaults, worked out from the file, or read from the front matter of
Markdown documents.

## 6.3 Kinds of storage

The second argument of `with_config` is a *store factory*: the object that knows how to build a
store of each type. Which types exist is decided when your program is compiled, by the factory
you pass.

| Factory | Types | Crate |
|---|---|---|
| `liquers_core::store_factory::default_store_factory()` | `memory`, `filesystem` | `liquers-core` |
| `liquers_store::store_factory::default_store_factory()` | the two above and the OpenDAL services | `liquers-store` |

`liquers-store` builds its stores on [Apache OpenDAL](https://opendal.apache.org), a library that
speaks to many storage services through one interface. Each service is a feature of the crate,
named `services-<type>`:

| Type | Stores data |
|---|---|
| `fs` | in a local directory, through OpenDAL |
| `s3` | in an S3 bucket, or a compatible service |
| `gcs`, `azblob` | in Google Cloud Storage, in Azure Blob Storage |
| `http`, `https`, `webdav`, `ftp`, `sftp` | on a server |
| `github`, `dropbox`, `onedrive`, `gdrive`, `ipfs` | at the named service |
| `hdfs`, `webhdfs` | in a Hadoop file system |
| `redis`, `mongodb`, `postgresql`, `mysql`, `sqlite` | in a database, one row for each key |

The default features of `liquers-store` include the common services. For another one, enable
its feature:

```toml
liquers-store = { git = "https://github.com/orest-d/liquers.git", features = ["services-sftp"] }
```

The `config` of a mount takes the settings that OpenDAL defines for the service, such as
`bucket`, `region` and `endpoint` for `s3`. A type that is known but was not compiled in is
refused with an error of the type `NotSupported` that says so, which is easier to act on than
"unknown type".

## 6.4 What a store guarantees

Stores differ: a directory, a bucket and a database row do not behave alike. Liquers states what
every store must do, and checks it with a conformance suite that every store of the repository
passes. The points that matter when you rely on a store:

- **What you write is what you read.** `get` after `set` returns the same bytes, and the
  metadata you gave with the gaps filled in.
- **A key is a file or a folder, never both.** Writing `a/b.txt` makes `a` a folder.
- **Keys are absolute.** A store refuses a key with `.` or `..` in it; relative keys are resolved
  before they reach a store (section 5.5).
- **A missing key is `KeyNotFound`,** and an operation the store cannot do is `KeyNotSupported`.
  Other failures are `KeyReadError` and `KeyWriteError`, with the cause attached.
- **Metadata may be missing.** A file that another program wrote has none stored. The store then
  makes it up from what it knows: the key, the size, the media type of the extension.

Stores do not promise transactions. Two programs writing the same key at the same time are not
coordinated, and the later write wins.

## 6.5 Two ways to write

There are two ways to put something at a key, and they differ in an important way.

`store.set(&key, data, &metadata)` writes bytes straight into the store. It changes the store
behind the back of a running environment, which goes on using the values it has already read,
exactly as if another program had changed the file.

`env.get_asset_manager().set_state(&key, state)` writes a *value* through Liquers. The value is
serialized, its type is recorded, and everything that was made from the key becomes out of date.
Section 7.6 shows it.

So the rule is: to *change* data that queries read, write through the asset manager. To *put
files in place* before queries read them, as the examples of this guide do, or to copy bytes
around, the store is fine and gives you control over the bytes.

## 6.6 Writing a store

When your data lives somewhere Liquers has no store type for, such as an internal service or a
database with a schema of its own, write the store yourself. Two methods of `AsyncStore` have no
default: `get` and `set_metadata`. Everything else defaults to "this store cannot do that". So a
store starts small and grows by overriding methods:

```rust
use async_trait::async_trait;
use liquers_core::environment_config::EnvironmentConfig;
use liquers_core::store_factory::{
    default_store_factory, StoreArgumentInfo, StoreArgumentType, StoreTypeInfo, StoreTypeMap,
};
use liquers_lib::environment::default_environment_builder;

/// A read-only store with one file, `<prefix>/hello.txt`.
struct Notes {
    prefix: Key,
    greeting: String,
}

#[async_trait]
impl AsyncStore for Notes {
    fn key_prefix(&self) -> Key {
        self.prefix.clone()
    }

    fn is_supported(&self, key: &Key) -> bool {
        key.has_key_prefix(&self.prefix)
    }

    async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
        key.as_absolute()?;
        if *key != self.prefix.join("hello.txt") {
            return Err(Error::key_not_found(key));
        }
        let data = self.greeting.clone().into_bytes();
        let mut metadata = Metadata::MetadataRecord(self.default_metadata(key, false));
        self.finalize_metadata(&mut metadata, key, &data, false);
        Ok((data, metadata))
    }

    async fn set_metadata(&self, key: &Key, _metadata: &Metadata) -> Result<(), Error> {
        Err(Error::key_not_supported(key, &self.store_name()))
    }

    async fn contains(&self, key: &Key) -> Result<bool, Error> {
        Ok(*key == self.prefix || *key == self.prefix.join("hello.txt"))
    }

    async fn is_dir(&self, key: &Key) -> Result<bool, Error> {
        Ok(*key == self.prefix)
    }

    async fn listdir(&self, key: &Key) -> Result<Vec<String>, Error> {
        Ok(if *key == self.prefix { vec!["hello.txt".to_string()] } else { vec![] })
    }
}
```

The example needs the crate `async-trait`, because the trait has `async` methods and is used as
a trait object. Four things in it are worth copying:

- `key_prefix` and `is_supported` tell the router which keys are the store's.
- `key.as_absolute()?` refuses a relative key at the top of a method.
- `default_metadata` and `finalize_metadata` build the metadata that a file without stored
  metadata should have, so that your store describes its files the way the others do.
- What the store cannot do it leaves to the defaults, which refuse with `KeyNotSupported`.

To make the store available to configurations, describe its type and say how to build it from a
mount. A `StoreTypeMap` is a factory made of such descriptions, and it chains onto the factory
you already use:

```rust
    let notes = StoreTypeMap::new().with_store_type(
        StoreTypeInfo::new("notes")
            .with_label("Notes")
            .with_doc("One greeting, read-only.")
            .with_argument(
                StoreArgumentInfo::new("greeting", StoreArgumentType::String)
                    .with_doc("The text of hello.txt."),
            ),
        Box::new(|config| {
            Ok(Box::new(Notes {
                prefix: config.key_prefix()?,
                greeting: config.require_config_string_expanded("greeting")?,
            }))
        }),
    );
    let factory = default_store_factory().chain(Box::new(notes));

    let config = EnvironmentConfig::from_yaml(
        "
store:
  stores:
    - {type: notes, prefix: notes, config: {greeting: Hello}}
    - {type: memory, prefix: scratch}
",
    )?;
    let mut builder =
        default_environment_builder::<Value, ()>().with_config(config, Box::new(factory));
    register_library(&mut builder.command_registry)?;
    let env = builder.build()?;

    let state = env.evaluate("-R/notes/hello.txt").await?.get().await?;
    assert_eq!(state.try_into_string()?, "Hello");
    let store = env.get_async_store();
    assert_eq!(store.listdir(&parse_key("notes")?).await?, vec!["hello.txt"]);
    let refused = store.set(&parse_key("notes/new.txt")?, b"x", &Metadata::new()).await;
    assert_eq!(refused.err().expect("read-only").error_type, ErrorType::KeyNotSupported);
```

The description of the type, with its arguments and their documentation, is not decoration. A
program can list the store types of a factory and what each needs, which is how a configuration
editor or an error message knows them.

A store that can write overrides `set`, `set_metadata`, `remove` and the folder methods. Before
you rely on such a store, run the conformance suite against it. It lives in `liquers-core`
behind the feature `store-conformance`: you describe what your store can do in a
`StoreCapabilities` value, wrap the store in a `GenericFixture`, and `run_all` reports every rule
your store breaks. A rule you declare your store unable to follow is checked the other way
round: the suite makes sure the store really refuses.

```rust
// Sketch: running the conformance suite against a store.
let fixture = GenericFixture::new(
    "Notes",
    Box::new(store),
    prefix,
    capabilities, // a StoreCapabilities value: every field named, none defaulted
    SafetyLevel::Scratch,
);
let report = run_all(&fixture).await;
eprintln!("{report}");
report.assert_conformant(&[])?;
```
