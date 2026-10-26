---
id: CORE-ONE-JOB-QUEUE-FOR-ALL-COMMANDS
kind: issue
title: Every queued command shares one job queue, so commands cannot be placed by their cost
status: draft
priority: P2
complexity: L
area: [core/assets]
design:
created: 2026-10-26
github:
---
## Problem

The queued asset manager has one `JobQueue` (`liquers-core/src/assets.rs`), with one capacity set
by `EnvironmentBuilder::with_job_capacity` (default 4). Every command that is not inline
(`cached: false`) takes a slot from it, whatever it costs. A slow Python command, a
resource-hungry calculation and a cheap transformation compete for the same slots, so a few heavy
jobs make every other query wait.

## Expected behaviour

Commands run on the queue they are configured on, for example:
- very fast commands run immediately, bypassing any queue (`cached: false` covers part of this);
- normal commands run on a `normal` queue;
- heavy calculations that cannot run in parallel run on a `calculation` queue with its own,
  possibly single, capacity;
- language-integration commands (Python) may get their own queue.

How many queues there are, their capacities and whether a queue has tiers is configuration, not
code: it depends on the deployment. The design has to decide where a command's queue is declared
(command metadata, a realm or namespace default, environment configuration) and how dependency
waits avoid deadlock across queues.

## Discovery

Raised by Orest (2026-10-26) in `design/liquers-py-async/` Phase 1, question 2. That design keeps
one queue and exposes its capacity from Python as `Environment(job_capacity=…)` until this is done.
