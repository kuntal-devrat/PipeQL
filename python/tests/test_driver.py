import os
import sqlite3
import sys
import pytest

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))

import pipeql_python as p
from pipeql_python.driver import create_pipeql_driver


def test_driver_sqlite_crud_and_data_expansion():
    conn = sqlite3.connect(":memory:")
    conn.row_factory = sqlite3.Row
    db = create_pipeql_driver(conn, dialect="sqlite")

    # Create table
    db.execute("table notes [id integer primary auto, title string, content string, is_archived integer default 0]")

    # Insert with explicit parameters
    res = db.execute("into notes | insert [title = $title, content = $content]", {"title": "Note 1", "content": "Content 1"})
    assert res["last_id"] == 1
    assert res["changes"] == 1

    # Insert with $data expansion
    res = db.execute("into notes | insert $data", {"title": "Note 2", "content": "Content 2"})
    assert res["last_id"] == 2
    assert res["changes"] == 1

    # Query rows
    rows = db.query("from notes | filter is_archived == 0 | select [id, title] | sort [id asc]")
    assert len(rows) == 2
    assert rows[0]["title"] == "Note 1"
    assert rows[1]["title"] == "Note 2"

    # Query single row with parameter
    rows = db.query("from notes | filter id == $id | select [id, title]", {"id": 2})
    assert len(rows) == 1
    assert rows[0]["title"] == "Note 2"

    # Update with $data expansion
    res = db.execute("from notes | filter id == $id | update $data", {"id": 1, "data": {"title": "Note 1 Updated"}})
    assert res["changes"] == 1

    rows = db.query("from notes | filter id == $id | select [title]", {"id": 1})
    assert rows[0]["title"] == "Note 1 Updated"

    # Delete
    res = db.execute("from notes | filter id == $id | delete", {"id": 1})
    assert res["changes"] == 1

    rows = db.query("from notes | select [id]")
    assert len(rows) == 1
    assert rows[0]["id"] == 2


def test_driver_union_query():
    conn = sqlite3.connect(":memory:")
    conn.row_factory = sqlite3.Row
    db = create_pipeql_driver(conn, dialect="sqlite")

    db.execute("table users [id integer primary auto, name string]")
    db.execute("table admins [id integer primary auto, name string]")
    db.execute("into users | insert [name = 'Alice']")
    db.execute("into admins | insert [name = 'Bob']")

    # Union query must return rows
    rows = db.query("from users | select [name] | union all from admins | select [name]")
    assert len(rows) == 2
    names = {r["name"] for r in rows}
    assert names == {"Alice", "Bob"}


def test_driver_missing_param_error():
    conn = sqlite3.connect(":memory:")
    db = create_pipeql_driver(conn, dialect="sqlite")
    db.execute("table users [id integer, age integer]")

    with pytest.raises((KeyError, ValueError), match="missing value for parameter"):
        db.query("from users | filter age >= $min", {})
