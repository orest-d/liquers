"""Docstring and signature coverage of the public API (LANGUAGE-INTEGRATION_GUIDE.md Appendix A
§DOCS, DOCS04; design py-docstrings).

The coverage set is every name in ``lq.__all__`` plus, for each class listed there, the public
names (not starting with ``_``) in the class's *own* ``__dict__``. Values are read with
``getattr(cls, name)`` rather than from ``__dict__``: a ``staticmethod`` object found in ``__dict__``
is the descriptor, not the function (on CPython 3.9 it even carried ``staticmethod``'s own ``__doc__``,
a false pass). Enum members (instances of their own class) are values, not API, and are skipped.
"""

import inspect
import types

import liquers as lq

#: Names the walk deliberately leaves out. ``QuerySegment`` is returned by ``Query.segments`` but
#: is not registered or exported yet (``PY-QUERY-SEGMENT-CLASS-NOT-REGISTERED``); its methods are
#: documented all the same, and this entry goes once ``py-query-segment-registration`` lands.
KNOWN_EXCLUSIONS = frozenset({"QuerySegment"})


def coverage_set(module_names, getattr_from):
    """Yields ``(qualified name, value)`` for each public name and each public class attribute."""
    for name in module_names:
        if name in KNOWN_EXCLUSIONS:
            continue
        value = getattr(getattr_from, name)
        yield name, value
        if not isinstance(value, type):
            continue
        for attribute in sorted(vars(value)):
            if attribute.startswith("_"):
                continue
            member = getattr(value, attribute)
            if isinstance(member, value):
                continue  # an enum member
            yield f"{name}.{attribute}", member


def _has_doc(value):
    doc = getattr(value, "__doc__", None)
    return isinstance(doc, str) and doc.strip() != ""


def undocumented(module_names, getattr_from):
    """The qualified names in the coverage set whose ``__doc__`` is missing or empty."""
    return [name for name, value in coverage_set(module_names, getattr_from) if not _has_doc(value)]


def _is_property(value):
    # A getset descriptor (a PyO3 #[getter] or #[pyo3(get)] field) or a Python property: an
    # attribute, which never has a call signature.
    return inspect.isdatadescriptor(value) or isinstance(value, property)


def without_signature(module_names, getattr_from):
    """The qualified names of the public callables ``inspect.signature`` fails on: functions,
    methods and classes (a class's signature is its constructor's)."""
    missing = []
    for name, value in coverage_set(module_names, getattr_from):
        if _is_property(value) or not callable(value):
            continue
        try:
            inspect.signature(value)
        except (ValueError, TypeError):
            missing.append(name)
    return missing


def test_DOCS04_every_public_name_is_documented():
    missing = undocumented(lq.__all__, lq)
    assert not missing, f"{len(missing)} public names have no docstring: {', '.join(missing)}"


def test_DOCS04_coverage_check_names_an_undocumented_attribute():
    class Synthetic:
        """A documented class."""

        def documented(self):
            """Has a docstring."""

        def bare(self):
            pass

    namespace = types.SimpleNamespace(Synthetic=Synthetic)
    assert undocumented(["Synthetic"], namespace) == ["Synthetic.bare"]


def test_DOCS04_every_public_callable_has_a_signature():
    missing = without_signature(lq.__all__, lq)
    assert not missing, f"{len(missing)} public callables have no signature: {', '.join(missing)}"


def test_DOCS04_generated_error_classes_are_documented():
    assert lq.KeyNotFoundError.__doc__ == "Raised for ErrorType.KeyNotFound."
    assert lq.StoreError.__doc__ == "Base class of every error in the Store category."
