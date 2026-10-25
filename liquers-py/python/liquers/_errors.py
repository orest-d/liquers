"""The exception hierarchy for Liquers errors.

Every Liquers error reaches Python as an instance of ``LiquersError``, or more precisely of the
subclass for its ``ErrorType``: ``KeyNotFoundError`` for ``ErrorType.KeyNotFound``,
``ExecutionError`` for ``ErrorType.ExecutionError`` (a variant already ending in ``Error`` is not
suffixed twice). Python code can raise either form, and a command that does keeps its type when
Liquers receives it::

    raise lq.LiquersError(lq.ErrorType.KeyNotFound, "no such key", key="a/b")
    raise lq.KeyNotFoundError("no such key", key="a/b")       # the same error

The subclasses are generated from the native ``ErrorType`` by ``_install``, so a variant added in
Rust appears here without a code change.

Error types also fall into overlapping categories (``ErrorType.categories()``), each with its own
abstract base class: ``StoreError`` (every store error), ``KeyedAssetError`` (the same for keyed
assets) and ``NotFoundError`` (``KeyNotFound`` and ``AssetNotFound``; also a ``LookupError``). A
variant's class derives from the class of every category it is in, so one ``except`` clause can
catch a whole category::

    try:
        env.evaluate("-R/data/missing.txt")
    except lq.NotFoundError as e:      # KeyNotFoundError (store) or AssetNotFoundError (no recipe)
        ...
"""

_SUBCLASSES = {}

# Extra Python bases of a category class, beyond ``LiquersError``.
_CATEGORY_EXTRA_BASES = {"NotFound": (LookupError,)}


def _subclass_name(variant_name):
    return variant_name if variant_name.endswith("Error") else variant_name + "Error"


class LiquersError(Exception):
    """A Liquers error. ``type`` is the ``ErrorType``; the other fields mirror the Rust error."""

    #: The ErrorType a subclass stands for; ``None`` on the base class.
    _error_type = None

    def __new__(cls, *args, **kwargs):
        if cls is LiquersError and args and _is_error_type(args[0]):
            cls = _SUBCLASSES.get(_variant_name(args[0]), LiquersError)
        return super().__new__(cls)

    def __init__(self, *args, query=None, key=None, position=None, traceback=None):
        if args and _is_error_type(args[0]):
            error_type, args = args[0], args[1:]
        else:
            error_type = type(self)._error_type
        if error_type is None:
            raise TypeError("LiquersError needs an ErrorType: LiquersError(ErrorType.X, message)")
        message = args[0] if args else ""
        if len(args) > 1:
            raise TypeError("LiquersError takes an ErrorType and one message")
        super().__init__(message)
        self.type = error_type
        self.message = message
        self.query = query
        self.key = key
        self.position = position
        self.traceback = traceback

    def __str__(self):
        return self.message

    def __repr__(self):
        return f"{type(self).__name__}({self.message!r})"


# Reported as `liquers.LiquersError`, where users import it from (OBJECT08).
LiquersError.__module__ = "liquers"

_ERROR_TYPE_CLASS = None


def _is_error_type(value):
    return _ERROR_TYPE_CLASS is not None and isinstance(value, _ERROR_TYPE_CLASS)


def _variant_name(error_type):
    # PyO3 renders a simple enum member as "ErrorType.Variant".
    return repr(error_type).rsplit(".", 1)[-1]


def _install(error_type_class):
    """Generate one class per error category and one subclass per ErrorType variant.

    Returns them all by name, for re-export.
    """
    global _ERROR_TYPE_CLASS
    _ERROR_TYPE_CLASS = error_type_class
    generated = {}
    categories = {}
    for category in error_type_class.all_categories():
        class_name = category + "Error"
        bases = (LiquersError,) + _CATEGORY_EXTRA_BASES.get(category, ())
        # No `_error_type`: like LiquersError itself, a category class needs an explicit ErrorType.
        categories[category] = type(class_name, bases, {"__module__": "liquers"})
        generated[class_name] = categories[category]
    for name in dir(error_type_class):
        member = getattr(error_type_class, name)
        if not isinstance(member, error_type_class):
            continue
        class_name = _subclass_name(name)
        if class_name in generated:
            raise RuntimeError(f"error class name {class_name} is both a category and a variant")
        bases = tuple(categories[c] for c in member.categories()) or (LiquersError,)
        subclass = type(class_name, bases, {"_error_type": member, "__module__": "liquers"})
        _SUBCLASSES[name] = subclass
        generated[class_name] = subclass
    globals().update(generated)
    return generated


def _from_native(error_type, message, query, key, position, traceback):
    """Called from Rust to build the exception for a Liquers error."""
    cls = _SUBCLASSES.get(_variant_name(error_type), LiquersError)
    return cls(error_type, message, query=query, key=key, position=position, traceback=traceback)
