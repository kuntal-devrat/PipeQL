package pipeql

import (
	"reflect"
	"strings"
	"testing"
)

func TestDriverCompileAndBind(t *testing.T) {
	query := "from users | filter age >= $min and status == 'active' | select [id, name]"
	sqlStr, args, res, err := CompileAndBind(query, "postgres", map[string]any{
		"min": 18,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !strings.Contains(sqlStr, "SELECT id, name FROM users") {
		t.Errorf("unexpected SQL: %s", sqlStr)
	}
	if !strings.Contains(sqlStr, "age >= $1") {
		t.Errorf("expected $1 in SQL: %s", sqlStr)
	}
	if len(args) != 2 {
		t.Fatalf("expected 2 args, got %d", len(args))
	}
	if args[0] != 18 || args[1] != "active" {
		t.Errorf("unexpected args: %v", args)
	}
	if res.StatementType != "select" {
		t.Errorf("unexpected statement type: %s", res.StatementType)
	}
}

func TestDriverMissingParameterError(t *testing.T) {
	query := "from users | filter age >= $min | select [id]"
	_, _, _, err := CompileAndBind(query, "postgres", map[string]any{})
	if err == nil {
		t.Fatal("expected error for missing parameter $min, got nil")
	}
	if !strings.Contains(err.Error(), "missing value for parameter $min") {
		t.Errorf("unexpected error message: %v", err)
	}
}

func TestDriverBuilderSupport(t *testing.T) {
	q := From("notes").
		Filter("is_archived == 0").
		Sort([]string{"created_at desc"}).
		Take(10)

	sqlStr, args, _, err := CompileAndBind(q, "sqlite", nil)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !strings.Contains(sqlStr, "FROM notes") {
		t.Errorf("unexpected SQL: %s", sqlStr)
	}
	if len(args) != 0 {
		t.Errorf("expected 0 args, got %v", args)
	}
}

func TestDriverDataExpansionInsert(t *testing.T) {
	query := "into notes | insert $data"
	data := map[string]any{
		"title":   "My Note",
		"content": "Hello World",
	}

	sqlStr, args, res, err := CompileAndBind(query, "postgres", map[string]any{
		"data": data,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !strings.Contains(sqlStr, "INSERT INTO notes (content, title) VALUES ($1, $2)") {
		t.Errorf("unexpected SQL: %s", sqlStr)
	}
	if !res.IsMutation {
		t.Errorf("expected IsMutation=true")
	}
	expectedArgs := []any{"Hello World", "My Note"}
	if !reflect.DeepEqual(args, expectedArgs) {
		t.Errorf("expected args %v, got %v", expectedArgs, args)
	}
}

func TestDriverDataExpansionUpdate(t *testing.T) {
	query := "from notes | filter id == $id | update $data"
	data := map[string]any{
		"title": "Renamed Note",
	}

	sqlStr, args, res, err := CompileAndBind(query, "sqlite", map[string]any{
		"id":   42,
		"data": data,
	})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if !strings.Contains(sqlStr, "UPDATE notes") || !strings.Contains(sqlStr, "SET title = ?") || !strings.Contains(sqlStr, "WHERE (id = ?)") {
		t.Errorf("unexpected SQL: %s", sqlStr)
	}
	if res.StatementType != "update" {
		t.Errorf("expected statement type update, got %s", res.StatementType)
	}
	expectedArgs := []any{"Renamed Note", 42}
	if !reflect.DeepEqual(args, expectedArgs) {
		t.Errorf("expected args %v, got %v", expectedArgs, args)
	}
}
