"""The generated error classes and their fields."""

from typing import assert_type

import liquers as lq

env = lq.Environment()
try:
    env.evaluate("-R/missing.txt")
except lq.KeyNotFoundError as e:
    assert_type(e.key, str | None)
    assert_type(e.type, lq.ErrorType)
    assert_type(e.message, str)
except lq.NotFoundError as e:
    assert_type(e.query, str | None)
except LookupError:
    pass
except lq.StoreError as e:
    assert_type(e.traceback, str | None)
except lq.LiquersError as e:
    assert_type(e.position, lq.Position | None)

error = lq.LiquersError(lq.ErrorType.KeyNotFound, "no such key", key="a/b")
assert_type(error, lq.LiquersError)
assert_type(lq.KeyNotFoundError("no such key", key="a/b"), lq.KeyNotFoundError)
assert_type(lq.ErrorType.KeyNotFound.categories(), list[str])
assert_type(lq.ErrorType.all_categories(), list[str])
