"""Asset metadata: status, log, media type and expiration of a value that Liquers made or stored.

Rust counterpart: `liquers_core::metadata` and `liquers_core::expiration`. Every name here is also available directly from `liquers`
(`liquers.Metadata is liquers.metadata.Metadata`); `import liquers as lq` and `lq.Metadata` stays
the usual style.

# Main components
- `Metadata`, `MetadataRecord`, `AssetInfo`: what is known about an asset.
- `Status`: where an asset is in its life cycle (`Ready`, `Error`, `Volatile`, …).
- `LogEntry`, `LogEntryKind`: the log a command writes while it runs.
- `Version`, `Expires`, `ExpirationTime`: the version and the expiration of an asset.

# Where it is used
- `liquers.assets.State.metadata` and `liquers.assets.AssetRef` return metadata.

# Related concepts
- Dependencies recorded in metadata are in `liquers.dependencies`.

# Examples
>>> from liquers.metadata import Status
>>> Status("Ready").name()
'Ready'
"""

from liquers._backend import (
    Metadata,
    MetadataRecord,
    AssetInfo,
    Status,
    LogEntry,
    LogEntryKind,
    Version,
    Expires,
    ExpirationTime,
)

__all__ = [
    "Metadata",
    "MetadataRecord",
    "AssetInfo",
    "Status",
    "LogEntry",
    "LogEntryKind",
    "Version",
    "Expires",
    "ExpirationTime",
]
