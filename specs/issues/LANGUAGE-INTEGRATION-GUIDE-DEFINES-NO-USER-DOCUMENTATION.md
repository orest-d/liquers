---
id: LANGUAGE-INTEGRATION-GUIDE-DEFINES-NO-USER-DOCUMENTATION
kind: issue
title: The language integration guide does not require user documentation for an integration
status: draft
priority: P2
complexity: S
area: [docs]
design:
created: 2026-10-19
github:
---

## Problem

`guides/LANGUAGE-INTEGRATION_GUIDE.md` defines what an integration must implement and test, but
not what it must document for its users. The only documentation it asks for is a quick-start that
a new user can run (`PACKAGE`, an Optional feature, and test `PACKAGE03`), and `STUBS` declarations.
Neither the feature table, the §7 design and review checklist, nor Appendix A mentions an API
reference, a reference of the integration's contract, or a usage guide.

## Impact

An integration can be `CONFORMANT` with no documentation beyond a README. `liquers-py` reached the
end of its design with a README and docstrings on about a quarter of its public names; the owner
had to ask for a Python API reference, a Python reference and a Python guide explicitly.
`liquers-web` has the same gap.

## Expected behaviour

The guide states the documentation an integration delivers, for example as an Essential `DOC`
feature or as §7 checklist items:

- a generated API reference in the language's standard form (docstrings and a generator such as
  pdoc or Sphinx for Python, TypeDoc for JavaScript), with a coverage check;
- a reference document of the integration contract: exposed types, value conversion, command
  declaration, errors, threading, lifecycle, limitations;
- a user guide: the package's modules, configuring an environment, registering a command,
  running a query, values, errors and testing, with examples that are executed;
- where each lives (`specs/reference/`, `specs/guides/`, and a future `docs/` per
  `DOCS_STRUCTURE_GUIDE.md` §9.6).

`liquers-py`'s `reference/PYTHON_INTEGRATION.md` and `guides/PYTHON_GUIDE.md` are a worked example.

## Discovery

Raised by the owner when closing `design/liquers-py/` (2026-10-19), who expected the guide to
define the documentation already.
