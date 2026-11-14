---
title: Preface
status: ready
sources: [reference/VISION.md, reference/POSITIONING.md]
reviewed: 2026-11-10
---
# Preface

PURPOSE: The reader knows who this guide is for, what it covers, how to read it, and where the two sister guides are.

This guide is for people who use Liquers in a web page: front-end developers who want to run
queries in the browser, write commands in JavaScript, show results in a page, or call a Liquers
server. It assumes that you know JavaScript, ES modules and `async` code. It does not assume
that you know Rust, WebAssembly or anything about Liquers.

## What Liquers is for

Most data work produces *derived* data: a filtered table, a summary, a chart, a report. Computing
such a result once is easy. The work starts afterwards. Somebody else needs the result, so it has
to be served. It is slow, so it has to be cached, and then the cache has to be kept in step with
the input. Months later somebody asks where a number came from.

Liquers is a toolkit for this kind of work. Its central idea is that every result has a name
that says how to make it. The name is a short text called a *query*:

```text
-R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/sort-month/report.html
```

Read from left to right, it says: take the file `data/sales.csv`, read it as a table, keep the
rows of the EU, sort them by month, and present the result as an HTML page. Because the name says
everything, Liquers can compute the result when it is asked for, remember it, and tell you what
it was made from, all without code written for this one result. Chapter 1 shows how.

Liquers is written in Rust. For the browser it is compiled to WebAssembly and wrapped in an
ordinary ES module, `liquers_web.js`. A page that imports the module has the toolkit *inside the
page*: queries are parsed, planned, evaluated and cached in the browser, and no server takes
part. That makes a page a good place for small tools, demonstrations, documentation that runs,
and front ends that should keep working offline. It also sets limits, because a page has one
thread, no file system and a few megabytes to download. This guide says what the limits are and
how the browser differs from Liquers in a native program (section 1.9 has the summary).

A page can also be a *client*: it asks a Liquers server for results over HTTP and does none of
the work itself. The two combine, and chapter 12 covers the client side.

## How to read this guide

Read the first three chapters in order. Chapter 1 gives the overview, chapter 2 loads the module
and explains how to host it, and chapter 3 teaches the query language that everything else uses.
After that each chapter stands on its own, and you can go straight to the one you need.

The appendices are for looking things up: the complete query syntax (A), the commands that come
with Liquers (B), the vocabulary (C) and a map to the other guides (D). Every function and class
of the module is described in the
[API reference](https://orest-d.github.io/liquers/api/js/index.html), which is generated from the
module's TypeScript declarations.

## Conventions

An example is a JavaScript module, as you would write it in a `<script type="module">` or in a
file that such a script imports:

```js
import init, * as liquers from "https://orest-d.github.io/liquers/pkg/liquers-web/liquers_web.js";

await init();            // loads the WebAssembly
await liquers.init();    // creates the Liquers environment
assert(liquers.isInitialized());
```

The first line imports the module from the project's site, so an example works in any page
without a build step. When you host the module yourself, which section 2.3 recommends for
anything you publish, the address becomes your own, for example `"./liquers_web.js"`. Nothing
else in an example changes.

Every example in this guide runs as printed: the tests of the repository run them all against
the module, under Node. They check their results with `assert`, which is Node's `node:assert`
module (`assert.strictEqual`, `assert.deepStrictEqual`, `assert.rejects`, …). A browser has no
`assert`. In a page, read each `assert` as a statement of what the line gives, and use
`console.log` to see a value. A few examples need what Node does not have, such as the page's
document or `localStorage`. The text says so where it happens, and those are not run by the
tests.

Queries on their own are shown as plain text:

```text
hello/greet-Hi
```

A term is set in *italics* where it is defined. Appendix C collects the definitions.

## The sister guides

There are three guides: *Liquers for Rust*, *Liquers for Python* and *Liquers for the Web* (this
one). They have the same chapters with the same numbers, so chapter 5 is about commands in each
of them. The browser module is built from the Rust crates, so what you read here about queries,
assets and stores is the same machinery the other guides describe; where the browser build
leaves something out, the chapter says so and points to the guide that has it. Appendix D takes
you to the same topic in the other guides.
