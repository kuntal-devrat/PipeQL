// Package pipeql provides zero-boilerplate database adapters for Go standard
// library's database/sql package.
//
// WrapDB wraps any *sql.DB connection (PostgreSQL, SQLite, DuckDB, MySQL) and
// auto-handles:
//   - compilation (PipeQL source -> dialect SQL)
//   - parameter binding (named PipeQL params map -> positional []any driver args)
//   - $data object expansion (partial insert/update with zero boilerplate)
//
// Usage:
//
//	db, _ := sql.Open("sqlite3", ":memory:")
//	pdb := pipeql.WrapDB(db, "sqlite")
//
//	rows, err := pdb.Query(ctx, "from users | filter age >= $min", map[string]any{"min": 18})
//	res, err := pdb.Exec(ctx, "into notes | insert [title = $title]", map[string]any{"title": "Note 1"})
//	res, err := pdb.Exec(ctx, "into notes | insert $data", map[string]any{"data": map[string]any{"title": "Note 2"}})
package pipeql

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"regexp"
	"sort"
	"strings"
)

var (
	dataRegex  = regexp.MustCompile(`\$data\b`)
	identRegex = regexp.MustCompile(`^[A-Za-z_][A-Za-z0-9_]*$`)
)

// PipeqlDB wraps *sql.DB with PipeQL compilation and parameter binding.
type PipeqlDB struct {
	*sql.DB
	Dialect string
}

// PipeqlTx wraps *sql.Tx with PipeQL compilation and parameter binding.
type PipeqlTx struct {
	*sql.Tx
	Dialect string
}

// PipeqlConn wraps *sql.Conn with PipeQL compilation and parameter binding.
type PipeqlConn struct {
	*sql.Conn
	Dialect string
}

// WrapDB wraps a *sql.DB with the target PipeQL SQL dialect ("postgres", "sqlite", "duckdb", "mysql").
func WrapDB(db *sql.DB, dialect string) *PipeqlDB {
	if dialect == "" {
		dialect = "postgres"
	}
	return &PipeqlDB{DB: db, Dialect: dialect}
}

// WrapTx wraps a *sql.Tx with the target PipeQL SQL dialect.
func WrapTx(tx *sql.Tx, dialect string) *PipeqlTx {
	if dialect == "" {
		dialect = "postgres"
	}
	return &PipeqlTx{Tx: tx, Dialect: dialect}
}

// WrapConn wraps a *sql.Conn with the target PipeQL SQL dialect.
func WrapConn(conn *sql.Conn, dialect string) *PipeqlConn {
	if dialect == "" {
		dialect = "postgres"
	}
	return &PipeqlConn{Conn: conn, Dialect: dialect}
}

// QuerySource extracts the source string from a string or a *Query builder.
func querySourceOf(source any) (string, error) {
	switch s := source.(type) {
	case string:
		return s, nil
	case *Query:
		return s.Source(), nil
	case interface{ Source() string }:
		return s.Source(), nil
	default:
		return "", fmt.Errorf("pipeql: source must be a string or query builder, got %T", source)
	}
}

func expandData(source string, data map[string]any) (string, map[string]any, error) {
	if len(data) == 0 {
		return "", nil, fmt.Errorf("pipeql: $data object expansion requires at least one property")
	}

	// Sort keys for deterministic column ordering
	keys := make([]string, 0, len(data))
	for k := range data {
		if !identRegex.MatchString(k) {
			return "", nil, fmt.Errorf("pipeql: cannot expand $data: column %q is not a valid identifier", k)
		}
		keys = append(keys, k)
	}
	sort.Strings(keys)

	parts := dataRegex.Split(source, -1)
	var chunks []string
	chunks = append(chunks, parts[0])
	values := make(map[string]any)
	n := 0

	for i := 1; i < len(parts); i++ {
		prev := strings.TrimRight(chunks[len(chunks)-1], " \t\r\n")
		var last byte
		if len(prev) > 0 {
			last = prev[len(prev)-1]
		}
		inBrackets := last == '[' || last == ','

		var assignments []string
		for _, key := range keys {
			pname := fmt.Sprintf("data%d", n)
			n++
			values[pname] = data[key]
			assignments = append(assignments, fmt.Sprintf("%s = $%s", key, pname))
		}
		body := strings.Join(assignments, ", ")
		if inBrackets {
			chunks = append(chunks, body)
		} else {
			chunks = append(chunks, "["+body+"]")
		}
		chunks = append(chunks, parts[i])
	}

	return strings.Join(chunks, ""), values, nil
}

// CompileAndBind prepares source and binds named params into positional driver arguments.
func CompileAndBind(source any, dialect string, params map[string]any) (string, []any, *Result, error) {
	src, err := querySourceOf(source)
	if err != nil {
		return "", nil, nil, err
	}

	mergedParams := make(map[string]any, len(params))
	for k, v := range params {
		mergedParams[k] = v
	}

	if dataRegex.MatchString(src) {
		dataObj, hasData := mergedParams["data"]
		var dataMap map[string]any
		if hasData {
			if m, ok := dataObj.(map[string]any); ok {
				dataMap = m
			}
		}
		if dataMap == nil {
			dataMap = mergedParams
		}
		expandedSrc, dataVals, err := expandData(src, dataMap)
		if err != nil {
			return "", nil, nil, err
		}
		src = expandedSrc
		for k, v := range dataVals {
			mergedParams[k] = v
		}
	}

	res, err := Compile(src, dialect)
	if err != nil {
		return "", nil, nil, err
	}

	// Extract parameter types from analysis
	type paramMeta struct {
		Name string `json:"name"`
		Ty   string `json:"ty"`
	}
	type analysisDoc struct {
		ParamMap []paramMeta `json:"param_map"`
	}
	var analysis analysisDoc
	if len(res.Analysis) > 0 {
		_ = json.Unmarshal(res.Analysis, &analysis)
	}
	paramTypes := make(map[string]string, len(analysis.ParamMap))
	for _, p := range analysis.ParamMap {
		paramTypes[p.Name] = p.Ty
	}

	args := make([]any, 0, len(res.Params))
	for _, name := range res.Params {
		if val, exists := mergedParams[name]; exists {
			args = append(args, val)
		} else if paramTypes[name] == "any" || paramTypes[name] == "Any" {
			return "", nil, nil, fmt.Errorf("pipeql: missing value for parameter $%s — pass in params map", name)
		} else {
			// Literal-derived parameter (in select/filter)
			args = append(args, name)
		}
	}

	return res.SQL, args, res, nil
}

// Query executes a PipeQL query against the database, returning rows.
func (db *PipeqlDB) Query(ctx context.Context, source any, params map[string]any) (*sql.Rows, error) {
	sqlStr, args, _, err := CompileAndBind(source, db.Dialect, params)
	if err != nil {
		return nil, err
	}
	return db.DB.QueryContext(ctx, sqlStr, args...)
}

// QueryRow executes a PipeQL query and returns a single row.
func (db *PipeqlDB) QueryRow(ctx context.Context, source any, params map[string]any) (*sql.Row, error) {
	sqlStr, args, _, err := CompileAndBind(source, db.Dialect, params)
	if err != nil {
		return nil, err
	}
	return db.DB.QueryRowContext(ctx, sqlStr, args...), nil
}

// Exec executes a PipeQL mutation/DDL statement and returns sql.Result.
func (db *PipeqlDB) Exec(ctx context.Context, source any, params map[string]any) (sql.Result, error) {
	sqlStr, args, _, err := CompileAndBind(source, db.Dialect, params)
	if err != nil {
		return nil, err
	}
	return db.DB.ExecContext(ctx, sqlStr, args...)
}

// BeginTx starts a transaction wrapped with PipeQL capabilities.
func (db *PipeqlDB) BeginTx(ctx context.Context, opts *sql.TxOptions) (*PipeqlTx, error) {
	tx, err := db.DB.BeginTx(ctx, opts)
	if err != nil {
		return nil, err
	}
	return &PipeqlTx{Tx: tx, Dialect: db.Dialect}, nil
}

// Query executes a PipeQL query in a transaction.
func (tx *PipeqlTx) Query(ctx context.Context, source any, params map[string]any) (*sql.Rows, error) {
	sqlStr, args, _, err := CompileAndBind(source, tx.Dialect, params)
	if err != nil {
		return nil, err
	}
	return tx.Tx.QueryContext(ctx, sqlStr, args...)
}

// QueryRow executes a PipeQL query in a transaction and returns a single row.
func (tx *PipeqlTx) QueryRow(ctx context.Context, source any, params map[string]any) (*sql.Row, error) {
	sqlStr, args, _, err := CompileAndBind(source, tx.Dialect, params)
	if err != nil {
		return nil, err
	}
	return tx.Tx.QueryRowContext(ctx, sqlStr, args...), nil
}

// Exec executes a PipeQL mutation/DDL statement in a transaction.
func (tx *PipeqlTx) Exec(ctx context.Context, source any, params map[string]any) (sql.Result, error) {
	sqlStr, args, _, err := CompileAndBind(source, tx.Dialect, params)
	if err != nil {
		return nil, err
	}
	return tx.Tx.ExecContext(ctx, sqlStr, args...)
}

// Query executes a PipeQL query on a connection.
func (c *PipeqlConn) Query(ctx context.Context, source any, params map[string]any) (*sql.Rows, error) {
	sqlStr, args, _, err := CompileAndBind(source, c.Dialect, params)
	if err != nil {
		return nil, err
	}
	return c.Conn.QueryContext(ctx, sqlStr, args...)
}

// Exec executes a PipeQL mutation/DDL statement on a connection.
func (c *PipeqlConn) Exec(ctx context.Context, source any, params map[string]any) (sql.Result, error) {
	sqlStr, args, _, err := CompileAndBind(source, c.Dialect, params)
	if err != nil {
		return nil, err
	}
	return c.Conn.ExecContext(ctx, sqlStr, args...)
}
