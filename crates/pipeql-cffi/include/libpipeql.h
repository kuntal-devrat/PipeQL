/*
 * libpipeql.h — C API for PipeQL.
 *
 * Pipelined, injection-safe polyglot query language. Compiles PipeQL source
 * to target-dialect SQL with a fully isolated parameter map.
 *
 * Memory model:
 *   - Every `char*` returned by PipeQL is heap-allocated and owned by the
 *     caller. Release it with the matching `pipeql_*_free` function.
 *   - `PipeqlError` must be zero-initialized by the caller; any message it
 *     holds afterwards is owned and must be released with `pipeql_error_clear`.
 *
 * Thread safety: all functions are thread-safe. A result or error may only be
 * used on the thread that created it.
 *
 * Example:
 *   PipeqlError err = {0};
 *   PipeqlResult* res = pipeql_compile(
 *       "from users | filter age >= $min | select [id, name]", "postgres", &err);
 *   if (!res) { fprintf(stderr, "error: %s\n", err.message); pipeql_error_clear(&err); return 1; }
 *   printf("%s\n", res->sql);
 *   pipeql_result_free(res);
 */

#ifndef PIPEQL_H
#define PIPEQL_H

#ifdef __cplusplus
extern "C" {
#endif

#include <stddef.h>

/*
 * PipeQL builds its C library as a Rust cdylib, which exports all
 * `#[no_mangle] extern "C"` symbols directly. No `dllimport`/`dllexport`
 * decoration is therefore required: link against the produced import library
 * (MSVC: pipeql_cffi.dll.lib) or the DLL itself (MinGW: pipeql_cffi.dll).
 */
#define PIPEQL_API

/* Error kinds. */
#define PIPEQL_ERR_NONE 0
#define PIPEQL_ERR_PARSE 1
#define PIPEQL_ERR_ANALYSIS 2
#define PIPEQL_ERR_CODEGEN 3

typedef struct PipeqlResult {
    char* sql;           /* target-dialect SQL (owned) */
    char* params_json;   /* JSON array of parameter names, e.g. ["min_age"] (owned) */
    char* statement_type;/* "select"|"insert"|"update"|"delete"|"create_table"|"upsert"|"union" (owned) */
    int is_mutation;     /* non-zero for insert/update/delete/upsert */
    char* analysis_json; /* full analysis document: param map, types, occurrences (owned) */
    int parameter_count; /* number of extracted parameters */
} PipeqlResult;

typedef struct PipeqlError {
    int kind;           /* one of PIPEQL_ERR_* */
    char* message;      /* human-readable message (owned) */
} PipeqlError;

/* Compile a PipeQL source string into target-dialect SQL.
 *
 *   source  — NUL-terminated PipeQL source.
 *   dialect — NUL-terminated dialect name ("postgres" default, "sqlite",
 *             "duckdb", "mysql"). May be NULL to use the default.
 *   err     — caller-owned, zero-initialized error slot.
 *
 * Returns a heap-allocated PipeqlResult on success (free with
 * pipeql_result_free), or NULL on failure with *err populated.
 */
PIPEQL_API PipeqlResult* pipeql_compile(const char* source, const char* dialect,
                                        PipeqlError* err);

/* Compile a PipeQL source string, optionally validating columns against a
 * JSON schema catalog.
 *
 *   source       — NUL-terminated PipeQL source.
 *   dialect      — NUL-terminated dialect name. May be NULL for default.
 *   catalog_json — NUL-terminated JSON catalog string, or NULL for no
 *                  validation. Format:
 *                  {"tables":{"users":{"name":"users","columns":[{"name":"id",
 *                  "ty":"Integer"}]}}}
 *   err          — caller-owned, zero-initialized error slot.
 *
 * Returns a heap-allocated PipeqlResult on success (free with
 * pipeql_result_free), or NULL on failure with *err populated.
 */
PIPEQL_API PipeqlResult* pipeql_compile_with_catalog(
    const char* source, const char* dialect, const char* catalog_json,
    PipeqlError* err);

/* Derive a JSON schema catalog from one or more PipeQL table DDL statements.
 *
 * The returned string must be freed with pipeql_string_free.
 *
 *   schema — NUL-terminated PipeQL table DDL source.
 *   err    — caller-owned, zero-initialized error slot.
 */
PIPEQL_API char* pipeql_catalog_from_schema(const char* schema, PipeqlError* err);

/* Compile a PipeQL source string with analyzer validation derived from schema DDL.
 *
 *   source  — NUL-terminated PipeQL source.
 *   dialect — NUL-terminated dialect name. May be NULL for default.
 *   schema  — NUL-terminated PipeQL table DDL source.
 *   err     — caller-owned, zero-initialized error slot.
 */
PIPEQL_API PipeqlResult* pipeql_compile_with_schema(
    const char* source, const char* dialect, const char* schema,
    PipeqlError* err);

/* Parse a PipeQL source into a lossless statement AST, returned as a JSON
 * string. Covers read pipelines, inserts, upserts, unions, and DDL.
 *
 * The returned string must be freed with pipeql_string_free.
 *
 *   source — NUL-terminated PipeQL source.
 *   err    — caller-owned, zero-initialized error slot.
 *
 * Returns a heap-allocated JSON string on success (free with
 * pipeql_string_free), or NULL on failure with *err populated.
 */
PIPEQL_API char* pipeql_parse(const char* source, PipeqlError* err);

/* Return the list of supported dialect names as a JSON array string,
 * e.g. ["postgres","sqlite","duckdb","mysql"].
 *
 * The returned string must be freed with pipeql_string_free.
 */
PIPEQL_API char* pipeql_supported_dialects(void);

/* Return the PipeQL version as a static string. Never free. */
PIPEQL_API const char* pipeql_version(void);

/* ==========================================================================*
 * Fluent query builder
 *
 * Compose a PipeQL source string stage by stage, then compile it. Every
 * pipeql_query_* stage function returns the same handle so calls can be
 * chained:
 *
 *   PipeqlQuery* q = pipeql_query_from("notes");
 *   q = pipeql_query_filter(q, "is_archived == 0");
 *   q = pipeql_query_sort(q, "created_at desc");
 *   q = pipeql_query_take(q, 10);
 *   PipeqlResult* res = pipeql_query_compile(q, "postgres", &err);
 *   ...
 *   pipeql_result_free(res);
 *   pipeql_query_free(q);
 *
 * NULL handling: every string argument is treated as an explicit signal, not
 * as an empty string. If a required string argument is NULL, the function
 * returns NULL and the builder is left unmodified (the caller retains
 * ownership of the handle — it is not freed). A NULL query handle also
 * yields NULL from the stage functions. The NULL result is a failure
 * signal, not a replacement handle: keep your original pointer and free it
 * with pipeql_query_free when you are done — do not overwrite it with the
 * NULL result or the builder leaks.
 * ==========================================================================*/

/* Opaque fluent query builder handle. */
typedef struct PipeqlQuery PipeqlQuery;

/* Create a builder starting with `from <table>`. Free with pipeql_query_free.
 * Returns NULL if table is NULL. */
PIPEQL_API PipeqlQuery* pipeql_query_from(const char* table);

/* Create a builder starting with `into <table>` (insert/upsert target).
 * Returns NULL if table is NULL. */
PIPEQL_API PipeqlQuery* pipeql_query_into(const char* table);

/* Create a builder from an explicit PipeQL source string.
 * Returns NULL if source is NULL. */
PIPEQL_API PipeqlQuery* pipeql_query_raw(const char* source);

/* Append `| filter <expr>`. All stage functions return q for chaining, or NULL
 * if a required string argument is NULL (stage not appended, q not freed). */
PIPEQL_API PipeqlQuery* pipeql_query_filter(PipeqlQuery* q, const char* expr);

/* Append `| select [<cols>]`; cols is a comma-separated list. */
PIPEQL_API PipeqlQuery* pipeql_query_select(PipeqlQuery* q, const char* cols);

/* Append `| derive [<cols>]`; cols is a comma-separated list. */
PIPEQL_API PipeqlQuery* pipeql_query_derive(PipeqlQuery* q, const char* cols);

/* Append `| sort [<cols>]`; cols is a comma-separated list. */
PIPEQL_API PipeqlQuery* pipeql_query_sort(PipeqlQuery* q, const char* cols);

/* Append `| take <n>`. Returns NULL if q is NULL. */
PIPEQL_API PipeqlQuery* pipeql_query_take(PipeqlQuery* q, long long n);

/* Append `| skip <n>`. Returns NULL if q is NULL. */
PIPEQL_API PipeqlQuery* pipeql_query_skip(PipeqlQuery* q, long long n);

/* Append `| join <table> on <on>` (inner). */
PIPEQL_API PipeqlQuery* pipeql_query_join(PipeqlQuery* q, const char* table, const char* on);

/* Append `| left join <table> on <on>`. */
PIPEQL_API PipeqlQuery* pipeql_query_left_join(PipeqlQuery* q, const char* table, const char* on);

/* Append `| right join <table> on <on>`. */
PIPEQL_API PipeqlQuery* pipeql_query_right_join(PipeqlQuery* q, const char* table, const char* on);

/* Append `| full join <table> on <on>`. */
PIPEQL_API PipeqlQuery* pipeql_query_full_join(PipeqlQuery* q, const char* table, const char* on);

/* Append `| inner join <table> on <on>`. */
PIPEQL_API PipeqlQuery* pipeql_query_inner_join(PipeqlQuery* q, const char* table, const char* on);

/* Append `| group [<cols>] (<aggs>)`. */
PIPEQL_API PipeqlQuery* pipeql_query_group(PipeqlQuery* q, const char* cols, const char* aggs);

/* Append `| union <other>`; other is a source string or builder source. */
PIPEQL_API PipeqlQuery* pipeql_query_union(PipeqlQuery* q, const char* other);

/* Append `| union all <other>`. */
PIPEQL_API PipeqlQuery* pipeql_query_union_all(PipeqlQuery* q, const char* other);

/* Append `| delete`. Returns NULL if q is NULL. */
PIPEQL_API PipeqlQuery* pipeql_query_delete(PipeqlQuery* q);

/* Append `| delete all` — explicit opt-in for a full-table delete that
 * bypasses the filter guard. Returns NULL if q is NULL. */
PIPEQL_API PipeqlQuery* pipeql_query_delete_all(PipeqlQuery* q);

/* Append an explicit stage string (`filter x == 1`, `take 5`, ...). */
PIPEQL_API PipeqlQuery* pipeql_query_raw_stage(PipeqlQuery* q, const char* stage);

/* Append `| insert [<assignments>]`; assignments is a comma-separated list of
 * `col = $param` (or `col = value`) pairs. */
PIPEQL_API PipeqlQuery* pipeql_query_insert(PipeqlQuery* q, const char* assignments);

/* Append `| update [<assignments>]` (requires a preceding filter stage). */
PIPEQL_API PipeqlQuery* pipeql_query_update(PipeqlQuery* q, const char* assignments);

/* Append `| update all [<assignments>]` — explicit opt-in for a full-table
 * update that bypasses the filter guard. */
PIPEQL_API PipeqlQuery* pipeql_query_update_all(PipeqlQuery* q, const char* assignments);

/* Append `| upsert [<assignments>]`. */
PIPEQL_API PipeqlQuery* pipeql_query_upsert(PipeqlQuery* q, const char* assignments);

/* Append `| conflict [<cols>]`; cols is a comma-separated list. */
PIPEQL_API PipeqlQuery* pipeql_query_conflict(PipeqlQuery* q, const char* cols);

/* Append `| do update [<assignments>]`. */
PIPEQL_API PipeqlQuery* pipeql_query_do_update(PipeqlQuery* q, const char* assignments);

/* Return the composed PipeQL source as an owned string (free with
 * pipeql_string_free). Returns NULL if q is NULL. */
PIPEQL_API char* pipeql_query_source(const PipeqlQuery* q);

/* Compile the composed query for a target dialect. Returns a PipeqlResult
 * (free with pipeql_result_free) or NULL with *err populated. */
PIPEQL_API PipeqlResult* pipeql_query_compile(const PipeqlQuery* q, const char* dialect,
                                              PipeqlError* err);

/* Free a query builder. Passing NULL is a no-op. */
PIPEQL_API void pipeql_query_free(PipeqlQuery* q);

/* Free a result from pipeql_compile or pipeql_compile_with_catalog.
 * Passing NULL is a no-op.
 */
PIPEQL_API void pipeql_result_free(PipeqlResult* res);

/* Free a string returned by pipeql_parse or pipeql_supported_dialects.
 * Passing NULL is a no-op.
 */
PIPEQL_API void pipeql_string_free(char* s);

/* Free any message held by *err and reset it. */
PIPEQL_API void pipeql_error_clear(PipeqlError* err);

#ifdef __cplusplus
}
#endif

#endif /* PIPEQL_H */
