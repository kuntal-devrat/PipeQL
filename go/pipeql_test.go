package pipeql

import (
	"strings"
	"testing"
)

func TestCompilePostgres(t *testing.T) {
	res, err := Compile(
		"from users | filter age >= $min and status == 'active' | select [id, name] | sort [name asc] | take 10",
		"postgres",
	)
	if err != nil {
		t.Fatalf("compile: %v", err)
	}
	if !strings.Contains(res.SQL, "SELECT id, name FROM users") {
		t.Errorf("unexpected sql: %s", res.SQL)
	}
	if !strings.Contains(res.SQL, "$1") {
		t.Errorf("expected postgres placeholder in %s", res.SQL)
	}
	if len(res.Params) != 2 {
		t.Errorf("expected 2 params, got %v", res.Params)
	}
}

func TestCompileSQLiteUsesQuestionMarks(t *testing.T) {
	res, err := Compile("from t | filter id == $id | take 5", "sqlite")
	if err != nil {
		t.Fatalf("compile: %v", err)
	}
	if !strings.Contains(res.SQL, "?") {
		t.Errorf("expected ? placeholder in %s", res.SQL)
	}
}

func TestCompileError(t *testing.T) {
	_, err := Compile("from users | explode", "postgres")
	if err == nil {
		t.Fatal("expected error")
	}
	perr, ok := err.(*Err)
	if !ok {
		t.Fatalf("expected *Err, got %T", err)
	}
	if perr.Kind != ErrParse {
		t.Errorf("expected ErrParse, got %d", perr.Kind)
	}
	if !strings.Contains(perr.Message, "explode") {
		t.Errorf("message should mention bad step: %s", perr.Message)
	}
}

func TestMustCompile(t *testing.T) {
	res := MustCompile("from t | select [*]", "postgres")
	if !strings.Contains(res.SQL, "SELECT * FROM t") {
		t.Errorf("unexpected sql: %s", res.SQL)
	}
}

func TestStatementMetadata(t *testing.T) {
	sel, err := Compile("from users | filter id == $id | select [id]", "postgres")
	if err != nil {
		t.Fatalf("compile: %v", err)
	}
	if sel.StatementType != "select" {
		t.Errorf("expected statement_type 'select', got %q", sel.StatementType)
	}
	if sel.IsMutation {
		t.Error("select must not be a mutation")
	}

	ins, err := Compile("into notes | insert [title = $t]", "sqlite")
	if err != nil {
		t.Fatalf("compile insert: %v", err)
	}
	if ins.StatementType != "insert" {
		t.Errorf("expected statement_type 'insert', got %q", ins.StatementType)
	}
	if !ins.IsMutation {
		t.Error("insert must be a mutation")
	}

	upd, err := Compile("from notes | filter id == $id | update [is_pinned = 1]", "sqlite")
	if err != nil {
		t.Fatalf("compile update: %v", err)
	}
	if upd.StatementType != "update" {
		t.Errorf("expected statement_type 'update', got %q", upd.StatementType)
	}

	del, err := Compile("from notes | filter id == $id | delete", "sqlite")
	if err != nil {
		t.Fatalf("compile delete: %v", err)
	}
	if del.StatementType != "delete" {
		t.Errorf("expected statement_type 'delete', got %q", del.StatementType)
	}

	ddl, err := Compile("table notes [id int primary auto]", "sqlite")
	if err != nil {
		t.Fatalf("compile ddl: %v", err)
	}
	if ddl.StatementType != "create_table" {
		t.Errorf("expected statement_type 'create_table', got %q", ddl.StatementType)
	}
	if ddl.IsMutation {
		t.Error("create_table must not be a mutation")
	}
}

func TestVersion(t *testing.T) {
	if Version() == "" {
		t.Error("empty version")
	}
}

func TestStatementMetadataUpsert(t *testing.T) {
	res, err := Compile(
		"into users | upsert [name = $name, email = $email] | conflict [email] | do update [name = $name]",
		"postgres",
	)
	if err != nil {
		t.Fatalf("compile: %v", err)
	}
	if res.StatementType != "upsert" {
		t.Errorf("expected statement_type 'upsert', got %q", res.StatementType)
	}
	if !res.IsMutation {
		t.Error("upsert must be a mutation")
	}
	if !strings.Contains(res.SQL, "ON CONFLICT (email) DO UPDATE SET name = $1") {
		t.Errorf("unexpected sql: %s", res.SQL)
	}
}

func TestStatementMetadataUnion(t *testing.T) {
	res, err := Compile(
		"from active_users | select [id, name] | union from archived_users | select [id, name]",
		"postgres",
	)
	if err != nil {
		t.Fatalf("compile: %v", err)
	}
	if res.StatementType != "union" {
		t.Errorf("expected statement_type 'union', got %q", res.StatementType)
	}
	if res.IsMutation {
		t.Error("union must not be a mutation")
	}
	if !strings.Contains(res.SQL, "UNION") {
		t.Errorf("unexpected sql: %s", res.SQL)
	}
}

func TestCompileUnionAll(t *testing.T) {
	res, err := Compile(
		"from active_users | select [id] | union all from archived_users | select [id]",
		"postgres",
	)
	if err != nil {
		t.Fatalf("compile: %v", err)
	}
	if !strings.Contains(res.SQL, "UNION ALL") {
		t.Errorf("expected UNION ALL in sql: %s", res.SQL)
	}
}

func TestCompileSubquery(t *testing.T) {
	res, err := Compile(
		"from orders | filter customer_id in (from customers | filter region == 'EU' | select [id])",
		"postgres",
	)
	if err != nil {
		t.Fatalf("compile: %v", err)
	}
	if !strings.Contains(res.SQL, "IN (SELECT id FROM customers") {
		t.Errorf("unexpected sql: %s", res.SQL)
	}
	if len(res.Params) != 1 || res.Params[0] != "EU" {
		t.Errorf("expected params [EU], got %v", res.Params)
	}
}

// --- New tests for gap-fill functions ---

func TestParameterCount(t *testing.T) {
	res, err := Compile(
		"from users | filter age >= $min and status == $s | select [id]",
		"postgres",
	)
	if err != nil {
		t.Fatalf("compile: %v", err)
	}
	if res.ParameterCount != 2 {
		t.Errorf("expected ParameterCount=2, got %d", res.ParameterCount)
	}
	if res.ParameterCount != len(res.Params) {
		t.Errorf("ParameterCount=%d != len(Params)=%d", res.ParameterCount, len(res.Params))
	}
}

func TestCompileWithCatalogValid(t *testing.T) {
	catalog := `{"tables":{"users":{"name":"users","columns":[{"name":"id","ty":"Integer"},{"name":"name","ty":"String"}]}}}`
	res, err := CompileWithCatalog("from users | select [id, name]", "postgres", catalog)
	if err != nil {
		t.Fatalf("compile with valid catalog: %v", err)
	}
	if !strings.Contains(res.SQL, "SELECT id, name FROM users") {
		t.Errorf("unexpected sql: %s", res.SQL)
	}
}

func TestCompileWithCatalogInvalidColumn(t *testing.T) {
	catalog := `{"tables":{"users":{"name":"users","columns":[{"name":"id","ty":"Integer"}]}}}`
	_, err := CompileWithCatalog("from users | select [nope]", "postgres", catalog)
	if err == nil {
		t.Fatal("expected error for unknown column")
	}
	perr, ok := err.(*Err)
	if !ok {
		t.Fatalf("expected *Err, got %T", err)
	}
	if perr.Kind != ErrAnalysis {
		t.Errorf("expected ErrAnalysis, got kind=%d msg=%s", perr.Kind, perr.Message)
	}
	if !strings.Contains(perr.Message, "nope") {
		t.Errorf("error should mention column: %s", perr.Message)
	}
}

func TestCompileWithCatalogEmpty(t *testing.T) {
	res, err := CompileWithCatalog("from users | select [id]", "sqlite", "")
	if err != nil {
		t.Fatalf("compile with empty catalog: %v", err)
	}
	if !strings.Contains(res.SQL, "SELECT id FROM users") {
		t.Errorf("unexpected sql: %s", res.SQL)
	}
}

func TestParse(t *testing.T) {
	ast, err := Parse("from users | filter id == $id | select [id]")
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	if len(ast) == 0 {
		t.Fatal("empty AST")
	}
	if !strings.Contains(string(ast), "users") {
		t.Errorf("AST should mention source table: %s", ast)
	}
}

func TestParseError(t *testing.T) {
	_, err := Parse("from users | explode")
	if err == nil {
		t.Fatal("expected parse error")
	}
	perr, ok := err.(*Err)
	if !ok {
		t.Fatalf("expected *Err, got %T", err)
	}
	if perr.Kind != ErrParse {
		t.Errorf("expected ErrParse, got %d", perr.Kind)
	}
}

func TestSupportedDialects(t *testing.T) {
	dialects := SupportedDialects()
	if len(dialects) != 4 {
		t.Fatalf("expected 4 dialects, got %d: %v", len(dialects), dialects)
	}
	expected := map[string]bool{"postgres": true, "sqlite": true, "duckdb": true, "mysql": true}
	for _, d := range dialects {
		if !expected[d] {
			t.Errorf("unexpected dialect: %s", d)
		}
	}
}

func TestCompileAllDialects(t *testing.T) {
	dialects := []string{"postgres", "sqlite", "duckdb", "mysql"}
	for _, dialect := range dialects {
		res, err := Compile("from users | filter id == $id | take 5", dialect)
		if err != nil {
			t.Errorf("compile failed for %s: %v", dialect, err)
			continue
		}
		if res.ParameterCount != 1 {
			t.Errorf("dialect %s: expected 1 param, got %d", dialect, res.ParameterCount)
		}
	}
}

func TestParseAllDialectsUpsert(t *testing.T) {
	dialects := []string{"postgres", "sqlite", "duckdb", "mysql"}
	for _, dialect := range dialects {
		res, err := Compile(
			"into users | upsert [name = $n, email = $e] | conflict [email] | do update [name = $n]",
			dialect,
		)
		if err != nil {
			t.Errorf("upsert compile failed for %s: %v", dialect, err)
			continue
		}
		if res.StatementType != "upsert" {
			t.Errorf("dialect %s: expected upsert, got %s", dialect, res.StatementType)
		}
		if !res.IsMutation {
			t.Errorf("dialect %s: upsert should be mutation", dialect)
		}
	}
}

func TestParseAllDialectsUnion(t *testing.T) {
	dialects := []string{"postgres", "sqlite", "duckdb", "mysql"}
	for _, dialect := range dialects {
		res, err := Compile(
			"from a | select [id] | union all from b | select [id]",
			dialect,
		)
		if err != nil {
			t.Errorf("union compile failed for %s: %v", dialect, err)
			continue
		}
		if res.StatementType != "union" {
			t.Errorf("dialect %s: expected union, got %s", dialect, res.StatementType)
		}
	}
}

func TestParseAllDialectsSubquery(t *testing.T) {
	dialects := []string{"postgres", "sqlite", "duckdb", "mysql"}
	for _, dialect := range dialects {
		res, err := Compile(
			"from orders | filter customer_id in (from customers | filter region == $r | select [id])",
			dialect,
		)
		if err != nil {
			t.Errorf("subquery compile failed for %s: %v", dialect, err)
			continue
		}
		if !strings.Contains(res.SQL, "IN (") {
			t.Errorf("dialect %s: expected IN clause, got %s", dialect, res.SQL)
		}
	}
}

func TestCompileWithCatalogAllDialects(t *testing.T) {
	catalog := `{"tables":{"users":{"name":"users","columns":[{"name":"id","ty":"Integer"},{"name":"name","ty":"String"}]}}}`
	dialects := []string{"postgres", "sqlite", "duckdb", "mysql"}
	for _, dialect := range dialects {
		res, err := CompileWithCatalog("from users | select [id, name]", dialect, catalog)
		if err != nil {
			t.Errorf("catalog compile failed for %s: %v", dialect, err)
			continue
		}
		if !strings.Contains(res.SQL, "SELECT id, name FROM users") {
			t.Errorf("dialect %s: unexpected sql: %s", dialect, res.SQL)
		}
	}
}

func TestCatalogFromSchema(t *testing.T) {
	schema := "table users [id integer primary auto, name string not null]"
	catalog, err := CatalogFromSchema(schema)
	if err != nil {
		t.Fatalf("CatalogFromSchema: %v", err)
	}
	want := `{"tables":{"users":{"name":"users","columns":[{"name":"id","ty":"Integer"},{"name":"name","ty":"String"}]}}}`
	if catalog != want {
		t.Errorf("unexpected catalog:\n got %s\nwant %s", catalog, want)
	}
}

func TestCatalogFromSchemaMemoized(t *testing.T) {
	// Same schema twice returns the identical string without re-deriving.
	schema := "table users [id integer, name string]"
	c1, err := CatalogFromSchema(schema)
	if err != nil {
		t.Fatalf("CatalogFromSchema: %v", err)
	}
	c2, err := CatalogFromSchema(schema)
	if err != nil {
		t.Fatalf("CatalogFromSchema: %v", err)
	}
	if c1 != c2 {
		t.Errorf("memoized catalog should be identical, got %s vs %s", c1, c2)
	}

	// Errors are not cached: the same bad schema fails every time.
	bad := "table t [id integer]\ntable t [id integer]"
	if _, err := CatalogFromSchema(bad); err == nil {
		t.Fatal("expected error for duplicate table")
	}
	if _, err := CatalogFromSchema(bad); err == nil {
		t.Fatal("expected error again — failures must not be cached")
	}
}

func TestCatalogFromSchemaTimestamp(t *testing.T) {
	// Timestamp columns are now natively supported across all layers.
	catalog, err := CatalogFromSchema("table events [id integer, at timestamp]")
	if err != nil {
		t.Fatalf("CatalogFromSchema: %v", err)
	}
	if !strings.Contains(catalog, `{"name":"at","ty":"Timestamp"}`) {
		t.Errorf("timestamp column should have Timestamp type: %s", catalog)
	}
	if _, err := CompileWithCatalog("from events | filter at == $t", "postgres", catalog); err != nil {
		t.Errorf("catalog with timestamp column rejected: %v", err)
	}
}

func TestCatalogFromSchemaMultiTable(t *testing.T) {
	schema := "table users [id integer primary auto]\n\ntable posts [id integer primary auto, user_id integer]"
	catalog, err := CatalogFromSchema(schema)
	if err != nil {
		t.Fatalf("CatalogFromSchema: %v", err)
	}
	if !strings.Contains(catalog, "\"users\"") || !strings.Contains(catalog, "\"posts\"") {
		t.Errorf("catalog should contain both tables: %s", catalog)
	}
}

func TestCatalogFromSchemaErrors(t *testing.T) {
	// No table statements -> error
	if _, err := CatalogFromSchema("from users | select [id]"); err == nil {
		t.Fatal("expected error for table-less schema")
	}
	// Duplicate table -> error
	if _, err := CatalogFromSchema("table users [id integer]\ntable users [id integer]"); err == nil {
		t.Fatal("expected error for duplicate table")
	}
	// Malformed table -> error
	if _, err := CatalogFromSchema("table users [id integer"); err == nil {
		t.Fatal("expected error for malformed table")
	}
}

func TestCompileWithSchema(t *testing.T) {
	schema := "table users [id integer primary auto, name string not null]"
	// Valid compile with schema-derived validation
	res, err := CompileWithSchema("from users | filter id == $id | select [name]", "postgres", schema)
	if err != nil {
		t.Fatalf("CompileWithSchema: %v", err)
	}
	if !strings.Contains(res.SQL, "SELECT name FROM users") {
		t.Errorf("unexpected sql: %s", res.SQL)
	}
	if len(res.Params) != 1 {
		t.Errorf("expected 1 param, got %v", res.Params)
	}
	// Unknown column rejected
	_, err = CompileWithSchema("from users | filter nme == $x", "postgres", schema)
	if err == nil {
		t.Fatal("expected unknown-column error")
	}
	perr, ok := err.(*Err)
	if !ok {
		t.Fatalf("expected *Err, got %T", err)
	}
	if perr.Kind != ErrAnalysis {
		t.Errorf("expected ErrAnalysis, got kind=%d msg=%s", perr.Kind, perr.Message)
	}
	if !strings.Contains(perr.Message, "nme") {
		t.Errorf("error should mention column: %s", perr.Message)
	}
	// CompileWithSchema across all dialects
	for _, dialect := range []string{"postgres", "sqlite", "duckdb", "mysql"} {
		if _, err := CompileWithSchema("from users | select [id]", dialect, schema); err != nil {
			t.Errorf("dialect %s: %v", dialect, err)
		}
	}
}
