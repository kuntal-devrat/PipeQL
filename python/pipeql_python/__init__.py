"""PipeQL Python SDK (v1.1.7).

Re-exports the native ``pipeql_python`` extension functions and the
zero-boilerplate ``driver`` adapters.

>>> import pipeql_python as p
>>> p.compile("from users | select [id]", "sqlite")["sql"]
'SELECT id FROM users;'
>>> from pipeql_python import driver
"""

try:
    from .pipeql_python import (
        compile,
        compile_with_catalog,
        parse,
        supported_dialects,
        version,
    )
except ImportError:
    from pipeql_python import (
        compile,
        compile_with_catalog,
        parse,
        supported_dialects,
        version,
    )
from pathlib import Path
from . import builder, catalog, driver
from .builder import PipeQL, Value
from .catalog import catalog_from_schema, compile_with_schema

_prompt_path = Path(__file__).parent / "ai" / "system_prompt.md"
if _prompt_path.exists():
    SYSTEM_PROMPT = _prompt_path.read_text(encoding="utf-8")
else:
    SYSTEM_PROMPT = ""

__all__ = [
    "compile",
    "compile_with_catalog",
    "parse",
    "supported_dialects",
    "version",
    "builder",
    "catalog",
    "driver",
    "PipeQL",
    "Value",
    "catalog_from_schema",
    "compile_with_schema",
    "SYSTEM_PROMPT",
]
