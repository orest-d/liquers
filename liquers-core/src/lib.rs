//!
//! # Liquers Core
//! 
//! Liquers core difines the essential components of a liquers implementation
//! 
//! ## Glossary
//! 
//! **[Key](crate::query::Key)** - an indentifier of a resource. In the simplest case [key](crate::query::Key) is a path to a file.
//! Key consist of names separated by '/'. More generally, key identifies a resource in a store (see [store](crate::store)),
//! which is an abstraction of a file-system capable of storing metadata.
//! Key can only point to a resource in a store, not to any physical file in the file-system,
//! which provides a layer of safety, preventing access to arbitrary files.
//! Key construction only allows certain patterns to ensure validity.
//! This restrictions exist to allow the key to safely coexist in the [query](crate::query::Query),
//! not clashing with the syntax of the query language. Also prevent clashes with the metadata stored physically
//! in the underlying filesystem.
//!
//! A key can be converted to a query.
//! 
//! **[Query](crate::query::Query)** - describes a sequence of steps in a pipeline.
//! One of the most important concepts in Liquers (see [query](crate::query::Query)).
//! Query can be evaluated resulting in a state.
//! 
//! **[Value](crate::value::ValueInterface)** - *1st layer of value encapsulation*: the basic data unit implementing [value interface](crate::value::ValueInterface).
//! Typically an enum that can be of various types. It defines basic operations like serialization and deserialization.
//! 
//! **[Metadata](crate::metadata::Metadata)** - Describes anything useful associated with a given value:
//! e.g. how it was created, title, description, log. Metadata also provides [status](crate::metadata::Status)
//! whether the value has been successfully produced or if there was an errot.
//! See [metadata](crate::metadata).
//!
//! **[State](crate::state::State)** - *2nd layer of value encapsulation*: basically a tuple of a value and metadata.
//! This is what is passed along the pipeline.
//! See [state](crate::state).
//!
//! **[Asset](crate::assets2)** - *3rd layer of value encapsulation*: represent a *[State](crate::state::State)* in making. It may be a recipe being executed or a ready value.
//! A requests to execute a query or fetch a resource results in a asset reference
//! that serves as a handle. In a simples case, asset reference can be used to fetch a result.
//! It can also be used to receive notifications of asset events and to poll asset state.
//! Asset resource (asset identified by a key) is typically shared and asset guarantees that
//! proper sharing via a read-write lock. Assets are accessed via a [AssetManager](crate::assets2::AssetManager).
//! AssetManager can be considered as a key-value store and cache for states and eventually their
//! binary representation.
//!
//! **Resource** - is a state identified by a key. It is typically stored in a store (see [store](crate::store)).
//! There is no special object representing a resource, but in the documentation it is often refered
//! to resources as an value, state or asset identified by a key.
//! 
//! **[Recipe](crate::recipes2::Recipe)** - A high level procedure ('recipe') how to create certain state. Recipes are typically defined in recipe files
//! organized in folders. Recipe in its simple form is a query. (see [query])
//! Besides a query, recipe allows to document the resource, providing a title and description.
//! Recipes may reside in a hierarchycal filesystem-like structure maintained by an asset manager.
//! 
//! **[Plan](crate::plan::Plan)** - a sequence of specific instructions that can be interpreted and resulting in a State.
//! A query or recipe can be compiled into a plan. This happens internally plan normally does not
//! need to be created explicitly. Plan is analogous to an execution plan in a database.
//!
//! **[Store](crate::store::AsyncStore)** - is a storage abstraction able to store binary data and metadata indexed by keys ([Key](crate::query::Key)).
//! It can be considered as a safe abstraction over a file system with some extra features.
//!  
//! **[Asset manager](crate::assets2::AssetManager)** - is a repository of assets.
//! It can be seen as an extension built on top of a store. Like store, asset manager can access physical files,
//! but besides that, asset manager can contain assets created on demand (represented by recipes).
//! Asset manager takes care of execution of the assets in a job queue and tracks the progress.
//! Asset manager also provides caching of the assets and creation/execution of ad-hoc assets (e.g. user queries and 'apply' operations).
//!  
//! **[Environment](crate::context2::Environment)** - a global environment representing a collection of services needed to evaluate queries and recipes,
//! e.g. store, asset manager, command metadata registry, etc.
//! Environment is common for all users
//! 
//! **[Session](crate::context2::Session)** - user session: environment (common for all users), user data, session data.
//! 
//! **[Context](crate::context2::Context)** - a context of creation of a resulting value. Context has a reference to
//! environment and a reference to the asset being created.
//! Context provides services to the command, e.g. log, progress messages and metadata.
//! Context is the mean of communicating to the asset (and thus all clients having an asset reference)
//! during the creation of the asset. The communication is performed using channels, thus the context
//! acts as an interface from blocking commands to an asynchronous environment of assets.
//! 
//! **[Command](crate::commands2)** - is a step in the transformation pipeline. It is basically a function that takes
//! a state as an argument and returns another state (or error). It can also take additional parameters.
//! When executed, it has an access to a context. An command with all the parameters is called and **action**.
//! Command is described by command metadata (see [CommandMetadata](crate::command_metadata::CommandMetadata))
//! that are registered in a [CommandMetadataRegistry](crate::command_metadata::CommandMetadataRegistry).
//! Command metadata contain a lot of details including argument types, documentation and eventually even description
//! of a basic user interface.
//! En executable code of the command is registered in a [CommandExecutor](crate::commands::CommandExecutor).
//! Before a command can be used, it must be registered in the command executor and command metadata registry.
//! In the registries commands are identified by by a [command key](crate::command_metadata::CommandKey).
//! This can be done by a macro (see [liquers_macro]).
//! Commands can be synchronous or asynchronous.
//!
//! **Command namespace** - a group of commands. It serves a similar purpose as a module.
//! Namespaces do not form a hierarchycal structure.
//! Command is searched in active name spaces, formed by default namespaces and namespace selected by a "ns" instruction.
//! 
//! **Realm** - a high level grouping of commands. Currently only a single realm is supported.
//! Realms can (in principle) be used if multiple environments need to be accessed from a single query.
//! An example could be a client and server realms - executing a query on the server and postprocessing the result on a client.
//! Another example could be a backend and frontend realm - backend may have access to computation services (e.g. GPU) but not the graphics;
//! frontend may have access to graphics but not to the computation commands.
//! Realms are not well supported yet. To support a realm, it is necessary to implement a plan interpreter
//! that would be able to deal with the realms.
//! 
//! **Action** - a command with parameters; and element of a query.
//! Action can be represented as a [action request](crate::query::ActionRequest) inside a query,
//! which can be compiled into an [Action step](crate::plan::Step::Action) inside a plan.
extern crate serde;
#[macro_use]
extern crate serde_derive;

pub mod cache;
pub mod command_metadata;
#[macro_use]
pub mod commands;
pub mod commands2;
pub mod context;
pub mod context2;
pub mod error;
pub mod interpreter;
pub mod interpreter2;
pub mod metadata;
pub mod parse;
pub mod plan;
pub mod query;
pub mod state;
pub mod store;
pub mod value;
pub mod media_type;
pub mod recipes;
pub mod recipes2;
pub mod assets;
pub mod assets2;
pub mod icons;
pub mod dependencies;