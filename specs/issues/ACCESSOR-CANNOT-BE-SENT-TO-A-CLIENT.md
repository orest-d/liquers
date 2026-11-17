---
id: ACCESSOR-CANNOT-BE-SENT-TO-A-CLIENT
kind: feature
title: An accessor cannot be serialized on a server and used on a client
status: draft
priority: P3
complexity: L
area: [core/value, axum, web]
design:
created: 2026-11-16
github:
---
## Problem

`settable-value-accessor` makes an accessor a handle value (`ExtValue::Accessor`) that is never
stored: its `TypeInfo("accessor")` has no formats, so serializing it is `NonSerializable`. A client
can reach a server-side accessor only by its address, through the `/api/acc` routes, and only if it
already knows that address. A server cannot hand a client an accessor as a value, for example as a
field of a UI element's state in the split UI (`ui-split-elements`), as the result of a query, or
inside a message, which the client then turns back into a working accessor.

Some accessors could be serialized, because everything that defines them is data: a store
accessor (key and view) and a keyed-asset accessor (key), with their projection steps. Others
cannot: a memory accessor holds a value in one process; an `AssetRefRoot` wraps a live handle; a
custom root, or a `validated` wrapper, holds code (Orest, 2026-11-16).

## Impact

A client that should edit what a server shows, such as a recipe editor in the browser for a
`recipes.yaml` on the server, must be told the address out of band, and must rebuild the
projection chain by hand. Split UI elements cannot carry accessors in their state.

## Expected behaviour

Raised by Orest on 2026-11-16; the analysis below is a starting point for the design:

1. **Transfer a reference, never the state.** A serialized accessor is an *accessor descriptor*:
   plain data that says how to reach the accessor, not its value, hub or subscribers. It needs its
   own transfer form, for example `accessor.json`. The accessor type itself keeps no storage
   formats, so an accessor is still never written to a store as data.
2. **A remote descriptor works for every accessor that has an address.** `{remote: base URL,
   query}` resolves on the client to a remote root that implements `AccessorRoot` over the
   `/api/acc` routes and their WebSocket (`GET` → `read`, `PUT` with `If-Match` → `write`, socket
   events → `poll_event`). The kind of root on the server does not matter, because the server does
   the I/O, and server-side wrappers such as `validated` keep working because the server still
   applies them. Projection steps can be applied on either side.
3. **A local descriptor only for first-class, data-defined roots.** `{root: "store", key, view,
   steps}` or `{root: "key", key, steps}` rebinds against the client's own environment. That is
   useful only when the client sees the same store, for example through the remote store of
   `remote-store-and-asset-manager`. A step serializes as its command form (`recipe-0`, `query`,
   `argument-n`), which the address already is.
4. **Everything else is not transferable.** An optional `fn descriptor(&self) ->
   Option<AccessorDescriptor>` on `ValueAccessor` (default `None`) states this per accessor:
   - a memory accessor, an `AssetRefRoot` with no query, a custom root and a closure-based wrapper
     return `None`;
   - a projection returns its parent's descriptor plus its own step, or `None` when the parent has
     none or the step has no command form;
   - serializing an accessor whose `descriptor` is `None` fails with `NonSerializable`, as now.
   A later extension could publish a non-transferable accessor under a server-side handle
   (`/api/acc/handle/{id}`) with a lease. That needs the session model of `CORE-SESSION-AND-KEY-ACL`
   and the lease rules of `VALUE-ACCESSOR-CANNOT-LOCK`, so it is not part of the first step.

Points the design must settle: the descriptor's schema and version; whether the remote form is the
default and the local form opt-in; how a client authenticates the remote root (no credential in a
descriptor); whether to resolve the remote form in core (any HTTP-capable host) or in
`liquers-web` and `liquers-py` only; and the relation to `ui-split-elements`, whose element state
would carry descriptors.

## Discovery

Raised by Orest at the Phase 2 gate of `specs/design/settable-value-accessor/` (2026-11-16), after
the language-wrapper scope was added. Out of scope for that design, which keeps the hook
possible: an address already names an accessor (AC-12), and the `/api/acc` routes are the remote
root's protocol.
