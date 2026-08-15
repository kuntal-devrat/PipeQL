"""Zero-boilerplate PipeQL database adapters (``pipeql_python.driver``).

Wraps any DB-API 2.0 (PEP 249) connection -- or an ``asyncpg`` connection --
and auto-handles:

* compilation (PipeQL source -> dialect SQL)
* parameter binding (named PipeQL params -> positional driver args)
* dispatch (SELECT via ``cursor.fetchall()``, mutations via ``cursor.execute``)
* ``$data`` object expansion (partial insert/update with zero boilerplate)
* ``insert_and_fetch`` / ``update_and_fetch`` (single-call write + return)

Supported connections (auto-detected, or forced via ``driver=``):
``sqlite3``, ``duckdb``, ``psycopg`` / ``psycopg2``, ``pymysql`` /
``mysql.connector``, and ``asyncpg``.

Example::

    import sqlite3
    from pipeql_python.driver import create_pipeql_driver

    db = create_pipeql_driver(sqlite3.connect("notes.db"))

    rows = db.query("from notes | filter category == $cat", {"cat": "Ideas"})
    run = db.execute("into notes | insert [title = $title]", {"title": "New Note"})
    note = db.insert_and_fetch("into notes | insert $data", {"title": "New Note"})
    updated = db.update_and_fetch(
        "from notes | filter id == $id | update $data", {"id": 1, "data": {"title": "Renamed"}})
"""

from __future__ import annotations

import re
from typing import Any, Dict, List, Optional, Tuple

from .pipeql_python import compile as _compile

__all__ = ["create_pipeql_driver", "detect_driver", "PipeqlDriver"]

_DIALECT_BY_DRIVER = {
    "sqlite3": "sqlite",
    "duckdb": "duckdb",
    "psycopg": "postgres",
    "asyncpg": "postgres",
    "pymysql": "mysql",
    "mysql": "mysql",
    "dbapi": None,
}

_ASYNC_DRIVERS = frozenset({"asyncpg"})
_NO_RETURNING = frozenset({"pymysql", "mysql"})

_DATA_RE = re.compile(r"\$data\b")
_IDENT_RE = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")
_RETURNING_RE = re.compile(r"\bRETURNING\b", re.IGNORECASE)


def _bind(compiled: Dict[str, Any], params: Optional[Dict[str, Any]]) -> List[Any]:
    """Map named PipeQL params to positional driver args.

    Literal-derived params (concrete type in the analysis param map, or a
    select-mode string literal with no param-map entry) bind as themselves.
    A user ``$param`` (type ``Any``) missing from ``params`` is a bug — fail
    loudly instead of silently binding its name and returning wrong data.
    """
    analysis = compiled.get("analysis") or {}
    types = {p["name"]: p.get("ty") for p in analysis.get("param_map", [])}
    args = []
    for name in compiled["params"]:
        if params is not None and name in params:
            args.append(params[name])
        elif types.get(name) == "Any":
            raise KeyError(
                f"missing value for parameter ${name} — pass it in params, "
                f"e.g. {{ {name}: ... }}"
            )
        else:
            # Literal-derived param: the value is the name itself.
            args.append(name)
    return args


def _rows_as_dicts(cursor) -> List[Dict[str, Any]]:
    cols = [d[0] for d in (cursor.description or [])]
    return [dict(zip(cols, row)) for row in cursor.fetchall()]


def _parse_command_tag(status: str) -> Optional[int]:
    parts = str(status).split()
    if parts and parts[-1].isdigit():
        return int(parts[-1])
    return None


def _expand_data(source: str, data: Any) -> Tuple[str, Dict[str, Any]]:
    """Rewrite ``$data`` in ``source`` into explicit column assignments.

    Values from ``data`` become bound params named ``data0``, ``data1``, ...
    """
    if not isinstance(data, dict):
        raise ValueError(
            f"$data object expansion requires a dict (got {type(data).__name__})"
        )
    if not data:
        raise ValueError("$data object expansion requires at least one property")
    parts = _DATA_RE.split(source)
    chunks = [parts[0]]
    values: Dict[str, Any] = {}
    n = 0
    for i in range(1, len(parts)):
        prev = chunks[-1].rstrip()
        last = prev[-1] if prev else ""
        in_brackets = last in ("[", ",")
        assignments = []
        for key, v in data.items():
            if not _IDENT_RE.match(key):
                raise ValueError(
                    f"cannot expand $data: column {key!r} is not a valid identifier"
                )
            pname = f"data{n}"
            n += 1
            values[pname] = v
            assignments.append(f"{key} = ${pname}")
        body = ", ".join(assignments)
        chunks.append(body if in_brackets else f"[{body}]")
        chunks.append(parts[i])
    return "".join(chunks), values


def _with_returning(sql: str) -> str:
    if _RETURNING_RE.search(sql):
        return sql
    return sql.rstrip().rstrip(";") + " RETURNING *;"


def detect_driver(conn) -> str:
    """Duck-type a connection and return its driver kind."""
    if conn is None:
        raise ValueError("detect_driver requires a connection")
    module = type(conn).__module__
    if module == "sqlite3":
        return "sqlite3"
    if module == "duckdb":
        return "duckdb"
    if hasattr(conn, "fetch") and hasattr(conn, "execute"):
        return "asyncpg"
    if module.startswith("psycopg"):
        return "psycopg"
    if module.startswith("pymysql"):
        return "pymysql"
    if module.startswith("mysql"):
        return "mysql"
    if hasattr(conn, "cursor"):
        return "dbapi"
    raise ValueError(
        f"unsupported database connection {type(conn).__name__!r}; "
        "pass driver=... to force a driver kind"
    )


class PipeqlDriver:
    """A database connection pre-wired with PipeQL compilation & dispatch."""

    def __init__(
        self,
        conn,
        *,
        dialect: Optional[str] = None,
        driver: Optional[str] = None,
    ):
        if conn is None:
            raise ValueError("create_pipeql_driver requires a database connection")
        self._conn = conn
        self.driver = driver or detect_driver(conn)
        self.dialect = dialect or _DIALECT_BY_DRIVER.get(self.driver)
        if self.dialect is None:
            raise ValueError(
                f"cannot infer a target dialect for driver {self.driver!r}; pass dialect=..."
            )
        self._cache: Dict[Tuple[str, str], Dict[str, Any]] = {}

    @staticmethod
    def _source_of(source: Any) -> str:
        """Resolve a builder object (duck-typed) or a raw source string."""
        if isinstance(source, str):
            return source
        if hasattr(source, "source") and callable(source.source):
            return source.source()
        if hasattr(source, "source"):
            return source.source
        raise TypeError(
            "source must be a PipeQL string or a builder object with .source()"
        )

    def _prepare(
        self,
        source: Any,
        params: Optional[Dict[str, Any]],
        whole_object_as_data: bool = False,
    ) -> Tuple[str, Dict[str, Any]]:
        """Resolve builders + ``$data`` object expansion into a source string."""
        builder_values = getattr(source, "values", None)
        source = self._source_of(source)
        merged = dict(params or {})
        if isinstance(builder_values, dict):
            merged.update(builder_values)
        if not _DATA_RE.search(source):
            return source, merged
        data = merged.get("data") if isinstance(merged, dict) else None
        if data is None and isinstance(merged, dict) and merged:
            data = merged
        expanded, values = _expand_data(source, data)
        merged.update(values)
        return expanded, merged

    def _compile(self, source: str) -> Dict[str, Any]:
        key = (self.dialect, source)
        hit = self._cache.get(key)
        if hit is None:
            hit = _compile(source, self.dialect)
            self._cache[key] = hit
        return hit

    def compile(
        self,
        source: str,
        params: Optional[Dict[str, Any]] = None,
    ) -> Dict[str, Any]:
        """Compile ``source`` and bind params; adds the positional ``args`` list."""
        src, p = self._prepare(source, params)
        compiled = self._compile(src)
        return {**compiled, "args": _bind(compiled, p)}

    def _run_sync(self, compiled: Dict[str, Any], args: List[Any]) -> Dict[str, Any]:
        # select AND union return rows; mutations and DDL go through execute.
        # Keying off `statement_type == "select"` dropped rows from unions.
        is_select = compiled["statement_type"] in ("select", "union")
        cursor = self._conn.cursor()
        try:
            cursor.execute(compiled["sql"], args)
            if is_select:
                return {"rows": _rows_as_dicts(cursor)}
            return {
                "rows": [],
                "last_id": getattr(cursor, "lastrowid", None),
                "changes": getattr(cursor, "rowcount", None),
            }
        finally:
            try:
                cursor.close()
            except Exception:
                pass

    def _run_returning(self, sql: str, args: List[Any]) -> List[Dict[str, Any]]:
        """Run a row-returning statement (``... RETURNING *``) and fetch dict rows."""
        cursor = self._conn.cursor()
        try:
            cursor.execute(sql, args)
            return _rows_as_dicts(cursor)
        finally:
            try:
                cursor.close()
            except Exception:
                pass

    async def _run_async(self, compiled: Dict[str, Any], args: List[Any]) -> Dict[str, Any]:
        # select AND union are read-only — both must return rows.
        is_select = compiled["statement_type"] in ("select", "union")
        if is_select:
            rows = await self._conn.fetch(compiled["sql"], *args)
            return {"rows": rows}
        status = await self._conn.execute(compiled["sql"], *args)
        return {"rows": [], "last_id": None, "changes": _parse_command_tag(status)}

    def query(self, source: str, params: Optional[Dict[str, Any]] = None):
        """Run any statement; rows for SELECT, ``{last_id, changes, rows: []}`` for mutations."""
        if self.driver in _ASYNC_DRIVERS:
            raise TypeError(f"{self.driver} is async-only: use aquery()/aexecute()")
        src, p = self._prepare(source, params)
        compiled = self._compile(src)
        raw = self._run_sync(compiled, _bind(compiled, p))
        if compiled["statement_type"] in ("select", "union"):
            return raw["rows"]
        return {"last_id": raw["last_id"], "changes": raw["changes"], "rows": []}

    def execute(self, source: str, params: Optional[Dict[str, Any]] = None):
        """Run a statement; mutations return ``{last_id, changes, rows: []}``, selects ``{rows}``."""
        if self.driver in _ASYNC_DRIVERS:
            raise TypeError(f"{self.driver} is async-only: use aquery()/aexecute()")
        src, p = self._prepare(source, params)
        compiled = self._compile(src)
        raw = self._run_sync(compiled, _bind(compiled, p))
        if compiled["statement_type"] in ("select", "union"):
            return {"rows": raw["rows"]}
        return {"last_id": raw["last_id"], "changes": raw["changes"], "rows": []}

    def _fetch_impl(self, source: str, params: Optional[Dict[str, Any]], whole_object: bool):
        """Shared insert/update + return via RETURNING (fallback for MySQL)."""
        src, p = self._prepare(source, params, whole_object_as_data=whole_object)
        compiled = self._compile(src)
        args = _bind(compiled, p)
        if self.driver in _NO_RETURNING:
            raw = self._run_sync(compiled, args)
            return {"last_id": raw["last_id"], "changes": raw["changes"], "rows": []}
        rows = self._run_returning(_with_returning(compiled["sql"]), args)
        if not rows:
            return {"last_id": None, "changes": 0, "rows": []}
        return rows[0] if len(rows) == 1 else rows

    async def _afetch_impl(
        self, source: str, params: Optional[Dict[str, Any]], whole_object: bool
    ):
        src, p = self._prepare(source, params, whole_object_as_data=whole_object)
        compiled = self._compile(src)
        args = _bind(compiled, p)
        rows = await self._conn.fetch(_with_returning(compiled["sql"]), *args)
        if not rows:
            return {"last_id": None, "changes": 0, "rows": []}
        return rows[0] if len(rows) == 1 else rows

    def insert_and_fetch(self, source: str, params: Optional[Dict[str, Any]] = None):
        """Insert and return the created row(s).

        The entire ``params`` object is treated as the data object when the
        source uses ``$data``:
        ``db.insert_and_fetch("into notes | insert $data", req_body)``
        """
        if self.driver in _ASYNC_DRIVERS:
            raise TypeError(f"{self.driver} is async-only: use ainsert_and_fetch()")
        return self._fetch_impl(source, params, whole_object=True)

    def update_and_fetch(self, source: str, params: Optional[Dict[str, Any]] = None):
        """Update matching rows and return the updated row(s)."""
        if self.driver in _ASYNC_DRIVERS:
            raise TypeError(f"{self.driver} is async-only: use aupdate_and_fetch()")
        return self._fetch_impl(source, params, whole_object=False)

    async def ainsert_and_fetch(self, source: str, params: Optional[Dict[str, Any]] = None):
        """Async variant of :meth:`insert_and_fetch` for ``asyncpg``."""
        if self.driver not in _ASYNC_DRIVERS:
            raise TypeError(f"{self.driver} is synchronous: use insert_and_fetch()")
        return await self._afetch_impl(source, params, whole_object=True)

    async def aupdate_and_fetch(self, source: str, params: Optional[Dict[str, Any]] = None):
        """Async variant of :meth:`update_and_fetch` for ``asyncpg``."""
        if self.driver not in _ASYNC_DRIVERS:
            raise TypeError(f"{self.driver} is synchronous: use update_and_fetch()")
        return await self._afetch_impl(source, params, whole_object=False)

    async def aquery(self, source: str, params: Optional[Dict[str, Any]] = None):
        """Async variant of :meth:`query` for ``asyncpg`` connections."""
        if self.driver not in _ASYNC_DRIVERS:
            raise TypeError(f"{self.driver} is synchronous: use query()/execute()")
        src, p = self._prepare(source, params)
        compiled = self._compile(src)
        raw = await self._run_async(compiled, _bind(compiled, p))
        if compiled["statement_type"] in ("select", "union"):
            return raw["rows"]
        return {"last_id": raw["last_id"], "changes": raw["changes"], "rows": []}

    async def aexecute(self, source: str, params: Optional[Dict[str, Any]] = None):
        """Async variant of :meth:`execute` for ``asyncpg`` connections."""
        if self.driver not in _ASYNC_DRIVERS:
            raise TypeError(f"{self.driver} is synchronous: use query()/execute()")
        src, p = self._prepare(source, params)
        compiled = self._compile(src)
        raw = await self._run_async(compiled, _bind(compiled, p))
        if compiled["statement_type"] in ("select", "union"):
            return {"rows": raw["rows"]}
        return {"last_id": raw["last_id"], "changes": raw["changes"], "rows": []}

    def close(self):
        """Close the underlying connection (best-effort, respects async)."""
        closer = getattr(self._conn, "close", None)
        if callable(closer):
            return closer()


def create_pipeql_driver(conn, *, dialect: Optional[str] = None, driver: Optional[str] = None):
    """Wrap a database connection with PipeQL compilation and dispatch.

    Args:
        conn: A DB-API 2.0 connection (``sqlite3``, ``duckdb``, ``psycopg``,
            ``pymysql``, ...) or an ``asyncpg`` connection.
        dialect: Target compile dialect. Inferred from the driver when omitted.
        driver: Force a driver kind instead of duck-typed auto-detection.
    """
    return PipeqlDriver(conn, dialect=dialect, driver=driver)
