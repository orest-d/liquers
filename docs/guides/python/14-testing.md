---
title: Testing
status: ready
sources: [guides/PYTHON_GUIDE.md, guides/UNITTEST_GUIDE.md]
reviewed: 2026-11-09
---
# 14. Testing

PURPOSE: The reader can test commands and queries in Python, and check their code with a type checker.

Code built on Liquers is easy to test, for two reasons. A command is still a plain function, and
a query is a complete description of a computation that can be run against a small store in
memory.

## 14.1 Testing a command as a function

The decorators return your function unchanged. The first test of a command needs no Liquers at
all:

```python
import liquers as lq

@lq.command
def repeat(text: str, count: int = 2) -> str:
    return text * count

def test_repeat_is_a_function():
    assert repeat("ab") == "abab"
    assert repeat("ab", 3) == "ababab"
```

Test the logic of your commands this way. It is fast, and a failure points straight at your
code.

## 14.2 Testing through a query

A test through a query also checks what a direct call cannot: that the command is registered
under the name you expect, that its arguments convert from text, and that it works together
with the commands before and after it.

Give every test its own environment, and shut it down afterwards. A pytest fixture does both:

```python
import liquers as lq
import pytest

@pytest.fixture
def env():
    environment = lq.Environment()
    yield environment
    environment.shutdown()

def test_repeat(env):
    @env.command
    def repeat(text: str, count: int = 2) -> str:
        return text * count

    env.store_set("data/in.txt", "ab")
    assert env.evaluate("-R/data/in.txt/-/repeat-3") == "ababab"

def test_missing_input(env):
    with pytest.raises(lq.NotFoundError):
        env.evaluate("-R/data/nothing.txt")
```

A new environment has an empty store in memory and only the commands of the library, so tests
cannot influence each other. Do not test with `@lq.command` and `lq.evaluate`: the default
environment is shared by the whole process, and a command registered in one test is still there
in the next.

When your commands live in a module, give the module a function that registers them in a given
environment, and call it from the fixture and from your application:

```python
import liquers as lq
import pytest

def register_commands(env: lq.Environment) -> None:
    @env.command
    def shout(text) -> str:
        return text.upper()

@pytest.fixture
def env():
    environment = lq.Environment()
    register_commands(environment)
    yield environment
    environment.shutdown()

def test_shout(env):
    env.store_set("data/in.txt", "milk")
    assert env.evaluate("-R/data/in.txt/-/shout") == "MILK"
```

## 14.3 Useful assertions

- **The value**: `env.evaluate(query) == expected`.
- **The type and the log**: `env.evaluate_state(query).metadata`, then `type_identifier()` and
  `log()` (chapter 4).
- **An error**: `pytest.raises(lq.ExecutionError)`, or a more specific class (chapter 15). The
  exception has `message`, `key` and `query`.
- **Caching**: count the calls of a command in a list, as chapter 7 does, and assert how often
  it ran.
- **A query's shape**, without running it: `lq.parse(text).encode()`.
- **A recipe**: put a `recipes.yaml` into the test store with `env.store.set`, then evaluate the
  key.

For input files, `env.store_set(key, value)` writes a value and `env.store.set(key, data)` writes
bytes (chapter 6). For tests against real files, configure a `filesystem` store on pytest's
`tmp_path`.

## 14.4 Type checking

The package ships type declarations, so mypy, pyright and editors check your calls into
`liquers`. A decorated command keeps its own signature:

```python
import liquers as lq

@lq.command
def repeat(text: str, count: int = 2) -> str:
    return text * count

assert repeat("ab", 2) == "abab"   # a checker knows this returns str, and refuses repeat(1)
```

What a query returns is typed `Any`, as are `State.value` and the input of a command, because a
checker cannot know what a query produces. Run your checker as usual, for example
`mypy --strict your_package` or `pyright`.
