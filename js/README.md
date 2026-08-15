<p align="center">
  <img src="https://raw.githubusercontent.com/Flaxmbot/PipeQL/master/logo.png" alt="PipeQL Logo" width="220" />
</p>

<h1 align="center">PipeQL</h1>
<h3 align="center">Pipelined · Injection-Safe · Polyglot Query Language</h3>

<p align="center">
  <a href="https://github.com/Flaxmbot/PipeQL/actions"><img src="https://img.shields.io/github/actions/workflow/status/Flaxmbot/PipeQL/ci.yml?style=flat-square&logo=github&label=CI" alt="CI" /></a>
  <a href="https://crates.io/crates/pipeql-core"><img src="https://img.shields.io/crates/v/pipeql-core?style=flat-square&logo=rust&logoColor=white&color=e6522c" alt="crates.io" /></a>
  <a href="https://npmjs.com/package/@flaxmbot/pipeql"><img src="https://img.shields.io/npm/v/@flaxmbot/pipeql?style=flat-square&logo=npm&logoColor=white&color=38bdf8" alt="npm" /></a>
  <a href="https://pypi.org/project/pipeql"><img src="https://img.shields.io/pypi/v/pipeql?style=flat-square&logo=pypi&logoColor=white&color=818cf8" alt="PyPI" /></a>
  <a href="https://github.com/Flaxmbot/PipeQL/releases/latest"><img src="https://img.shields.io/github/v/release/Flaxmbot/PipeQL?style=flat-square&logo=github&color=10b981" alt="Release" /></a>
  <a href="https://github.com/Flaxmbot/PipeQL/blob/master/LICENSE"><img src="https://img.shields.io/badge/license-MIT-c084fc?style=flat-square" alt="License" /></a>
</p>

<p align="center">
  <a href="https://pipeql.vercel.app">📚 Docs &amp; Live Playground</a> ·
  <a href="#install">💿 Install</a> ·
  <a href="#syntax-reference">🧠 Syntax Reference</a> ·
  <a href="#sdk-usage">🔌 SDK Usage</a> ·
  <a href="https://github.com/Flaxmbot/PipeQL/releases/latest">⬇️ Download Binaries</a>
</p>

---

**PipeQL** is a compiled query language that transpiles to **parameterized SQL**. You write clean, left-to-right pipelines; the compiler extracts *every* value into bind parameters at the AST level — making SQL injection **mathematically impossible**. One query, four databases: **PostgreSQL, SQLite, DuckDB, and MySQL**.

```pipeql
from orders
| join customers on orders.customer_id == customers.id
| filter orders.status == 'active' and orders.total >= $min
| group [region] (total = sum(orders.total), cnt = count(*))
| filter total > $threshold
| select [region, total, cnt]
| sort [total desc]
| take 10
```

**→ PostgreSQL output:**

```sql
SELECT region, SUM(orders.total) AS total, COUNT(*) AS cnt
FROM orders
INNER JOIN customers ON (orders.customer_id = customers.id)
WHERE ((orders.status = $1) AND (orders.total >= $2))
GROUP BY region HAVING (SUM(orders.total) > $3)
ORDER BY total DESC LIMIT 10;
-- Parameters: [$1='active', $2=min, $3=threshold]
```

Every string literal and `$param` is extracted into a positional bind array at the AST level. **No string concatenation. No escaping. No injection.**

---

## Why PipeQL?

| Problem | PipeQL Solution |
|:---|:---|
| 🛡️ SQL injection | 100% AST-level parameter isolation — impossible to inject |
| 🔀 Dialect lock-in | Write once, compile to Postgres / SQLite / DuckDB / MySQL |
| ↔️ Right-to-left SQL | Clean left-to-right pipeline: `from → filter → select → sort` |
| 🐌 Slow template engines | Native Rust compiler, **~19µs** per query |
| 🌍 Language silos | One compiler, **5 SDKs**: Rust, JS/TS, Python, C/C++, Go |
| 🧩 Fragmented tooling | Built-in LSP, VS Code extension, tree-sitter grammar, CLI |

---

<h2 id="install">💿 Install</h2>

### Rust — CLI & Library

```bash
cargo install pipeql-cli          # CLI tool
```

```toml
# Cargo.toml
[dependencies]
pipeql-core = "1.1.6"
```

### JavaScript / TypeScript

```bash
npm install @flaxmbot/pipeql
```

Compile with analyzer validation derived from your `table` DDL — one call,
no hand-written catalog:

```js
import { compileWithSchema } from '@flaxmbot/pipeql';

await compileWithSchema(
  'from users | filter nme == $x',
  'table users [id integer primary auto, name string]',
); // throws: Unknown column 'nme'. Did you mean 'name'?
```

Prefer explicit steps? `catalogFromSchema(schema)` returns the catalog object
accepted by `compileWithCatalog(source, catalog, dialect)`.

### Python

```bash
pip install pipeql
```

### Go

> ⚠️ **Prerequisites:** the Go binding uses **CGO** and needs the `libpipeql_cffi` shared library on your system. It is a *library*, not a command — use `go get` (inside a Go module), **not** `go install`.

```bash
# 1. Build the shared library
git clone https://github.com/Flaxmbot/PipeQL.git && cd PipeQL
cargo build --release -p pipeql-cffi

# 2. Install the shared library
# Linux:
sudo cp target/release/libpipeql_cffi.so /usr/local/lib/ && sudo ldconfig
# macOS:
sudo cp target/release/libpipeql_cffi.dylib /usr/local/lib/
# Windows: copy target/release/pipeql_cffi.dll next to your binary (or add to PATH)

# 3. Add the module — from INSIDE your Go project (any dir with a go.mod):
go get github.com/Flaxmbot/PipeQL/go@latest

#    Or pin a specific release:
#    go get github.com/Flaxmbot/PipeQL/go@v1.1.6
```

> If you see `go.mod file not found` you are outside a Go module — run `go mod init <yourmodule>` first, or run the `go get` from a project that already has a `go.mod`.

### C / C++

Build the shared library, then link against the header:

```bash
cargo build --release -p pipeql-cffi
# Header:  crates/pipeql-cffi/include/libpipeql.h
# Library: target/release/libpipeql_cffi.{so,dylib,dll}
```

```bash
gcc demo.c -I./crates/pipeql-cffi/include -L./target/release -lpipeql_cffi -o demo
```

> **C package managers:** for distributing the C SDK, [**vcpkg**](https://vcpkg.io) (Microsoft; cross-platform, CMake-native, 2400+ ports) and [**Conan**](https://conan.io) (decentralized, custom remotes, great for versioned dependency graphs) are the two leading choices. **vcpkg** is the easiest first step — create a port in the `ports/` dir, and `vcpkg install pipeql` works on Windows/macOS/Linux out of the box.

### Pre-built Binaries

Download from [GitHub Releases](https://github.com/Flaxmbot/PipeQL/releases/latest):

| Platform | CLI | Shared Library |
|:---|:---|:---|
| Linux x64 | `pipeql-linux-x86_64` | `libpipeql_cffi.so` |
| macOS x64 | `pipeql-macos-x86_64` | `libpipeql_cffi.dylib` |
| Windows x64 | `pipeql-windows-x86_64.exe` | `pipeql_cffi.dll` |

Each release also ships: `@flaxmbot/pipeql` npm tarball, `pipeql` PyPI wheel + sdist, `pipeql-core` crate file, the C SDK header tarball, changelog, and the AI system prompt.

---

<h2 id="syntax-reference">🧠 Syntax Reference</h2>

### Keywords

All PipeQL reserved keywords:

| Keyword | Purpose | Example |
|:---|:---|:---|
| `from` | Source table for reads, updates, deletes | `from users` |
| `into` | Target table for inserts and upserts | `into users` |
| `as` | Alias a table or column | `from users as u`, `select [name as n]` |
| `filter` | Row filtering (WHERE / HAVING) | `filter age >= 18` |
| `select` | Choose output columns | `select [id, name]` |
| `derive` | Add computed columns | `derive [total = price * qty]` |
| `join` | Inner join | `join orders on users.id == orders.uid` |
| `left` | Left outer join modifier | `left join orders on ...` |
| `right` | Right outer join modifier | `right join roles on ...` |
| `full` | Full outer join modifier | `full join archive on ...` |
| `inner` | Explicit inner join modifier | `inner join orders on ...` |
| `on` | Join condition | `join t on a.id == t.id` |
| `group` | Group by with aggregates | `group [region] (total = sum(amt))` |
| `sort` | Order results | `sort [created_at desc]` |
| `take` | Limit rows | `take 25` |
| `skip` | Offset rows | `skip 50` |
| `insert` | Insert values | `insert [name = $name]` |
| `update` | Update values (requires filter) | `update [name = $name]` |
| `delete` | Delete rows (requires filter) | `delete` |
| `upsert` | Insert-or-update values | `upsert [id = $id, name = $n]` |
| `conflict` | Conflict target columns for upsert | `conflict [id]` |
| `do` | Conflict action for upsert | `do update [name = $n]` |
| `union` | Combine result sets (distinct) | `... \| union ...` |
| `all` | Include duplicates in union | `... \| union all ...` |
| `table` | Create a table (DDL) | `table users [...]` |
| `and` | Logical AND | `filter a == 1 and b == 2` |
| `or` | Logical OR | `filter a == 1 or b == 2` |
| `not` | Logical NOT / negation | `filter not active` |
| `in` | Set membership test | `filter id in (1, 2, 3)` |
| `is` | Null check | `filter name is null` |
| `null` | Null literal | `filter name is not null` |
| `true` | Boolean true | `filter active == true` |
| `false` | Boolean false | `filter active == false` |
| `asc` | Sort ascending | `sort [name asc]` |
| `desc` | Sort descending | `sort [created_at desc]` |

### Pipeline Stages

Every PipeQL query starts with a source table and chains stages with `|`:

```
from <table> [as <alias>]
| <stage>
| <stage>
| ...
```

| Stage | Syntax | SQL Equivalent |
|:---|:---|:---|
| **from** | `from users` | `FROM users` |
| **from (alias)** | `from users as u` | `FROM users u` |
| **filter** | `filter age >= 18 and active == true` | `WHERE age >= 18 AND active = TRUE` |
| **select** | `select [id, name, email]` | `SELECT id, name, email` |
| **select (alias)** | `select [full_name as name]` | `SELECT full_name AS name` |
| **select (star)** | `select [*]` | `SELECT *` |
| **derive** | `derive [total = price * qty]` | `SELECT *, (price * qty) AS total` |
| **join** | `join orders on users.id == orders.user_id` | `INNER JOIN orders ON ...` |
| **left join** | `left join orders on users.id == orders.uid` | `LEFT JOIN orders ON ...` |
| **right join** | `right join roles on users.role_id == roles.id` | `RIGHT JOIN roles ON ...` |
| **full join** | `full join archive on a.id == archive.id` | `FULL JOIN archive ON ...` |
| **group** | `group [region] (total = sum(amount))` | `GROUP BY region` |
| **sort** | `sort [created_at desc, name asc]` | `ORDER BY created_at DESC, name ASC` |
| **take** | `take 25` | `LIMIT 25` |
| **skip** | `skip 50` | `OFFSET 50` |

### Parameters

Parameters are auto-extracted from the query and converted to dialect-specific placeholders:

| Syntax | Description | Postgres | SQLite / DuckDB / MySQL |
|:---|:---|:---|:---|
| `$name` | Named parameter | `$1` | `?` |
| `${name}` | Braced parameter | `$1` | `?` |
| `'literal'` | String literal (auto-extracted) | `$1` | `?` |

```pipeql
from users | filter email == $email and role == 'admin'
-- Postgres: WHERE (email = $1) AND (role = $2)  → params: ["email", "admin"]
-- SQLite:   WHERE (email = ?) AND (role = ?)    → params: ["email", "admin"]
```

### Expressions & Operators

| Category | Operators | Example |
|:---|:---|:---|
| **Comparison** | `==` `!=` `<` `<=` `>` `>=` | `filter price >= 10` |
| **Logical** | `and` `or` `not` | `filter a == 1 and not b` |
| **Null checks** | `is null` `is not null` | `filter name is not null` |
| **Set membership** | `in (...)` `not in (...)` | `filter id in (1, 2, 3)` |
| **Subquery** | `in (from ... \| select ...)` | `filter id in (from t \| select [id])` |
| **Arithmetic** | `+` `-` `*` `/` | `derive [total = price * qty]` |
| **Functions** | `count(*)` `sum()` `avg()` `min()` `max()` `coalesce()` | `group [r] (n = count(*))` |
| **Column ref** | `table.column` | `filter users.id == orders.uid` |
| **Literals** | integers, floats, strings, booleans, null | `42` `3.14` `'text'` `true` `null` |

### Mutations (DML)

#### Insert

```pipeql
into users | insert [name = $name, email = $email]
```

→ `INSERT INTO users (name, email) VALUES ($1, $2) RETURNING *;`

#### Update

```pipeql
from users | filter id == $id | update [name = $name, email = $email]
```

→ `UPDATE users SET name = $1, email = $2 WHERE (id = $3);`

> ⚠️ `update` **requires** a preceding `filter` stage — PipeQL enforces this to prevent accidental mass updates.

#### Delete

```pipeql
from users | filter id == $id | delete
```

→ `DELETE FROM users WHERE (id = $1);`

> ⚠️ `delete` **requires** a preceding `filter` stage — same safety enforcement as `update`.

#### Upsert (Insert or Update on Conflict)

```pipeql
into users
| upsert [id = $id, name = $name, email = $email]
| conflict [id]
| do update [name = $name, email = $email]
```

| Dialect | Output |
|:---|:---|
| Postgres / SQLite / DuckDB | `INSERT INTO users (...) VALUES (...) ON CONFLICT (id) DO UPDATE SET name = $4, email = $5;` |
| MySQL | `INSERT INTO users (...) VALUES (...) ON DUPLICATE KEY UPDATE name = VALUES(name), email = VALUES(email);` |

### Union

```pipeql
from active_users | select [id, name]
| union
from archived_users | select [id, name]
```

→ `SELECT id, name FROM active_users UNION SELECT id, name FROM archived_users;`

Use `union all` to include duplicates.

### Subqueries

```pipeql
from orders
| filter customer_id in (from vip_customers | select [id])
| select [order_id, total]
```

→ `SELECT order_id, total FROM orders WHERE customer_id IN (SELECT id FROM vip_customers);`

### DDL (Table Schema)

```pipeql
table users [
  id integer primary_key auto_increment,
  name string not_null,
  email string not_null unique,
  active bool default true,
  created_at timestamp default '2024-01-01'
]
```

| Type | Column Modifiers |
|:---|:---|
| `integer`, `float`, `string`, `bool`, `timestamp` | `primary_key`, `auto_increment`, `not_null`, `unique`, `default <value>` |

### Comments

```pipeql
-- This is a line comment
from users | select [id, name]  -- inline comment
```

Comments are preserved in the lossless AST for IDE tooling.

---

<h2 id="sdk-usage">🔌 SDK Usage</h2>

### Rust

```rust
use pipeql_core::api;

let result = api::compile("from users | filter id == $id | select [name]", "postgres").unwrap();
println!("{}", result.sql);      // SELECT name FROM users WHERE (id = $1);
println!("{:?}", result.params); // ["id"]
```

### JavaScript / TypeScript

```javascript
import { compile } from '@flaxmbot/pipeql';

const { sql, params } = compile(
  "from notes | filter category == $cat | sort [updated_at desc]",
  "sqlite"
);
console.log(sql);    // SELECT * FROM notes WHERE (category = ?) ORDER BY updated_at DESC;
console.log(params); // ["cat"]
```

#### Driver adapters — zero-boilerplate DB wrappers

```javascript
import { createPipeqlDriver } from '@flaxmbot/pipeql/driver';
import sqlite3 from 'sqlite3';

const db = createPipeqlDriver(new sqlite3.Database('app.db'), { dialect: 'sqlite' });

const rows = await db.query('from users | filter role == $role', { role: 'admin' });
const { lastId, changes } = await db.execute('into notes | insert [title = $title]', { title: 'Hi' });
const newNote = await db.insertAndFetch('into notes | insert $data', req.body);
```

> ⚠️ A `$param` that you forget to supply in the params object raises a clear
> error (`missing value for parameter '$role'`) instead of silently binding the
> parameter's name — a typo can never quietly return wrong data. `union`
> queries return rows just like `select`; only mutations and DDL dispatch to
> `.run()`/`execute()`.

### Python

```python
import pipeql_python as pipeql

res = pipeql.compile("into users | insert [name = $name, email = $email]", "postgres")
print(res["sql"])    # INSERT INTO users (name, email) VALUES ($1, $2) RETURNING *;
print(res["params"]) # ["name", "email"]
```

```python
from pipeql_python.driver import create_pipeql_driver
import sqlite3

db = create_pipeql_driver(sqlite3.connect('app.db'))
rows = db.query("from users | filter role == $role", {"role": "admin"})
```

### Go

```go
package main

import (
    "fmt"
    "log"
    pipeql "github.com/Flaxmbot/PipeQL/go"
)

func main() {
    res, err := pipeql.Compile("from users | filter age >= $min | select [id, name]", "postgres")
    if err != nil { log.Fatal(err) }
    fmt.Println("SQL:", res.SQL)       // SELECT id, name FROM users WHERE (age >= $1);
    fmt.Println("Params:", res.Params) // ["min"]
}
```

### C

```c
#include <stdio.h>
#include "libpipeql.h"

int main() {
    PipeqlError err = {0};
    PipeqlResult* res = pipeql_compile("from users | filter id == $id", "postgres", &err);
    if (!res) { fprintf(stderr, "Error: %s\n", err.message); return 1; }
    printf("SQL: %s\n", res->sql);
    pipeql_result_free(res);
    return 0;
}
```

### CLI

```bash
pipeql compile "from users | take 10" --dialect postgres
pipeql compile "from users | filter id == $id" --dialect sqlite
pipeql parse "from users | select [id, name]"
pipeql dialects
pipeql version
```

### Optional: Fluent builder (programmatic composition)

The **string DSL above is PipeQL's primary interface.** For composing queries in
code — conditional or looped pipeline stages, or object-style inserts — the SDK
also ships an **optional fluent builder**. A builder query and a hand-written
string query are *provably identical*: the builder assembles the exact same
source string and hands it to the same compiler.

```javascript
import { PipeQL } from '@flaxmbot/pipeql/builder';

const q = PipeQL.from('notes')
  .filter('is_archived == 0')
  .sort(['created_at desc'])
  .take(10);
const { sql, params } = await q.compile('sqlite');

// Object inserts auto-generate $b0, $b1, ... bind params
const ins = PipeQL.into('notes').insert({ title: 'Hi', flag: 1 });
// source: "into notes | insert [title = $b0, flag = $b1]"  values: { b0: 'Hi', b1: 1 }
```

The builder duck-types through the driver too: `db.query(q)` and
`db.execute(q)` both accept a `PipeQL` instance, and builder values merge with
your params automatically. The same builder API ships in all 5 SDKs (Rust,
Python, Go, C) — see the [main README](../README.md) and the
[docs site](https://pipeql.vercel.app).

---

## Ecosystem

| Component | Description |
|:---|:---|
| [`pipeql-core`](crates/pipeql-core) | Core compiler: lexer → parser → AST → codegen |
| [`pipeql-cli`](crates/pipeql-cli) | Command-line tool |
| [`pipeql-wasm`](crates/pipeql-wasm) | WebAssembly target for browsers |
| [`pipeql-python`](crates/pipeql-python) | Python binding (PyO3, ABI3) → `pipeql` on PyPI |
| [`pipeql-cffi`](crates/pipeql-cffi) | C ABI shared library |
| [`pipeql-lsp`](crates/pipeql-lsp) | Language Server Protocol |
| [`js/`](js) | JavaScript/TypeScript SDK (`@flaxmbot/pipeql`) |
| [`go/`](go) | Go binding (CGO) — `github.com/Flaxmbot/PipeQL/go` |
| [`python/`](python) | Python package + driver adapters |
| [`docs-web/`](docs-web) | Interactive documentation + WASM playground |
| [`extensions/`](extensions) | VS Code extension |
| [`tree-sitter-pipeql/`](tree-sitter-pipeql) | Tree-sitter grammar |

---

## Compiler Architecture

```
Source Text ──→ Lexer ──→ Tokens ──→ Parser ──→ AST ──→ Codegen ──→ SQL + Params
                          │                     │                    │
                          │                     │                    ├─ PostgreSQL ($1, $2)
                          │                     │                    ├─ SQLite     (?, ?)
                          │                     │                    ├─ DuckDB     (?, ?)
                          │                     │                    └─ MySQL      (?, ?)
                          │                     │
                          │                     └─ Lossless AST (spans + comments)
                          │
                          └─ Character-level span tracking
```

1. **Lexer** — hand-written tokenizer with exact character positions for IDE support
2. **Parser** — Pratt parser producing a lossless abstract syntax tree
3. **Codegen** — walks the AST, extracts all values into bind parameters, emits dialect-specific SQL

All language bindings are thin wrappers calling the Rust core through `pipeql-core`'s API.

**Safety:** `#![deny(unsafe_code)]` enforced across the entire compiler core.

---

## Error Messages

PipeQL provides compiler-grade error messages with exact positions and actionable suggestions:

```
Error at line 1, col 1: Unknown keyword 'selct'
  hint: Did you mean 'select'?

Error at line 1, col 35: 'update' requires a preceding 'filter' stage
  hint: Add a filter to prevent accidental mass updates.
  help: from users | filter id == $id | update [...]

Error at line 1, col 15: Unclosed string literal
  hint: Add a closing single quote (') to terminate the string.
```

Features: Levenshtein-based fuzzy keyword matching, contextual hints, unclosed string/subquery detection, duplicate column detection, empty pipeline errors, filter-before-mutate enforcement.

---

## AI & LLM Integration

PipeQL ships with an optimized **System Prompt** for code generation with LLMs (GPT-4, Claude, Gemini, etc.):

```python
# Python
import pipeql_python
system_prompt = pipeql_python.SYSTEM_PROMPT
```

```javascript
// JavaScript — bundled in npm package
import prompt from '@flaxmbot/pipeql/ai/system_prompt.md';
```

Direct download: [`ai/system_prompt.md`](https://raw.githubusercontent.com/Flaxmbot/PipeQL/master/ai/system_prompt.md)

---

## Contributing

```bash
git clone https://github.com/Flaxmbot/PipeQL.git && cd PipeQL
cargo test --workspace              # Run all tests
cargo bench -p pipeql-core          # Benchmarks
cargo build --release -p pipeql-cli # Build CLI
```

---

## License

[MIT](LICENSE) © Flaxmbot
