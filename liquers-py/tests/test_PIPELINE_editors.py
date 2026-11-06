"""PIPELINE: the editors (`specs/design/liquers-py-pipelines/`, AC-7, AC-8)."""

import threading

import pytest

import liquers as lq


def test_refused_edit_changes_nothing(env):
    """AC-7: a refused edit raises and leaves the editor as it was, also a two-step call."""
    editor = env.query_editor()
    editor.resource("data/sales.csv").action("from_csv", ns="pl")
    action = editor.action("is_in", ns="pl")
    action.set("column", "region").push("EU").push("US")
    before = editor.pipeline
    with pytest.raises(lq.ParameterError):
        action.set("colum", "x")
    with pytest.raises(lq.LiquersError):
        editor.action("is_in", ns="no_such_namespace")
    assert editor.pipeline == before
    assert editor.query().encode() == "-R/data/sales.csv/-/ns-pl/from_csv/is_in-region-EU-US"


def test_editor_exposes_the_registry(env):
    """AC-8: the editor tells which arguments an action still needs, and what they are."""
    editor = env.pipeline_editor()
    editor.resource("data/sales.csv").action("from_csv", ns="pl")
    action = editor.action("is_in", ns="pl")
    assert [info.name for info in editor.unset_arguments(action.index)] == ["column", "values"]
    assert editor.argument_info(action.index, "values").multiple
    assert action.argument_info(0).name == "column"
    assert action.metadata.name == "is_in"
    assert editor.command_metadata(action.index).name == "is_in"
    edited = env.pipeline_editor(editor.pipeline)
    assert edited.pipeline == editor.pipeline


def test_stale_action_editor(env):
    """An action editor whose step was removed raises `ParameterError`; nothing changes."""
    editor = env.pipeline_editor()
    editor.action("hello")
    action = editor.action("same_obj")
    editor.remove(action.index)
    before = editor.pipeline
    with pytest.raises(lq.ParameterError):
        action.push(1)
    assert editor.pipeline == before


def test_action_editor_is_stale_after_its_step_moves(env):
    """Removing an earlier step, inserting or moving one makes an action editor stale instead of
    retargeting it; appending does not."""
    editor = env.pipeline_editor()
    a = editor.action("hello")
    b = editor.action("same_obj")
    editor.push("same_obj")  # an append keeps both editors valid
    assert b.metadata.name == "same_obj"
    editor.remove(a.index)
    before = editor.pipeline
    with pytest.raises(lq.ParameterError, match="stale"):
        b.metadata
    assert editor.pipeline == before
    fresh = editor.edit_action(0)
    assert fresh.metadata.name == "same_obj"
    editor.insert(0, "hello")
    with pytest.raises(lq.ParameterError, match="stale"):
        fresh.metadata


def test_editor_threads(env):
    """Two threads editing one editor while a third holds the GIL: no deadlock, both applied."""
    editor = env.pipeline_editor()
    editor.action("hello")
    stop = threading.Event()

    def spin():
        while not stop.is_set():
            sum(range(1000))

    def push():
        for _ in range(50):
            editor.push({"action": "same_obj"})

    spinner = threading.Thread(target=spin)
    spinner.start()
    workers = [threading.Thread(target=push) for _ in range(2)]
    for worker in workers:
        worker.start()
    for worker in workers:
        worker.join(5)
    stop.set()
    spinner.join(5)
    assert not any(worker.is_alive() for worker in workers)
    assert editor.pipeline.elements_count == 101
