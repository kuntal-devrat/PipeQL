<p align="center">
  <img src="https://raw.githubusercontent.com/Flaxmbot/PipeQL/master/logo.png" alt="PipeQL Logo" width="220" />
</p>

<h1 align="center">PipeQL · Go</h1>
<h3 align="center">Pipelined · Injection-Safe · Polyglot Query Language</h3>

<p align="center">
  <a href="https://github.com/Flaxmbot/PipeQL/actions"><img src="https://img.shields.io/github/actions/workflow/status/Flaxmbot/PipeQL/ci.yml?style=flat-square&logo=github&label=CI" alt="CI" /></a>
  <a href="https://pkg.go.dev/github.com/Flaxmbot/PipeQL/go"><img src="https://img.shields.io/badge/go-doc-00ADD8?style=flat-square&logo=go" alt="GoDoc" /></a>
  <a href="https://github.com/Flaxmbot/PipeQL/blob/master/LICENSE"><img src="https://img.shields.io/badge/license-MIT-c084fc?style=flat-square" alt="License" /></a>
</p>

<p align="center">
  <a href="https://pipeql.vercel.app">📚 Docs &amp; Live Playground</a> · <a href="https://github.com/Flaxmbot/PipeQL">🔗 Monorepo</a>
</p>

---

**PipeQL** is a compiled query language that transpiles to **parameterized SQL**.
You write clean, left-to-right pipelines; the compiler extracts *every* value
into bind parameters at the AST level — making SQL injection **mathematically
impossible**. One query, four databases: **PostgreSQL, SQLite, DuckDB, and
MySQL**.

This is the **Go binding** (CGO). The same compiler powers SDKs for Rust,
JavaScript/TypeScript, Python, and C.

## Install

> ⚠️ **Prerequisites:** the Go binding uses **CGO** and needs the
> `libpipeql_cffi` shared library on your system. It is a *library*, not a
> command — use `go get` (inside a Go module), **not** `go install`.

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

> If you see `go.mod file not found` you are outside a Go module — run
> `go mod init <yourmodule>` first, or run the `go get` from a project that
> already has a `go.mod`.

## Usage

The **string DSL is PipeQL's primary interface** — most queries are one line:

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

Compile with analyzer validation derived from your `table` DDL — one call, no
hand-written catalog:

```go
res, err := pipeql.CompileWithSchema(
    "from users | filter nme == $x", "postgres",
    "table users [id integer primary auto, name string]",
)
// err: Unknown column 'nme'...
```

Prefer explicit steps? `CatalogFromSchema(schema)` returns the catalog JSON
accepted by `CompileWithCatalog(source, dialect, catalogJSON)`.

### Optional: Fluent builder (programmatic composition)

For composing queries in code — conditional or looped pipeline stages, or
object-style inserts — the Go SDK ships an **optional fluent builder**. A
builder query and a hand-written string query are *provably identical*: the
builder assembles the exact same source string and hands it to the same
compiler.

```go
q := pipeql.From("notes").
    Filter("is_archived == 0").
    Sort([]string{"created_at desc"}).
    Take(10)
res, err := q.Compile("postgres")

// Object inserts accept map[string]any (keys sorted for deterministic SQL);
// use PairsOf for exact column order.
ins := pipeql.Into("notes").Insert(pipeql.PairsOf("title", "Hi", "flag", 1))
// source: "into notes | insert [title = $b0, flag = $b1]"
```

The same builder API ships in all 5 SDKs (Rust, JS/TS, Python, Go, C) — see
the [docs site](https://pipeql.vercel.app).

## Supported Dialects

PostgreSQL, SQLite, DuckDB, MySQL

## License

[MIT](https://github.com/Flaxmbot/PipeQL/blob/master/LICENSE) © Flaxmbot
