---
title: Preface
status: ready
sources: [reference/VISION.md, reference/POSITIONING.md]
reviewed: 2026-11-10
---
# Preface

PURPOSE: The reader knows who this guide is for, what it covers, how to read it, and where the two sister guides are.

This guide is for people who build on Liquers in Rust: developers who write commands, add value
types and stores, embed Liquers in an application or run it as a server. Liquers itself is
written in Rust, so this is the guide that goes furthest into how things work. It assumes that
you know Rust and have used `async` code with Tokio. It does not assume that you know anything
about Liquers.

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
everything, Liquers can compute the result when it is asked for, remember it, notice when the
input has changed, and serve it at a web address, all without code written for this one result.
Chapter 1 shows how.

In Rust you meet Liquers at two levels. As a *user* of the toolkit you register functions as
commands and evaluate queries, much as you would from Python. As a *builder* you extend the
toolkit itself: a new kind of value, a store for a new kind of storage, a server with your own
routes. This guide covers both, and says which is which.

## How to read this guide

Read the first three chapters in order. Chapter 1 gives the overview, chapter 2 sets up a
project, and chapter 3 teaches the query language that everything else uses. After that each
chapter stands on its own, and you can go straight to the one you need.

The appendices are for looking things up: the complete query syntax (A), the commands that come
with Liquers (B), the vocabulary (C) and a map to the other guides (D). The types and functions
named in this guide are described one by one in the
[API reference](https://orest-d.github.io/liquers/overview.html#api-reference).

## Conventions

The examples of chapter 1 that show a whole program look like this, and you can copy them into
`src/main.rs` of the project that chapter 2 sets up:

```rust
// Sketch: the shape of a complete example.
#[tokio::main]
async fn main() -> Result<(), Error> {
    // ...
    Ok(())
}
```

Every other example is the *body* of such a function: statements that run inside an `async fn`
returning `Result<(), Error>`. They use two small files that section 2.4 gives you: `library.rs`,
which builds an environment with the command library, and `prelude.rs`, which imports the names
that almost every example needs. A `use` line inside an example imports something beyond those.

Every example in this guide is compiled and run by the tests of the repository, exactly as
printed. The only exceptions start with a comment that says `// Sketch`, like the one above: they
show a shape and leave the rest out. An `assert_eq!` states what a line gives.

Queries on their own are shown as plain text:

```text
hello/greet-Hi
```

A term is set in *italics* where it is defined. Appendix C collects the definitions.

## The sister guides

There are three guides: *Liquers for Rust* (this one), *Liquers for Python* and *Liquers for the
Web*, which covers JavaScript in the browser. They have the same chapters with the same numbers,
so chapter 5 is about commands in each of them. The Python package and the browser module are
both built from the crates this guide describes, so what you read here about queries, assets and
stores holds there too. Appendix D takes you to the same topic in the other guides.
