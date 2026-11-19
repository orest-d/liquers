"""The Liquers UI model: show queries as elements, submit actions and follow their statuses, in a window or without one.

Rust counterpart: `liquers_lib::ui`. Every name here is also available directly from `liquers`
(`liquers.ui is liquers.lui.ui`); `import liquers as lq` and `lq.ui` stays
the usual style.

# Main components
- `ui()`: the UI of the default environment.
- `UI`: a UI model, with `run()` for a native window.
- `UIElement`, `UIElementInfo`: an element and what it shows.
- `UIStatusEvent`, `UISubscription`: status changes and their subscriptions.

# Where it is used
- `liquers.environment.Environment.ui` returns a `UI`.

# Related concepts
- Named `lui` (Liquers UI) rather than `ui` so that the function `liquers.ui` keeps its name; the command namespace of the UI commands is also `lui`.

# Examples
>>> import liquers as lq
>>> from liquers.lui import UI
>>> lq.lui.ui is lq.ui
True
"""

from liquers._env import ui
from liquers._backend import (
    UI,
    UIElement,
    UIElementInfo,
    UIStatusEvent,
    UISubscription,
)

__all__ = [
    "ui",
    "UI",
    "UIElement",
    "UIElementInfo",
    "UIStatusEvent",
    "UISubscription",
]
