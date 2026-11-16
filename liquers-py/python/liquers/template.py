"""Templates: query-aware Mustache text, rendered against an environment.

Rust counterpart: the `liquers-template` crate and `liquers_lib::template`. Every name here is also available directly from `liquers`
(`liquers.RenderResult is liquers.template.RenderResult`); `import liquers as lq` and `lq.RenderResult` stays
the usual style.

# Main components
- `RenderResult`: the rendered text with its diagnostics.
- `Diagnostic`: one problem found in a template, with its position.

# Where it is used
- `liquers.environment.Environment.validate_template` returns a list of `Diagnostic`.

# Related concepts
- A template command is a template stored as a command; see the Python guide.

# Examples
>>> from liquers.template import Diagnostic, RenderResult
>>> RenderResult.__module__
'liquers.template'
"""

from liquers._backend import (
    RenderResult,
    Diagnostic,
)

__all__ = [
    "RenderResult",
    "Diagnostic",
]
