---
title: Preface
status: draft
sources: [reference/VISION.md, reference/POSITIONING.md]
reviewed: 2026-11-09
---
# Preface

PURPOSE: The reader knows who this guide is for, what it covers, how to read it, and where the two sister guides are.

This guide is for people who use Liquers from Python: data scientists, analysts and developers
who work with the `liquers` package in scripts, notebooks and services. It assumes that you know
Python. It does not assume that you know Rust, the language Liquers is written in, or anything
about Liquers itself.

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

## How to read this guide

Read the first three chapters in order. Chapter 1 gives the overview, chapter 2 installs the
package, and chapter 3 teaches the query language that everything else uses. After that each
chapter stands on its own, and you can go straight to the one you need.

The appendices are for looking things up: the complete query syntax (A), the commands that come
with Liquers (B), the vocabulary (C) and a map to the other guides (D).

## Conventions

Python examples look like this, and each one is a complete program that you can copy and run,
unless the text says what it leaves out:

```python
import liquers as lq

print(lq.evaluate("commands_doc")[:10])
```

The package is always imported as `lq`. A comment after a line shows what it prints.

Queries on their own are shown as plain text:

```text
hello/greet-Hi
```

A term is set in *italics* where it is defined. Appendix C collects the definitions.

## The sister guides

There are three guides: *Liquers for Python* (this one), *Liquers for Rust* and *Liquers for the
Web*, which covers JavaScript in the browser. They have the same chapters with the same numbers,
so chapter 5 is about commands in each of them. If you use Liquers from two languages, or want to
know how a server written in Rust handles the query you tried in Python, appendix D takes you to
the same topic in the other guides.
