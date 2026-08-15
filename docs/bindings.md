# PipeQL Bindings

PipeQL's compiler core is a `#![deny(unsafe_code)]` Rust crate with four
high-level APIs: WASM (JS), native Python, a C ABI, and a language server.

## JavaScript / WASM (`@flaxmbot/pipeql`)

The WASM engine compiles the core to a ~107KB-gzip bundle.

```ts
import { compile, compileWithCatalog, compileWithSchema, catalogFromSchema, parse, supportedDialects } from "@flaxmbot/pipeql";

const result = compile("from users | filter age >= $min | take 5", "postgres");
result.sql;             // "SELECT ... WHERE (age >= $1) ... LIMIT 5;"
result.params;          // ["min"]
result.statementType;   // "select"
result.isMutation;      // false

// Analyzer validation derived from your table DDL — one call, no catalog to write:
await compileWithSchema("from users | filter nme == $x",
  "table users [id integer primary auto, name string]");  // throws: Unknown column 'nme'
const catalog = await catalogFromSchema("table users [id integer primary auto, name string]");

// Tagged template — compiles and types the interpolation:
const sql = pipeql`from t | filter id == ${id}`;
```

Build:

```bash
wasm-pack build crates/pipeql-wasm --target web --out-dir ../../js/dist --release
cd js && npm run build
node test/smoke.mjs
```

### `@flaxmbot/pipeql/driver` — zero-boilerplate database adapters

Wrap any native connection (Node & Edge runtimes) and let PipeQL handle
compilation, parameter binding, and `.all()` vs `.run()` dispatch:

```ts
import { createPipeqlDriver } from "@flaxmbot/pipeql/driver";
import sqlite3 from "sqlite3";

const db = createPipeqlDriver(new sqlite3.Database("notes.db"), { dialect: "sqlite" });

const rows = await db.query("from notes | filter category == $cat", { cat: "Ideas" });
const { lastId, changes } = await db.execute(
  "into notes | insert [title = $title]", { title: "New Note" });
const result = await db.pipeql`from notes | filter title == ${userInput}`;
```

Supported drivers (duck-typed, auto-detected): `better-sqlite3`, `sqlite3`,
`pg`, `postgres.js`, `mysql2`, `duckdb`. Pass `{ driver }` to force a kind.

#### `$data` object expansion (zero partial-update boilerplate)

Pass a partial payload object directly; its keys become the insert/update
columns and its values are bound as parameters:

```ts
// partial update — only the sent fields are touched
const updated = await db.execute('from notes | filter id == $id | update $data', {
  id: req.params.id,
  data: req.body,
});
```

#### Single-call write + return

`insertAndFetch` / `updateAndFetch` run the mutation with `RETURNING *`
(sqlite, postgres, duckdb; MySQL falls back to run metadata) and return the
affected row(s) in one call:

```ts
const newNote = await db.insertAndFetch('into notes | insert $data', req.body);
const updated = await db.updateAndFetch(
  'from notes | filter id == $id | update $data', { id, data: req.body });
```

## Python (`pipeql_python`)

Native wheel via maturin (abi3, Python 3.11+):

```python
import pipeql_python

result = pipeql_python.compile("from users | filter age >= $min | take 5", "postgres")
print(result["sql"])     # ... WHERE (age >= $1) ... LIMIT 5;
print(result["params"])  # ["min"]
print(result["statement_type"])  # "select"
print(result["is_mutation"])     # False

# Analyzer validation derived from your table DDL — one call, no catalog to write:
compile_with_schema(
    "from users | filter nme == $x", "sqlite",
    "table users [id integer primary auto, name string]",
)  # raises: Unknown column 'nme'
```

Build & test:

```bash
maturin build -m crates/pipeql-python/Cargo.toml --release
pip install target/wheels/pipeql-*.whl
python python/tests/test_pipeql.py
```

### `pipeql_python.driver` — DB-API 2.0 / asyncpg adapters

Wrap any PEP 249 connection (or an `asyncpg` connection) with automatic
compilation, binding, and dispatch:

```python
import sqlite3
from pipeql_python.driver import create_pipeql_driver

db = create_pipeql_driver(sqlite3.connect("notes.db"))

rows = db.query("from notes | filter category == $cat", {"cat": "Ideas"})
run = db.execute("into notes | insert [title = $title]", {"title": "New Note"})
# run == {"last_id": 1, "changes": 1, "rows": []}
```

`$data` object expansion and single-call writes work the same as in JS:

```python
note = db.insert_and_fetch("into notes | insert $data", {"title": "Hi", "category": "Ideas"})
updated = db.update_and_fetch(
    "from notes | filter id == $id | update $data", {"id": 1, "data": {"title": "Bye"}})
# asyncpg: await db.ainsert_and_fetch(...) / db.aupdate_and_fetch(...)
```

Supported: `sqlite3`, `duckdb`, `psycopg` / `psycopg2`, `pymysql`,
`mysql.connector`, and `asyncpg` (async-only: `aquery` / `aexecute`).

## C / C++ (`libpipeql`)

```c
#include "libpipeql.h"

PipeqlError err = {0};

/* Basic compile */
PipeqlResult *res = pipeql_compile(
    "from users | filter age >= $min | take 5", "postgres", &err);
if (res) {
    puts(res->sql);              /* SQL text */
    puts(res->params_json);      /* ["min"] */
    puts(res->statement_type);   /* "select" */
    printf("%d\n", res->is_mutation);     /* 0 */
    printf("%d\n", res->parameter_count); /* 1 */
    pipeql_result_free(res);
} else {
    printf("error kind=%d: %s\n", err.kind, err.message);
    pipeql_error_clear(&err);
}

/* Compile with catalog validation */
const char *catalog = "{\"users\":{\"name\":\"users\","
    "\"columns\":[{\"name\":\"id\",\"ty\":\"Integer\"}]}}";
res = pipeql_compile_with_catalog(
    "from users | select [id]", "postgres", catalog, &err);

/* Parse-only (returns JSON AST) */
char *ast = pipeql_parse("from users | filter id == $id", &err);
if (ast) { puts(ast); pipeql_string_free(ast); }

/* List supported dialects */
char *dialects = pipeql_supported_dialects();
if (dialects) { puts(dialects); pipeql_string_free(dialects); }
```

Build the shared library, then link a consumer (Windows example):

```bash
cargo build --release -p pipeql-cffi
gcc crates/pipeql-cffi/examples/c_demo.c \
    -I crates/pipeql-cffi/include \
    target/release/pipeql_cffi.dll -o c_demo
```

Error kinds: `0=none`, `1=parse`, `2=analysis`, `3=codegen`.

### C API reference

| Function | Returns | Notes |
|----------|---------|-------|
| `pipeql_compile(source, dialect, err)` | `PipeqlResult*` | Basic compile |
| `pipeql_compile_with_catalog(source, dialect, catalog_json, err)` | `PipeqlResult*` | With schema validation. `catalog_json` may be NULL. |
| `pipeql_parse(source, err)` | `char*` | JSON AST. Free with `pipeql_string_free`. |
| `pipeql_supported_dialects()` | `char*` | JSON array. Free with `pipeql_string_free`. |
| `pipeql_version()` | `const char*` | Static, never free. |
| `pipeql_result_free(res)` | void | Free a `PipeqlResult`. |
| `pipeql_string_free(s)` | void | Free a string from `pipeql_parse` / `pipeql_supported_dialects`. |
| `pipeql_error_clear(err)` | void | Free error message and reset. |

Catalog JSON format:

```json
{"tables":{"users":{"name":"users","columns":[{"name":"id","ty":"Integer"},{"name":"name","ty":"String"}]}}}
```

## Go (`github.com/Flaxmbot/PipeQL/go`)

Fetch from inside a Go module (the binding is a library, not a command, so
`go install` does not apply — use `go get`):

```bash
# inside your project (any directory containing a go.mod)
go get github.com/Flaxmbot/PipeQL/go@latest
# or pin a specific release
# go get github.com/Flaxmbot/PipeQL/go@v1.1.6
```

```go
import "github.com/Flaxmbot/PipeQL/go" // cgo wrapper over libpipeql

/* Basic compile */
res, err := pipeql.Compile("from t | take 5", "sqlite")
// res.SQL, res.Params, res.StatementType, res.IsMutation, res.ParameterCount

/* Compile with catalog validation */
catalog := `{"tables":{"users":{"name":"users","columns":[{"name":"id","ty":"Integer"}]}}}`
res, err = pipeql.CompileWithCatalog("from users | select [id]", "postgres", catalog)

/* Parse-only (returns JSON AST) */
ast, err := pipeql.Parse("from users | filter id == $id")

/* List supported dialects */
dialects := pipeql.SupportedDialects() // ["postgres","sqlite","duckdb","mysql"]
```

### Go API reference

| Function | Returns | Notes |
|----------|---------|-------|
| `Compile(source, dialect)` | `(*Result, error)` | Basic compile |
| `CompileWithCatalog(source, dialect, catalogJSON)` | `(*Result, error)` | With schema validation. Empty string = no validation. |
| `CatalogFromSchema(schema)` | `(string, error)` | Derive catalog JSON from `table` DDL — no hand-written catalog |
| `CompileWithSchema(source, dialect, schema)` | `(*Result, error)` | Compile + schema-derived validation in one call |
| `Parse(source)` | `(json.RawMessage, error)` | JSON AST of the statement |
| `SupportedDialects()` | `[]string` | List of dialect names |
| `Version()` | `string` | Library version |
| `MustCompile(source, dialect)` | `*Result` | Compile or panic |

Requires `libpipeql` on the library path and a cgo-capable toolchain.

## Language Server (`pipeql-lsp`)

The tower-lsp server provides diagnostics, keyword completion, and hover:

```bash
cargo build --release -p pipeql-lsp
# wire it to your editor's stdio-based LSP client
```

Test it end-to-end:

```bash
node crates/pipeql-lsp/tests/smoke.cjs
```

## VS Code extension (`vscode-pipeql`)

Syntax highlighting (TextMate + a matching tree-sitter grammar), snippets, LSP
integration, and a "compile to SQL" command.

```bash
cd extensions/vscode-pipeql && npm install && npm run compile
```

Set `pipeql.lsp.path` to your `pipeql-lsp` binary and `pipeql.cliPath` to the
`pipeql` CLI for the compile command.
