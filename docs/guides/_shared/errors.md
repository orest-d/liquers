EDITOR: shared fragment (errors, the same in every guide), included by chapter 15 of all three guides. Keep it language-neutral.

## 15.1 Kinds of errors

Every error Liquers reports has a *type*, and the type tells you which stage of the work failed.
A query goes through the same stages every time: it is parsed, then planned against the
registered commands, then evaluated. Reading the type first saves most of the search.

| Stage | Type | Meaning |
|---|---|---|
| Parsing | `ParseError` | The text is not a valid query or key. The error carries the position |
| Planning | `ActionNotRegistered` | No command of that name in the namespaces the query searches |
| Planning | `TooManyParameters` | An action gives more arguments than its command declares |
| Planning | `ArgumentMissing` | A required argument has no value and no default |
| Planning | `ParameterError` | An argument's value does not fit: not a number where one is declared, an unknown argument name |
| Evaluating | `ExecutionError` | A command failed. The message is the command's own |
| Evaluating | `ConversionError` | A value is not of the type a command or a caller needs |
| Evaluating | `SerializationError` | A value cannot be written in, or read from, the requested format |
| Evaluating | `Cancelled` | The evaluation was stopped |
| Store | `KeyNotFound` | The store has nothing at that key |
| Store | `AssetNotFound` | Nothing is stored at that key and no recipe makes it |
| Store | `KeyNotSupported` | No store accepts that key, or the store cannot do the operation (a write to a read-only store) |
| Store | `KeyReadError`, `KeyWriteError` | The store failed while reading or writing |
| Store | `KeyNotAbsolute` | A key with `.` or `..` reached a store unresolved |
| Setup | `ConfigurationError` | The configuration is inconsistent, for example two recipes for one key |
| Setup | `NotSupported`, `NotAvailable` | The feature is not part of this build, or not usable here |
| Any | `General`, `UnexpectedError` | Everything else. `UnexpectedError` suggests a defect in Liquers; please report it |

## 15.2 Reading an error

An error carries more than its message:

- the **type**, from the table above;
- the **message**;
- the **query** that failed, when the error belongs to one. In a query built of several steps
  this is the step, not the whole query, so it shows how far the evaluation got;
- the **position** in the query text, for errors found while parsing and planning;
- the **key**, for errors about stored data.

Three habits make errors short-lived.

**Read the position.** A parse or planning error points at a character of the query. A surplus
argument, a misspelled command and a missing `/-/` are all found this way.

**Check the query without running it.** Parsing and planning need no data and run no command, so
a query can be checked the moment it is written. Each guide shows how; the result also shows what
the query *means*, which is the way to catch a query that is valid but not what you intended.

**Look at the log.** The metadata of an asset holds a log: the messages its commands wrote, the
warnings Liquers added, and the error that ended it. For a failed asset the log is kept with the
error, so you can read it after the fact, and for a result that looks wrong it often says why,
for example that an argument was converted or that a search expression was malformed.

A failed asset is not retried on its own. Ask for it again after fixing the cause, a command or
an input, and it is computed again.
