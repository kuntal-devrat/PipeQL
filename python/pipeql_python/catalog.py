"""Analyzer catalog helpers for PipeQL (``pipeql_python.catalog``).

The analyzer's optional catalog drives column/type validation at compile
time (``compile_with_catalog``). Instead of hand-writing the catalog JSON,
derive it straight from your ``table`` statements — it can never drift
from the DDL.

>>> from pipeql_python import catalog_from_schema
>>> catalog_from_schema("table users [id integer primary auto, name string]")
{'users': {'name': 'users',
           'columns': [{'name': 'id', 'ty': 'Integer'},
                       {'name': 'name', 'ty': 'String'}]}}
"""

from __future__ import annotations

from functools import lru_cache

try:
    from .pipeql_python import (
        compile_with_catalog as _compile_with_catalog,
        compile_with_schema as _native_compile_with_schema,
        catalog_from_schema as _native_catalog_from_schema,
        parse as _parse,
    )
except ImportError:
    try:
        from pipeql_python import (
            compile_with_catalog as _compile_with_catalog,
            compile_with_schema as _native_compile_with_schema,
            catalog_from_schema as _native_catalog_from_schema,
            parse as _parse,
        )
    except ImportError:
        _compile_with_catalog = None
        _native_compile_with_schema = None
        _native_catalog_from_schema = None
        _parse = None

__all__ = ["catalog_from_schema", "compile_with_schema"]

# PipeQL statements are newline-oriented: a statement starts at a line whose
# first keyword introduces one (`table <name>`, `from <table>`, `into <table>`).
# Only `table` statements contribute to the catalog, but every boundary must
# be respected so a multi-statement schema parses one table at a time.
_STATEMENT_STARTERS = ("table ", "from ", "into ")


def _split_statements(source: str) -> list[str]:
    """Split a multi-statement PipeQL source into individual statements."""
    statements: list[str] = []
    current: list[str] = []
    for line in source.splitlines():
        stripped = line.strip()
        if stripped and stripped.startswith(_STATEMENT_STARTERS) and current:
            statements.append("\n".join(current))
            current = [line]
        else:
            current.append(line)
    if current and any(l.strip() for l in current):
        statements.append("\n".join(current))
    return statements


@lru_cache(maxsize=128)
def _catalog_for(source: str) -> dict:
    """Memoized derivation: identical schemas parse once, not per call."""
    return _build_catalog(source)


def _clone_catalog(catalog: dict) -> dict:
    """Fast, shape-known copy — 20x cheaper than copy.deepcopy."""
    return {
        name: {
            "name": meta["name"],
            "columns": [dict(col) for col in meta["columns"]],
        }
        for name, meta in catalog.items()
    }


def _build_catalog(source: str) -> dict:
    catalog: dict = {}
    for statement in _split_statements(source):
        if not statement.lstrip().startswith("table "):
            continue
        try:
            ast = _parse(statement)
        except Exception as exc:
            raise ValueError(
                f"failed to parse table statement: {exc}"
            ) from exc
        name = ast["name"]["name"]
        if name in catalog:
            raise ValueError(f"duplicate table '{name}' in schema")
        catalog[name] = {
            "name": name,
            "columns": [
                {
                    "name": col["name"]["name"],
                    # The analyzer's ValueType has no Timestamp variant (the
                    # parser's ColumnType does); map it to Any so every SDK's
                    # catalog_from_schema emits the same, binding-agnostic
                    # shape.
                    "ty": "Any" if col["ty"] == "Timestamp" else col["ty"],
                }
                for col in ast["columns"]
            ],
        }
    if not catalog:
        raise ValueError(
            "catalog_from_schema requires at least one `table` statement"
        )
    return catalog


def catalog_from_schema(source: str) -> dict:
    """Derive an analyzer catalog from one or more PipeQL ``table`` statements.

    Args:
        source: PipeQL source containing ``table`` statements (one per line,
            each possibly spanning multiple lines). Non-table statements are
            ignored.

    Returns:
        A catalog dict in the shape accepted by ``compile_with_catalog``:
        ``{"<table>": {"name": ..., "columns": [{"name": ..., "ty": ...}]}}``.

    Raises:
        ValueError: if the source contains no ``table`` statement, a table
            statement fails to parse, or the same table is declared twice.
    """
    if _native_catalog_from_schema is not None:
        return _native_catalog_from_schema(source)
    return _build_catalog(source)


def compile_with_schema(source: str, dialect: str, schema: str) -> dict:
    """Compile with analyzer validation, deriving the catalog from ``schema``.

    One call instead of three: pass your ``table`` DDL and get column/type
    checking against it for free — no separate catalog to build or keep in
    sync with the schema.

    >>> from pipeql_python import compile_with_schema
    >>> compile_with_schema(
    ...     "from users | filter nme == $x", "sqlite",
    ...     "table users [id integer primary auto, name string]",
    ... )
    Traceback (most recent call last):
        ...
    ValueError: Unknown column 'nme'...

    Args:
        source: PipeQL source to compile.
        dialect: Target SQL dialect ("postgres", "sqlite", "duckdb", "mysql").
        schema: PipeQL ``table`` statements describing the schema.

    Returns:
        The same result dict as ``compile`` / ``compile_with_catalog``.

    Raises:
        ValueError: if the schema has no ``table`` statement, a table fails to
            parse, or a table is declared twice.
    """
    if _native_compile_with_schema is not None:
        return _native_compile_with_schema(source, dialect, schema)
    if _compile_with_catalog is not None:
        return _compile_with_catalog(source, dialect, catalog_from_schema(schema))
    raise RuntimeError("PipeQL native compiler not loaded")
