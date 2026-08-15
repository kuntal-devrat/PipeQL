// Package pipeql is a Go binding for PipeQL via the libpipeql C ABI (cgo).
//
// It compiles PipeQL source into target-dialect SQL with a fully isolated
// parameter map, giving Go applications the same injection-safe, polyglot
// query pipeline as the Rust core.
//
// Prerequisites:
//   - Build libpipeql once: `cargo build --release -p pipeql-cffi`
//   - Point CGO_LDFLAGS at the produced library, e.g. on Linux:
//     CGO_LDFLAGS="-L$PWD/target/release -lpipeql_cffi"
//
// Usage:
//
//	res, err := pipeql.Compile("from users | filter age >= $min | select [id]", "postgres")
//	if err != nil { log.Fatal(err) }
//	fmt.Println(res.SQL)
//	fmt.Println(res.Params) // ["min"]
package pipeql

/*
#cgo LDFLAGS: -lpipeql_cffi
#cgo CFLAGS: -I${SRCDIR}/../crates/pipeql-cffi/include
#include <stdlib.h>
#include "libpipeql.h"
*/
import "C"

import (
	"encoding/json"
	"unsafe"
)

// ErrKind classifies a compile failure, mirroring the C error kinds.
type ErrKind int

const (
	ErrNone     ErrKind = C.PIPEQL_ERR_NONE
	ErrParse    ErrKind = C.PIPEQL_ERR_PARSE
	ErrAnalysis ErrKind = C.PIPEQL_ERR_ANALYSIS
	ErrCodegen  ErrKind = C.PIPEQL_ERR_CODEGEN
)

// Err is an error returned by Compile.
type Err struct {
	Kind    ErrKind
	Message string
}

func (e *Err) Error() string { return e.Message }

// Result is the outcome of a successful compile.
type Result struct {
	// SQL is the target-dialect SQL text with positional placeholders.
	SQL string
	// Params is the ordered list of extracted parameter names.
	Params []string
	// StatementType is "select", "insert", "update", "delete", "create_table",
	// "upsert", or "union".
	StatementType string
	// IsMutation is true for insert/update/delete/upsert statements.
	IsMutation bool
	// Analysis is the full semantic analysis (param map, types, occurrences).
	Analysis json.RawMessage
	// ParameterCount is the number of extracted parameters.
	ParameterCount int
}

// Compile a PipeQL source string for a target dialect
// ("postgres" default, "sqlite", "duckdb", "mysql").
func Compile(source, dialect string) (*Result, error) {
	if dialect == "" {
		dialect = "postgres"
	}
	csrc := C.CString(source)
	defer C.free(unsafe.Pointer(csrc))
	cdial := C.CString(dialect)
	defer C.free(unsafe.Pointer(cdial))

	var cerr C.PipeqlError
	res := C.pipeql_compile(csrc, cdial, &cerr)
	if res == nil {
		defer C.pipeql_error_clear(&cerr)
		msg := "PipeQL compile failed"
		if cerr.message != nil {
			msg = C.GoString(cerr.message)
		}
		return nil, &Err{Kind: ErrKind(cerr.kind), Message: msg}
	}
	defer C.pipeql_result_free(res)

	return parseResult(res), nil
}

// CompileWithCatalog compiles a PipeQL source string, optionally validating
// columns against a JSON schema catalog.
//
// catalogJSON format: {"tables":{"users":{"name":"users","columns":[{"name":"id","ty":"Integer"}]}}}
// Pass empty string for no catalog validation.
func CompileWithCatalog(source, dialect, catalogJSON string) (*Result, error) {
	if dialect == "" {
		dialect = "postgres"
	}
	csrc := C.CString(source)
	defer C.free(unsafe.Pointer(csrc))
	cdial := C.CString(dialect)
	defer C.free(unsafe.Pointer(cdial))

	var cerr C.PipeqlError
	var res *C.PipeqlResult
	if catalogJSON == "" {
		res = C.pipeql_compile_with_catalog(csrc, cdial, nil, &cerr)
	} else {
		ccatalog := C.CString(catalogJSON)
		defer C.free(unsafe.Pointer(ccatalog))
		res = C.pipeql_compile_with_catalog(csrc, cdial, ccatalog, &cerr)
	}
	if res == nil {
		defer C.pipeql_error_clear(&cerr)
		msg := "PipeQL compile failed"
		if cerr.message != nil {
			msg = C.GoString(cerr.message)
		}
		return nil, &Err{Kind: ErrKind(cerr.kind), Message: msg}
	}
	defer C.pipeql_result_free(res)

	return parseResult(res), nil
}

// CatalogFromSchema derives an analyzer catalog (as the JSON string accepted
// by CompileWithCatalog) from one or more PipeQL `table` statements. The same
// string that defines your DDL becomes the schema the analyzer validates
// against — no hand-written catalog, nothing to keep in sync.
//
//	catalog, err := pipeql.CatalogFromSchema("table users [id integer primary auto, name string]")
//	res, err := pipeql.CompileWithCatalog("from users | filter nme == $x", "postgres", catalog)
func CatalogFromSchema(schema string) (string, error) {
	cschema := C.CString(schema)
	defer C.free(unsafe.Pointer(cschema))

	var cerr C.PipeqlError
	res := C.pipeql_catalog_from_schema(cschema, &cerr)
	if res == nil {
		defer C.pipeql_error_clear(&cerr)
		msg := "PipeQL catalog derivation failed"
		if cerr.message != nil {
			msg = C.GoString(cerr.message)
		}
		return "", &Err{Kind: ErrKind(cerr.kind), Message: msg}
	}
	defer C.pipeql_string_free(res)
	return C.GoString(res), nil
}

// CompileWithSchema compiles a PipeQL source string with analyzer validation,
// deriving the catalog from `schema` DDL in a single call — no separate
// catalog JSON to build or keep in sync.
//
//	res, err := pipeql.CompileWithSchema(
//	    "from users | filter nme == $x", "postgres",
//	    "table users [id integer primary auto, name string]",
//	)
//	// err: Unknown column 'nme'
func CompileWithSchema(source, dialect, schema string) (*Result, error) {
	if dialect == "" {
		dialect = "postgres"
	}
	csrc := C.CString(source)
	defer C.free(unsafe.Pointer(csrc))
	cdial := C.CString(dialect)
	defer C.free(unsafe.Pointer(cdial))
	csch := C.CString(schema)
	defer C.free(unsafe.Pointer(csch))

	var cerr C.PipeqlError
	res := C.pipeql_compile_with_schema(csrc, cdial, csch, &cerr)
	if res == nil {
		defer C.pipeql_error_clear(&cerr)
		msg := "PipeQL compile failed"
		if cerr.message != nil {
			msg = C.GoString(cerr.message)
		}
		return nil, &Err{Kind: ErrKind(cerr.kind), Message: msg}
	}
	defer C.pipeql_result_free(res)
	return parseResult(res), nil
}

// Parse a PipeQL source into a lossless statement AST, returned as raw JSON.
// Covers read pipelines, inserts, upserts, unions, and DDL.
func Parse(source string) (json.RawMessage, error) {
	csrc := C.CString(source)
	defer C.free(unsafe.Pointer(csrc))

	var cerr C.PipeqlError
	jsonPtr := C.pipeql_parse(csrc, &cerr)
	if jsonPtr == nil {
		defer C.pipeql_error_clear(&cerr)
		msg := "PipeQL parse failed"
		if cerr.message != nil {
			msg = C.GoString(cerr.message)
		}
		return nil, &Err{Kind: ErrKind(cerr.kind), Message: msg}
	}
	defer C.pipeql_string_free(jsonPtr)

	return json.RawMessage(C.GoString(jsonPtr)), nil
}

// SupportedDialects returns the list of supported dialect names.
func SupportedDialects() []string {
	jsonPtr := C.pipeql_supported_dialects()
	if jsonPtr == nil {
		return nil
	}
	defer C.pipeql_string_free(jsonPtr)

	var dialects []string
	if err := json.Unmarshal([]byte(C.GoString(jsonPtr)), &dialects); err != nil {
		return nil
	}
	return dialects
}

// MustCompile compiles or panics. Convenient for static/codegen contexts.
func MustCompile(source, dialect string) *Result {
	res, err := Compile(source, dialect)
	if err != nil {
		panic(err)
	}
	return res
}

// Version returns the PipeQL library version.
func Version() string {
	return C.GoString(C.pipeql_version())
}

// parseResult converts a C PipeqlResult into a Go Result.
func parseResult(res *C.PipeqlResult) *Result {
	sql := C.GoString(res.sql)
	paramsJSON := C.GoString(res.params_json)
	statementType := C.GoString(res.statement_type)
	analysisJSON := C.GoString(res.analysis_json)

	// Fast path: parse simple JSON string array ["a","b"] manually
	// to avoid the overhead of json.Unmarshal.
	var params []string
	if len(paramsJSON) > 2 && paramsJSON[0] == '[' && paramsJSON[len(paramsJSON)-1] == ']' {
		inner := paramsJSON[1 : len(paramsJSON)-1]
		if len(inner) == 0 {
			params = []string{}
		} else {
			// Count commas to pre-allocate
			n := 1
			for i := 0; i < len(inner); i++ {
				if inner[i] == ',' {
					n++
				}
			}
			params = make([]string, 0, n)
			start := -1
			for i := 0; i <= len(inner); i++ {
				if i < len(inner) && inner[i] == '"' {
					if start == -1 {
						start = i + 1
					} else {
						params = append(params, inner[start:i])
						start = -1
					}
				} else if i == len(inner) || inner[i] == ',' {
					if start != -1 {
						params = append(params, inner[start:i])
						start = -1
					}
				}
			}
		}
	} else {
		// Fallback to json.Unmarshal for complex cases
		_ = json.Unmarshal([]byte(paramsJSON), &params)
	}

	return &Result{
		SQL:            sql,
		Params:         params,
		StatementType:  statementType,
		IsMutation:     res.is_mutation != 0,
		Analysis:       json.RawMessage(analysisJSON),
		ParameterCount: int(res.parameter_count),
	}
}
