import json
import os
import sys

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))

import pipeql_python as p


def test_pipeql_python():
    r = p.compile(
        "from users | filter age >= $min and status == 'active' | select [id, name] | sort [name asc] | take 10",
        "postgres",
    )
    print(r["sql"])
    assert "SELECT id, name FROM users" in r["sql"]
    assert "age >= $1" in r["sql"]
    assert r["params"] == ["min", "active"]
    assert r["parameter_count"] == 2

    # Statement metadata (v2.1) — drivers dispatch on this instead of SQL prefixes
    assert r["statement_type"] == "select"
    assert r["is_mutation"] is False
    assert p.compile("into notes | insert [title = $t]", "sqlite")["statement_type"] == "insert"
    assert p.compile("into notes | insert [title = $t]", "sqlite")["is_mutation"] is True
    assert (
        p.compile("from notes | filter id == $id | update [is_pinned = 1]", "sqlite")["statement_type"]
        == "update"
    )
    assert p.compile("from notes | filter id == $id | delete", "sqlite")["statement_type"] == "delete"
    assert (
        p.compile("table notes [id int primary auto]", "sqlite")["statement_type"] == "create_table"
    )
    assert p.compile("table notes [id int primary auto]", "sqlite")["is_mutation"] is False

    # Upsert statement metadata
    upsert = p.compile(
        "into users | upsert [name = $name, email = $email] | conflict [email] | do update [name = $name]",
        "postgres",
    )
    assert upsert["statement_type"] == "upsert"
    assert upsert["is_mutation"] is True
    assert "ON CONFLICT (email) DO UPDATE SET name = $1" in upsert["sql"]

    # Union statement metadata
    union = p.compile(
        "from active_users | select [id, name] | union from archived_users | select [id, name]",
        "postgres",
    )
    assert union["statement_type"] == "union"
    assert union["is_mutation"] is False
    assert "UNION" in union["sql"]

    # Union ALL
    union_all = p.compile(
        "from active_users | select [id, name] | union all from archived_users | select [id, name]",
        "postgres",
    )
    assert "UNION ALL" in union_all["sql"]

    # Subquery
    subq = p.compile(
        "from orders | filter customer_id in (from customers | filter region == 'EU' | select [id])",
        "postgres",
    )
    assert "IN (SELECT id FROM customers" in subq["sql"]
    assert subq["params"] == ["EU"]

    assert isinstance(r["analysis"], dict)
    assert r["analysis"]["param_map"][0]["name"] == "min"

    # Dialects
    sqlite = p.compile("from users | filter id == $id | take 5", "sqlite")
    assert "?" in sqlite["sql"]
    mysql = p.compile("from users | filter id == $id | take 5", "mysql")
    assert "LIMIT" in mysql["sql"]

    # Catalog validation
    catalog = {
        "users": {
            "name": "users",
            "columns": [{"name": "id", "ty": "Integer"}, {"name": "name", "ty": "String"}],
        }
    }
    ok = p.compile_with_catalog("from users | select [id, name]", "postgres", catalog)
    assert "SELECT id, name FROM users" in ok["sql"]
    try:
        p.compile_with_catalog("from users | select [nope]", "postgres", catalog)
        raise AssertionError("expected unknown column error")
    except ValueError as e:
        assert "nope" in str(e), str(e)

    # AST parse
    ast = p.parse("from users | filter id == $x | select [id]")
    assert ast["source"]["name"]["name"] == "users"
    assert len(ast["steps"]) == 2

    # Errors
    try:
        p.compile("from users | filter", "postgres")
        raise AssertionError("expected parse error")
    except ValueError:
        pass

    assert p.supported_dialects() == ["postgres", "sqlite", "duckdb", "mysql"]
    assert p.version().count(".") >= 2

    # ---------------------------------------------------------------------------
    # v2.1 driver adapters (pipeql_python.driver)
    # ---------------------------------------------------------------------------
    import asyncio
    import sqlite3

    from pipeql_python.driver import create_pipeql_driver, detect_driver

    # Real sqlite3 (DB-API 2.0)
    conn = sqlite3.connect(":memory:")
    conn.execute(
        "CREATE TABLE notes (id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT, is_pinned INT DEFAULT 0)"
    )

    db = create_pipeql_driver(conn)
    assert db.driver == "sqlite3"
    assert db.dialect == "sqlite"
    assert detect_driver(conn) == "sqlite3"

    # select
    assert db.query("from notes | select [id, title]") == []

    # mutation -> {last_id, changes, rows: []}
    r = db.execute("into notes | insert [title = $title]", {"title": "Hello"})
    assert r == {"last_id": 1, "changes": 1, "rows": []}

    # rows come back as dicts
    rows = db.query("from notes | filter id == $id | select [id, title]", {"id": 1})
    assert rows == [{"id": 1, "title": "Hello"}]

    # query() auto-dispatches mutations
    r2 = db.query("from notes | filter id == $id | update [is_pinned = 1]", {"id": 1})
    assert r2["changes"] == 1
    assert r2["rows"] == []

    # create_table DDL dispatches to execute, not fetchall
    db.execute("table tags [id int primary auto, name string]", {})
    assert (
        conn.execute("SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'tags'").fetchone()[0]
        == "tags"
    )

    # compile() exposes bound positional args + statement metadata
    c = db.compile("from notes | filter id == $id | select [id]", {"id": 5})
    assert c["args"] == [5]
    assert c["statement_type"] == "select"
    assert c["is_mutation"] is False

    # literal params bind as themselves
    c2 = db.compile("from notes | filter title == 'x'", {})
    assert c2["args"] == ["x"]

    # UNION queries return rows (regression: dispatched as mutation, rows dropped)
    conn.execute("CREATE TABLE active (id INT)")
    conn.execute("CREATE TABLE archived (id INT)")
    conn.execute("INSERT INTO active VALUES (1)")
    conn.execute("INSERT INTO archived VALUES (2)")
    union_rows = db.query(
        "from active | select [id] | union from archived | select [id]"
    )
    assert sorted(r["id"] for r in union_rows) == [1, 2], union_rows

    # Missing user param fails loudly (regression: silently bound its name)
    try:
        db.query("from notes | filter id == $user_id | select [id]", {"id": 42})
        raise AssertionError("expected KeyError for missing param")
    except KeyError as e:
        assert "user_id" in str(e)

    conn.close()

    # asyncpg-style (async) connection
    class FakeAsyncPG:
        def __init__(self):
            self.log = []

        async def fetch(self, sql, *args):
            self.log.append(("fetch", sql, args))
            return [{"id": 1}]

        async def execute(self, sql, *args):
            self.log.append(("execute", sql, args))
            return "INSERT 0 1"

    adb = create_pipeql_driver(FakeAsyncPG())
    assert adb.driver == "asyncpg"
    assert adb.dialect == "postgres"

    rows = asyncio.run(adb.aquery("from t | select [id]"))
    assert rows == [{"id": 1}]
    ar = asyncio.run(adb.aexecute("into t | insert [a = $a]", {"a": 1}))
    assert ar["changes"] == 1

    # sync methods on an async driver raise clearly
    try:
        adb.query("from t | select [id]")
        raise AssertionError("expected TypeError")
    except TypeError:
        pass

    # --- $data object expansion + insert/update_and_fetch ---
    conn2 = sqlite3.connect(":memory:")
    db2 = create_pipeql_driver(conn2)
    db2.execute("table t [id int primary auto, title string, flag int default 0]", {})

    # $data insert via execute()
    r = db2.execute("into t | insert $data", {"data": {"title": "x", "flag": 1}})
    assert r["last_id"] == 1
    assert r["changes"] == 1
    assert db2.query("from t | select [id, title, flag]") == [
        {"id": 1, "title": "x", "flag": 1}
    ]

    # $data partial update (SET params before WHERE params)
    r = db2.execute("from t | filter id == $id | update $data", {"id": 1, "data": {"title": "y"}})
    assert r["changes"] == 1
    assert db2.query("from t | select [title]") == [{"title": "y"}]

    # insert_and_fetch returns the created row
    note = db2.insert_and_fetch("into t | insert $data", {"title": "Hello", "flag": 0})
    assert note == {"id": 2, "title": "Hello", "flag": 0}

    # update_and_fetch returns the updated row
    updated = db2.update_and_fetch(
        "from t | filter id == $id | update $data", {"id": 2, "data": {"title": "Bye"}}
    )
    assert updated == {"id": 2, "title": "Bye", "flag": 0}

    # $data requires a dict with at least one property
    try:
        db2.execute("into t | insert $data", {})
        raise AssertionError("expected ValueError")
    except ValueError as e:
        assert "$data" in str(e)

    # asyncpg $data + fetch
    class FakeAsyncPG2(FakeAsyncPG):
        async def fetch(self, sql, *args):
            assert "RETURNING" in sql
            return [{"id": 1, "title": args[0]}]

    adb2 = create_pipeql_driver(FakeAsyncPG2())
    n = asyncio.run(adb2.ainsert_and_fetch("into t | insert $data", {"title": "Async"}))
    assert n == {"id": 1, "title": "Async"}

    conn2.close()


def test_catalog_from_schema():
    """catalog_from_schema derives an analyzer catalog from table statements."""
    catalog = p.catalog_from_schema(
        "table users [id integer primary auto, name string not null]"
    )
    assert catalog["users"]["name"] == "users"
    assert catalog["users"]["columns"] == [
        {"name": "id", "ty": "Integer"},
        {"name": "name", "ty": "String"},
    ]

    # Multi-table schemas, one table per line, blank lines tolerated.
    catalog = p.catalog_from_schema(
        "table users [id integer primary auto]\n\ntable posts [id integer primary auto, user_id integer]"
    )
    assert set(catalog) == {"users", "posts"}

    # Derived catalog actually validates: a column typo is rejected.
    try:
        p.compile_with_catalog("from users | filter nme == $x", "sqlite", catalog)
        raise AssertionError("expected compile failure for unknown column")
    except Exception:
        pass
    # ...while a correct column compiles.
    p.compile_with_catalog("from users | filter id == $x", "sqlite", catalog)

    # Non-table statements are ignored; a table-less source raises ValueError.
    try:
        p.catalog_from_schema("from users | select [id]")
        raise AssertionError("expected ValueError")
    except ValueError as e:
        assert "table" in str(e)

    # Duplicate table names are a mistake, not a merge.
    try:
        p.catalog_from_schema(
            "table users [id integer]\ntable users [id integer]"
        )
        raise AssertionError("expected ValueError")
    except ValueError as e:
        assert "duplicate table" in str(e)

    # One-call form: compile_with_schema(source, dialect, schema) derives the
    # catalog from the DDL and validates in a single command.
    schema = "table users [id integer primary auto, name string not null]"
    r = p.compile_with_schema("from users | filter id == $x", "sqlite", schema)
    assert "WHERE (id = ?)" in r["sql"]
    assert r["parameter_count"] == 1

    # Timestamp columns are natively typed as Timestamp across all layers.
    ts = p.catalog_from_schema("table events [id integer, at timestamp]")
    assert ts["events"]["columns"][1] == {"name": "at", "ty": "Timestamp"}
    p.compile_with_catalog("from events | filter at == $t", "sqlite", ts)

    # Derivation is memoized, and callers get defensive copies: mutating the
    # returned catalog must not poison the cache for the next caller.
    c1 = p.catalog_from_schema("table t [id integer]")
    c2 = p.catalog_from_schema("table t [id integer]")
    assert c1 == c2
    assert c1 is not c2
    c1["t"]["columns"].append({"name": "junk", "ty": "String"})
    c3 = p.catalog_from_schema("table t [id integer]")
    assert [col["name"] for col in c3["t"]["columns"]] == ["id"]

    # Errors are not cached: the same bad schema raises every time.
    for _ in range(2):
        try:
            p.catalog_from_schema("table t [id integer]\ntable t [id integer]")
            raise AssertionError("expected ValueError")
        except ValueError as e:
            assert "duplicate table" in str(e)
    try:
        p.compile_with_schema("from users | filter nme == $x", "sqlite", schema)
        raise AssertionError("expected unknown-column error")
    except Exception:
        pass
    try:
        p.compile_with_schema("from users | select [id]", "sqlite", "from users | select [id]")
        raise AssertionError("expected ValueError")
    except ValueError as e:
        assert "table" in str(e)


if __name__ == "__main__":
    test_pipeql_python()
    test_catalog_from_schema()
