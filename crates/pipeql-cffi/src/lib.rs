//! C-FFI bindings for PipeQL, exposed as `libpipeql`.
//!
//! These symbols form a stable C ABI consumed by C/C++, Go (via cgo), and any
//! other FFI-capable language. Memory ownership: every returned pointer that
//! must be freed is documented as "owned"; release it with the matching
//! `pipeql_*_free` function.

use std::ffi::{c_char, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;

use pipeql_core::{api, PipeQLError};

/// Error kinds, mirroring `PipeQLError` variants. Stable across versions.
pub const PIPEQL_ERR_NONE: i32 = 0;
pub const PIPEQL_ERR_PARSE: i32 = 1;
pub const PIPEQL_ERR_ANALYSIS: i32 = 2;
pub const PIPEQL_ERR_CODEGEN: i32 = 3;

/// Result of a successful compile. All fields are NUL-terminated owned
/// strings; free with `pipeql_result_free`.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct PipeqlResult {
    /// The target-dialect SQL.
    pub sql: *mut c_char,
    /// JSON array of parameter names, e.g. `["min_age","top"]`.
    pub params_json: *mut c_char,
    /// The statement kind: "select", "insert", "update", "delete",
    /// "create_table", "upsert", "union".
    pub statement_type: *mut c_char,
    /// Non-zero when the statement is a mutation (insert/update/delete/upsert).
    pub is_mutation: i32,
    /// JSON document of the full analysis (param map, types, occurrences).
    pub analysis_json: *mut c_char,
    /// Number of extracted parameters.
    pub parameter_count: i32,
}

/// Error payload. `message` is owned; free with `pipeql_error_clear`.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct PipeqlError {
    /// One of `PIPEQL_ERR_*`.
    pub kind: i32,
    /// NUL-terminated owned message.
    pub message: *mut c_char,
}

/// Compile a PipeQL source string into target-dialect SQL.
///
/// # Safety
///
/// `source` and `dialect` must be valid NUL-terminated C strings for the
/// duration of the call. `err` must point to a `PipeqlError` zero-initialized
/// by the caller.
///
/// Returns a heap-allocated `PipeqlResult` on success (free with
/// `pipeql_result_free`), or `NULL` on failure with `err` populated.
#[no_mangle]
pub unsafe extern "C" fn pipeql_compile(
    source: *const c_char,
    dialect: *const c_char,
    err: *mut PipeqlError,
) -> *mut PipeqlResult {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let src = read_cstr(source);
        let dialect = read_cstr(dialect);
        compile_inner(src.as_deref(), dialect.as_deref(), err)
    }));
    match result {
        Ok(ptr) => ptr,
        Err(_) => {
            write_error(err, PIPEQL_ERR_PARSE, "PipeQL panicked while compiling");
            ptr::null_mut()
        }
    }
}

/// Internal entry point taking `&str` directly — avoids CString round-trip
/// for callers that already own a Rust String (e.g. query builder).
fn compile_inner(
    src: Option<&str>,
    dialect: Option<&str>,
    err: *mut PipeqlError,
) -> *mut PipeqlResult {
    match src {
        Some(s) => match api::compile(s, dialect.unwrap_or("postgres")) {
            Ok(compiled) => {
                let params_json = serde_json::to_string(&compiled.params)
                    .unwrap_or_else(|_| "[]".to_string());
                let analysis_json = serde_json::to_string(&compiled.analysis)
                    .unwrap_or_else(|_| "{}".to_string());
                let parameter_count = compiled.params.len() as i32;
                Box::into_raw(Box::new(PipeqlResult {
                    sql: into_cstring(compiled.sql),
                    params_json: into_cstring(params_json),
                    statement_type: into_cstring(compiled.statement_type.as_str().to_string()),
                    is_mutation: compiled.is_mutation as i32,
                    analysis_json: into_cstring(analysis_json),
                    parameter_count,
                }))
            }
            Err(e) => {
                unsafe { write_error(err, classify(&e), &format!("{e}")) };
                ptr::null_mut()
            }
        },
        None => {
            unsafe { write_error(err, PIPEQL_ERR_PARSE, "source must be a non-null string") };
            ptr::null_mut()
        }
    }
}

/// Returns the PipeQL version as a static, NUL-terminated string. Never needs
/// freeing.
#[no_mangle]
pub extern "C" fn pipeql_version() -> *const c_char {
    let version = concat!(env!("CARGO_PKG_VERSION"), "\0");
    version.as_ptr() as *const c_char
}

/// Opaque fluent query builder handle (see `pipeql_query_*`).
#[repr(C)]
pub struct PipeqlQuery {
    source: String,
}

// SAFETY: PipeqlQuery is only accessed through the C-FFI functions which
// serialize access. The builder functions take `*mut PipeqlQuery` and return
// the same pointer, so callers must serialize access themselves.
unsafe impl Send for PipeqlQuery {}
unsafe impl Sync for PipeqlQuery {}

unsafe fn query_from_ptr<'a>(q: *mut PipeqlQuery) -> Option<&'a mut PipeqlQuery> {
    if q.is_null() {
        None
    } else {
        Some(&mut *q)
    }
}

unsafe fn stage(q: *mut PipeqlQuery, stage: &str) -> *mut PipeqlQuery {
    if let Some(query) = query_from_ptr(q) {
        query.source.push_str(" | ");
        query.source.push_str(stage);
    }
    q
}

/// Create a query builder starting with `from <table>`. Free with
/// `pipeql_query_free`.
///
/// Returns NULL (no handle allocated) if `table` is NULL — a NULL source
/// would silently compose a broken `from ` fragment, so it is rejected.
///
/// # Safety
///
/// `table` must be a valid NUL-terminated C string, or NULL.
#[no_mangle]
pub unsafe extern "C" fn pipeql_query_from(table: *const c_char) -> *mut PipeqlQuery {
    match read_cstr(table) {
        Some(t) => Box::into_raw(Box::new(PipeqlQuery {
            source: format!("from {t}"),
        })),
        None => ptr::null_mut(),
    }
}

/// Create a query builder starting with `into <table>` (insert/upsert target).
/// Free with `pipeql_query_free`.
///
/// Returns NULL (no handle allocated) if `table` is NULL — a NULL source
/// would silently compose a broken `into ` fragment, so it is rejected.
///
/// # Safety
///
/// `table` must be a valid NUL-terminated C string, or NULL.
#[no_mangle]
pub unsafe extern "C" fn pipeql_query_into(table: *const c_char) -> *mut PipeqlQuery {
    match read_cstr(table) {
        Some(t) => Box::into_raw(Box::new(PipeqlQuery {
            source: format!("into {t}"),
        })),
        None => ptr::null_mut(),
    }
}

/// Create a query builder from an explicit PipeQL source string.
///
/// Returns NULL (no handle allocated) if `source` is NULL — an empty source
/// is a caller bug, not a valid query.
///
/// # Safety
///
/// `source` must be a valid NUL-terminated C string, or NULL.
#[no_mangle]
pub unsafe extern "C" fn pipeql_query_raw(source: *const c_char) -> *mut PipeqlQuery {
    match read_cstr(source) {
        Some(s) => Box::into_raw(Box::new(PipeqlQuery { source: s })),
        None => ptr::null_mut(),
    }
}

/// Append `| filter <expr>`. Returns `q` for chaining, or NULL if `expr` is
/// NULL (the stage is not appended — an empty `filter ` fragment would be
/// invalid PipeQL).
///
/// # Safety
///
/// `q` must come from a `pipeql_query_*` builder (or be NULL). `expr` must be
/// a valid NUL-terminated C string, or NULL.
#[no_mangle]
pub unsafe extern "C" fn pipeql_query_filter(
    q: *mut PipeqlQuery,
    expr: *const c_char,
) -> *mut PipeqlQuery {
    match read_cstr(expr) {
        Some(e) => stage(q, &format!("filter {e}")),
        None => ptr::null_mut(),
    }
}

/// Append `| select [<cols>]` where `cols` is a comma-separated list.
/// Returns NULL if `cols` is NULL (stage not appended).
///
/// # Safety
///
/// `q` must come from a `pipeql_query_*` builder (or be NULL). `cols` must be
/// a valid NUL-terminated C string, or NULL.
#[no_mangle]
pub unsafe extern "C" fn pipeql_query_select(
    q: *mut PipeqlQuery,
    cols: *const c_char,
) -> *mut PipeqlQuery {
    match read_cstr(cols) {
        Some(c) => stage(q, &format!("select [{c}]")),
        None => ptr::null_mut(),
    }
}

/// Append `| derive [<cols>]` where `cols` is a comma-separated list.
/// Returns NULL if `cols` is NULL (stage not appended).
///
/// # Safety
///
/// `q` must come from a `pipeql_query_*` builder (or be NULL). `cols` must be
/// a valid NUL-terminated C string, or NULL.
#[no_mangle]
pub unsafe extern "C" fn pipeql_query_derive(
    q: *mut PipeqlQuery,
    cols: *const c_char,
) -> *mut PipeqlQuery {
    match read_cstr(cols) {
        Some(c) => stage(q, &format!("derive [{c}]")),
        None => ptr::null_mut(),
    }
}

/// Append `| sort [<cols>]` where `cols` is a comma-separated list.
/// Returns NULL if `cols` is NULL (stage not appended).
///
/// # Safety
///
/// `q` must come from a `pipeql_query_*` builder (or be NULL). `cols` must be
/// a valid NUL-terminated C string, or NULL.
#[no_mangle]
pub unsafe extern "C" fn pipeql_query_sort(
    q: *mut PipeqlQuery,
    cols: *const c_char,
) -> *mut PipeqlQuery {
    match read_cstr(cols) {
        Some(c) => stage(q, &format!("sort [{c}]")),
        None => ptr::null_mut(),
    }
}

/// Append `| take <n>`.
///
/// # Safety
///
/// `q` must come from a `pipeql_query_*` builder (or be NULL).
#[no_mangle]
pub unsafe extern "C" fn pipeql_query_take(q: *mut PipeqlQuery, n: i64) -> *mut PipeqlQuery {
    stage(q, &format!("take {n}"))
}

/// Append `| skip <n>`.
///
/// # Safety
///
/// `q` must come from a `pipeql_query_*` builder (or be NULL).
#[no_mangle]
pub unsafe extern "C" fn pipeql_query_skip(q: *mut PipeqlQuery, n: i64) -> *mut PipeqlQuery {
    stage(q, &format!("skip {n}"))
}

/// Append `| join <table> on <on>`. Returns NULL if `table` or `on` is NULL
/// (stage not appended).
///
/// # Safety
///
/// `q` must come from a `pipeql_query_*` builder (or be NULL). `table` and
/// `on` must be valid NUL-terminated C strings, or NULL.
#[no_mangle]
pub unsafe extern "C" fn pipeql_query_join(
    q: *mut PipeqlQuery,
    table: *const c_char,
    on: *const c_char,
) -> *mut PipeqlQuery {
    match (read_cstr(table), read_cstr(on)) {
        (Some(t), Some(o)) => stage(q, &format!("join {t} on {o}")),
        _ => ptr::null_mut(),
    }
}

/// Append `| left join <table> on <on>`. Returns NULL if `table` or `on` is
/// NULL (stage not appended).
///
/// # Safety
///
/// `q` must come from a `pipeql_query_*` builder (or be NULL). `table` and
/// `on` must be valid NUL-terminated C strings, or NULL.
#[no_mangle]
pub unsafe extern "C" fn pipeql_query_left_join(
    q: *mut PipeqlQuery,
    table: *const c_char,
    on: *const c_char,
) -> *mut PipeqlQuery {
    match (read_cstr(table), read_cstr(on)) {
        (Some(t), Some(o)) => stage(q, &format!("left join {t} on {o}")),
        _ => ptr::null_mut(),
    }
}

/// Append `| right join <table> on <on>`. Returns NULL if `table` or `on` is
/// NULL (stage not appended).
///
/// # Safety
///
/// `q` must come from a `pipeql_query_*` builder (or be NULL). `table` and
/// `on` must be valid NUL-terminated C strings, or NULL.
#[no_mangle]
pub unsafe extern "C" fn pipeql_query_right_join(
    q: *mut PipeqlQuery,
    table: *const c_char,
    on: *const c_char,
) -> *mut PipeqlQuery {
    match (read_cstr(table), read_cstr(on)) {
        (Some(t), Some(o)) => stage(q, &format!("right join {t} on {o}")),
        _ => ptr::null_mut(),
    }
}

/// Append `| full join <table> on <on>`. Returns NULL if `table` or `on` is
/// NULL (stage not appended).
///
/// # Safety
///
/// `q` must come from a `pipeql_query_*` builder (or be NULL). `table` and
/// `on` must be valid NUL-terminated C strings, or NULL.
#[no_mangle]
pub unsafe extern "C" fn pipeql_query_full_join(
    q: *mut PipeqlQuery,
    table: *const c_char,
    on: *const c_char,
) -> *mut PipeqlQuery {
    match (read_cstr(table), read_cstr(on)) {
        (Some(t), Some(o)) => stage(q, &format!("full join {t} on {o}")),
        _ => ptr::null_mut(),
    }
}

/// Append `| inner join <table> on <on>`. Returns NULL if `table` or `on` is
/// NULL (stage not appended).
///
/// # Safety
///
/// `q` must come from a `pipeql_query_*` builder (or be NULL). `table` and
/// `on` must be valid NUL-terminated C strings, or NULL.
#[no_mangle]
pub unsafe extern "C" fn pipeql_query_inner_join(
    q: *mut PipeqlQuery,
    table: *const c_char,
    on: *const c_char,
) -> *mut PipeqlQuery {
    match (read_cstr(table), read_cstr(on)) {
        (Some(t), Some(o)) => stage(q, &format!("inner join {t} on {o}")),
        _ => ptr::null_mut(),
    }
}

/// Append `| group [<cols>] (<aggs>)`. Returns NULL if `cols` or `aggs` is
/// NULL (stage not appended).
///
/// # Safety
///
/// `q` must come from a `pipeql_query_*` builder (or be NULL). `cols` and
/// `aggs` must be valid NUL-terminated C strings, or NULL.
#[no_mangle]
pub unsafe extern "C" fn pipeql_query_group(
    q: *mut PipeqlQuery,
    cols: *const c_char,
    aggs: *const c_char,
) -> *mut PipeqlQuery {
    match (read_cstr(cols), read_cstr(aggs)) {
        (Some(c), Some(a)) => stage(q, &format!("group [{c}] ({a})")),
        _ => ptr::null_mut(),
    }
}

/// Append `| union <other>` where `other` is a source string or a query
/// built via `pipeql_query_source`. Returns NULL if `other` is NULL.
///
/// # Safety
///
/// `q` must come from a `pipeql_query_*` builder (or be NULL). `other` must
/// be a valid NUL-terminated C string, or NULL.
#[no_mangle]
pub unsafe extern "C" fn pipeql_query_union(
    q: *mut PipeqlQuery,
    other: *const c_char,
) -> *mut PipeqlQuery {
    match read_cstr(other) {
        Some(o) => stage(q, &format!("union {o}")),
        None => ptr::null_mut(),
    }
}

/// Append `| union all <other>`. Returns NULL if `other` is NULL.
///
/// # Safety
///
/// `q` must come from a `pipeql_query_*` builder (or be NULL). `other` must
/// be a valid NUL-terminated C string, or NULL.
#[no_mangle]
pub unsafe extern "C" fn pipeql_query_union_all(
    q: *mut PipeqlQuery,
    other: *const c_char,
) -> *mut PipeqlQuery {
    match read_cstr(other) {
        Some(o) => stage(q, &format!("union all {o}")),
        None => ptr::null_mut(),
    }
}

/// Append `| delete`.
///
/// # Safety
///
/// `q` must come from a `pipeql_query_*` builder (or be NULL).
#[no_mangle]
pub unsafe extern "C" fn pipeql_query_delete(q: *mut PipeqlQuery) -> *mut PipeqlQuery {
    stage(q, "delete")
}

/// Append `| delete all` — explicit opt-in for a full-table delete that
/// bypasses the filter guard.
///
/// # Safety
///
/// `q` must come from a `pipeql_query_*` builder (or be NULL).
#[no_mangle]
pub unsafe extern "C" fn pipeql_query_delete_all(q: *mut PipeqlQuery) -> *mut PipeqlQuery {
    stage(q, "delete all")
}

/// Append an explicit stage string (`filter x == 1`, `take 5`, ...).
/// Returns NULL if `raw` is NULL (stage not appended).
///
/// # Safety
///
/// `q` must come from a `pipeql_query_*` builder (or be NULL). `raw` must be
/// a valid NUL-terminated C string, or NULL.
#[no_mangle]
pub unsafe extern "C" fn pipeql_query_raw_stage(
    q: *mut PipeqlQuery,
    raw: *const c_char,
) -> *mut PipeqlQuery {
    match read_cstr(raw) {
        Some(s) => stage(q, &s),
        None => ptr::null_mut(),
    }
}

/// Append `| insert [<assignments>]` where `assignments` is a comma-separated
/// list of `col = $param` (or `col = value`) pairs. Returns NULL if
/// `assignments` is NULL (stage not appended).
///
/// # Safety
///
/// `q` must come from a `pipeql_query_*` builder (or be NULL). `assignments`
/// must be a valid NUL-terminated C string, or NULL.
#[no_mangle]
pub unsafe extern "C" fn pipeql_query_insert(
    q: *mut PipeqlQuery,
    assignments: *const c_char,
) -> *mut PipeqlQuery {
    match read_cstr(assignments) {
        Some(a) => stage(q, &format!("insert [{a}]")),
        None => ptr::null_mut(),
    }
}

/// Append `| update [<assignments>]` (requires a preceding filter stage).
/// Returns NULL if `assignments` is NULL (stage not appended).
///
/// # Safety
///
/// `q` must come from a `pipeql_query_*` builder (or be NULL). `assignments`
/// must be a valid NUL-terminated C string, or NULL.
#[no_mangle]
pub unsafe extern "C" fn pipeql_query_update(
    q: *mut PipeqlQuery,
    assignments: *const c_char,
) -> *mut PipeqlQuery {
    match read_cstr(assignments) {
        Some(a) => stage(q, &format!("update [{a}]")),
        None => ptr::null_mut(),
    }
}

/// Append `| update all [<assignments>]` — explicit opt-in for a full-table
/// update that bypasses the filter guard. Returns NULL if `assignments` is
/// NULL (stage not appended).
///
/// # Safety
///
/// `q` must come from a `pipeql_query_*` builder (or be NULL). `assignments`
/// must be a valid NUL-terminated C string, or NULL.
#[no_mangle]
pub unsafe extern "C" fn pipeql_query_update_all(
    q: *mut PipeqlQuery,
    assignments: *const c_char,
) -> *mut PipeqlQuery {
    match read_cstr(assignments) {
        Some(a) => stage(q, &format!("update all [{a}]")),
        None => ptr::null_mut(),
    }
}

/// Append `| upsert [<assignments>]`. Returns NULL if `assignments` is NULL
/// (stage not appended).
///
/// # Safety
///
/// `q` must come from a `pipeql_query_*` builder (or be NULL). `assignments`
/// must be a valid NUL-terminated C string, or NULL.
#[no_mangle]
pub unsafe extern "C" fn pipeql_query_upsert(
    q: *mut PipeqlQuery,
    assignments: *const c_char,
) -> *mut PipeqlQuery {
    match read_cstr(assignments) {
        Some(a) => stage(q, &format!("upsert [{a}]")),
        None => ptr::null_mut(),
    }
}

/// Append `| conflict [<cols>]` where `cols` is a comma-separated list.
/// Returns NULL if `cols` is NULL (stage not appended).
///
/// # Safety
///
/// `q` must come from a `pipeql_query_*` builder (or be NULL). `cols` must be
/// a valid NUL-terminated C string, or NULL.
#[no_mangle]
pub unsafe extern "C" fn pipeql_query_conflict(
    q: *mut PipeqlQuery,
    cols: *const c_char,
) -> *mut PipeqlQuery {
    match read_cstr(cols) {
        Some(c) => stage(q, &format!("conflict [{c}]")),
        None => ptr::null_mut(),
    }
}

/// Append `| do update [<assignments>]`. Returns NULL if `assignments` is
/// NULL (stage not appended).
///
/// # Safety
///
/// `q` must come from a `pipeql_query_*` builder (or be NULL). `assignments`
/// must be a valid NUL-terminated C string, or NULL.
#[no_mangle]
pub unsafe extern "C" fn pipeql_query_do_update(
    q: *mut PipeqlQuery,
    assignments: *const c_char,
) -> *mut PipeqlQuery {
    match read_cstr(assignments) {
        Some(a) => stage(q, &format!("do update [{a}]")),
        None => ptr::null_mut(),
    }
}

/// Return the composed PipeQL source as an owned string (free with
/// `pipeql_string_free`). Returns NULL if `q` is NULL.
///
/// # Safety
///
/// `q` must come from a `pipeql_query_*` builder, or be NULL.
#[no_mangle]
pub unsafe extern "C" fn pipeql_query_source(q: *const PipeqlQuery) -> *mut c_char {
    if q.is_null() {
        return ptr::null_mut();
    }
    let query = &*q;
    into_cstring(query.source.clone())
}

/// Compile the composed query for a target dialect. Returns a
/// `PipeqlResult` (free with `pipeql_result_free`) or NULL with `err` set.
///
/// # Safety
///
/// `dialect` must be a valid NUL-terminated C string; `err` must be
/// zero-initialized. `q` must come from a `pipeql_query_*` builder.
#[no_mangle]
pub unsafe extern "C" fn pipeql_query_compile(
    q: *const PipeqlQuery,
    dialect: *const c_char,
    err: *mut PipeqlError,
) -> *mut PipeqlResult {
    if q.is_null() {
        write_error(err, PIPEQL_ERR_PARSE, "query builder is NULL");
        return ptr::null_mut();
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        let source = (*q).source.clone();
        let d = read_cstr(dialect);
        compile_inner(Some(&source), d.as_deref(), err)
    }));
    match result {
        Ok(ptr) => ptr,
        Err(_) => {
            write_error(err, PIPEQL_ERR_PARSE, "PipeQL panicked while compiling");
            ptr::null_mut()
        }
    }
}

/// Free a query builder. Passing NULL is a no-op.
///
/// # Safety
///
/// `q` must come from a `pipeql_query_*` builder, or be NULL. After this call,
/// `q` must not be used again.
#[no_mangle]
pub unsafe extern "C" fn pipeql_query_free(q: *mut PipeqlQuery) {
    if q.is_null() {
        return;
    }
    drop(Box::from_raw(q));
}

/// Compile a PipeQL source string, optionally validating columns against a
/// JSON schema catalog.
///
/// # Safety
///
/// `source` and `dialect` must be valid NUL-terminated C strings.
/// `catalog_json` may be NULL (no validation) or a NUL-terminated JSON string
/// in the format: `{"tables":{"users":{"name":"users","columns":[{"name":"id",
/// "ty":"Integer"}]}}}`.
/// `err` must point to a `PipeqlError` zero-initialized by the caller.
///
/// Returns a heap-allocated `PipeqlResult` on success (free with
/// `pipeql_result_free`), or `NULL` on failure with `err` populated.
#[no_mangle]
pub unsafe extern "C" fn pipeql_compile_with_catalog(
    source: *const c_char,
    dialect: *const c_char,
    catalog_json: *const c_char,
    err: *mut PipeqlError,
) -> *mut PipeqlResult {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let src = read_cstr(source);
        let dialect = read_cstr(dialect);
        let catalog_str = read_cstr(catalog_json);

        match src {
            Some(s) => {
                let catalog = match catalog_str.as_deref() {
                    Some(js) => match serde_json::from_str::<pipeql_core::Catalog>(js) {
                        Ok(c) => Some(c),
                        Err(e) => {
                            write_error(
                                err,
                                PIPEQL_ERR_PARSE,
                                &format!("invalid catalog JSON: {e}"),
                            );
                            return ptr::null_mut();
                        }
                    },
                    None => None,
                };
                let catalog_ref = catalog.as_ref();
                match api::compile_with_catalog(
                    &s,
                    dialect.as_deref().unwrap_or("postgres"),
                    catalog_ref,
                ) {
                    Ok(compiled) => {
                        let params_json = serde_json::to_string(&compiled.params)
                            .unwrap_or_else(|_| "[]".to_string());
                        let analysis_json = serde_json::to_string(&compiled.analysis)
                            .unwrap_or_else(|_| "{}".to_string());
                        let parameter_count = compiled.params.len() as i32;
                        Box::into_raw(Box::new(PipeqlResult {
                            sql: into_cstring(compiled.sql),
                            params_json: into_cstring(params_json),
                            statement_type: into_cstring(
                                compiled.statement_type.as_str().to_string(),
                            ),
                            is_mutation: compiled.is_mutation as i32,
                            analysis_json: into_cstring(analysis_json),
                            parameter_count,
                        }))
                    }
                    Err(e) => {
                        write_error(err, classify(&e), &format!("{e}"));
                        ptr::null_mut()
                    }
                }
            }
            None => {
                write_error(err, PIPEQL_ERR_PARSE, "source must be a non-null string");
                ptr::null_mut()
            }
        }
    }));
    match result {
        Ok(ptr) => ptr,
        Err(_) => {
            write_error(err, PIPEQL_ERR_PARSE, "PipeQL panicked while compiling");
            ptr::null_mut()
        }
    }
}

/// Derive a JSON schema catalog from one or more PipeQL `table` statements.
///
/// Returns an owned JSON string (free with `pipeql_string_free`) on success,
/// or NULL on failure with `err` populated.
#[no_mangle]
/// # Safety
/// `schema` must be a null-terminated UTF-8 string.
/// `err` must be a valid pointer to a `PipeqlError` struct.
pub unsafe extern "C" fn pipeql_catalog_from_schema(
    schema: *const c_char,
    err: *mut PipeqlError,
) -> *mut c_char {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let schema_str = read_cstr(schema);
        match schema_str {
            Some(s) => match api::catalog_from_schema(&s) {
                Ok(catalog) => match serde_json::to_string(&catalog) {
                    Ok(json) => into_cstring(json),
                    Err(e) => {
                        write_error(
                            err,
                            PIPEQL_ERR_CODEGEN,
                            &format!("Catalog serialization failed: {e}"),
                        );
                        ptr::null_mut()
                    }
                },
                Err(e) => {
                    write_error(err, classify(&e), &format!("{e}"));
                    ptr::null_mut()
                }
            },
            None => {
                write_error(err, PIPEQL_ERR_PARSE, "schema must be a non-null string");
                ptr::null_mut()
            }
        }
    }));
    match result {
        Ok(ptr) => ptr,
        Err(_) => {
            write_error(err, PIPEQL_ERR_PARSE, "PipeQL panicked while building catalog");
            ptr::null_mut()
        }
    }
}

/// Compile a PipeQL source string with analyzer validation derived from a schema DDL string.
#[no_mangle]
/// # Safety
/// `source`, `dialect`, and `schema` must be null-terminated UTF-8 strings.
/// `dialect` may be NULL, in which case it defaults to "postgres".
/// `err` must be a valid pointer to a `PipeqlError` struct.
pub unsafe extern "C" fn pipeql_compile_with_schema(
    source: *const c_char,
    dialect: *const c_char,
    schema: *const c_char,
    err: *mut PipeqlError,
) -> *mut PipeqlResult {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let src = read_cstr(source);
        let dialect_str = read_cstr(dialect);
        let schema_str = read_cstr(schema);

        match (src, schema_str) {
            (Some(s), Some(sch)) => {
                match api::compile_with_schema(
                    &s,
                    dialect_str.as_deref().unwrap_or("postgres"),
                    &sch,
                ) {
                    Ok(compiled) => {
                        let params_json = serde_json::to_string(&compiled.params)
                            .unwrap_or_else(|_| "[]".to_string());
                        let analysis_json = serde_json::to_string(&compiled.analysis)
                            .unwrap_or_else(|_| "{}".to_string());
                        let parameter_count = compiled.params.len() as i32;
                        Box::into_raw(Box::new(PipeqlResult {
                            sql: into_cstring(compiled.sql),
                            params_json: into_cstring(params_json),
                            statement_type: into_cstring(
                                compiled.statement_type.as_str().to_string(),
                            ),
                            is_mutation: compiled.is_mutation as i32,
                            analysis_json: into_cstring(analysis_json),
                            parameter_count,
                        }))
                    }
                    Err(e) => {
                        write_error(err, classify(&e), &format!("{e}"));
                        ptr::null_mut()
                    }
                }
            }
            (None, _) => {
                write_error(err, PIPEQL_ERR_PARSE, "source must be a non-null string");
                ptr::null_mut()
            }
            (_, None) => {
                write_error(err, PIPEQL_ERR_PARSE, "schema must be a non-null string");
                ptr::null_mut()
            }
        }
    }));
    match result {
        Ok(ptr) => ptr,
        Err(_) => {
            write_error(err, PIPEQL_ERR_PARSE, "PipeQL panicked while compiling");
            ptr::null_mut()
        }
    }
}

/// Parse a PipeQL source into a lossless statement AST, returned as a JSON
/// string. Covers read pipelines, inserts, upserts, unions, and DDL.
///
/// The returned string must be freed with `pipeql_string_free`.
///
/// # Safety
///
/// `source` must be a valid NUL-terminated C string. `err` must point to a
/// `PipeqlError` zero-initialized by the caller.
///
/// Returns a heap-allocated JSON string on success (free with
/// `pipeql_string_free`), or `NULL` on failure with `err` populated.
#[no_mangle]
pub unsafe extern "C" fn pipeql_parse(source: *const c_char, err: *mut PipeqlError) -> *mut c_char {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let src = read_cstr(source);
        match src {
            Some(s) => match api::parse_statement(&s) {
                Ok(stmt) => match serde_json::to_string(&stmt) {
                    Ok(json) => into_cstring(json),
                    Err(e) => {
                        write_error(
                            err,
                            PIPEQL_ERR_CODEGEN,
                            &format!("AST serialization failed: {e}"),
                        );
                        ptr::null_mut()
                    }
                },
                Err(e) => {
                    write_error(err, classify(&e), &format!("{e}"));
                    ptr::null_mut()
                }
            },
            None => {
                write_error(err, PIPEQL_ERR_PARSE, "source must be a non-null string");
                ptr::null_mut()
            }
        }
    }));
    match result {
        Ok(ptr) => ptr,
        Err(_) => {
            write_error(err, PIPEQL_ERR_PARSE, "PipeQL panicked while parsing");
            ptr::null_mut()
        }
    }
}

/// Return the list of supported dialect names as a JSON array string,
/// e.g. `["postgres","sqlite","duckdb","mysql"]`.
///
/// The returned string must be freed with `pipeql_string_free`.
#[no_mangle]
pub extern "C" fn pipeql_supported_dialects() -> *mut c_char {
    let dialects = api::supported_dialects();
    let json = serde_json::to_string(&dialects).unwrap_or_else(|_| "[]".to_string());
    into_cstring(json)
}

/// Free a string previously returned by `pipeql_parse` or
/// `pipeql_supported_dialects`.
///
/// # Safety
///
/// `s` must be a pointer returned by `pipeql_parse` or
/// `pipeql_supported_dialects`, or NULL.
#[no_mangle]
pub unsafe extern "C" fn pipeql_string_free(s: *mut c_char) {
    free_cstring(s);
}

/// Free a result previously returned by `pipeql_compile`.
///
/// # Safety
///
/// `res` must be a pointer from `pipeql_compile` or NULL.
#[no_mangle]
pub unsafe extern "C" fn pipeql_result_free(res: *mut PipeqlResult) {
    if res.is_null() {
        return;
    }
    let boxed = Box::from_raw(res);
    free_cstring(boxed.sql);
    free_cstring(boxed.params_json);
    free_cstring(boxed.statement_type);
    free_cstring(boxed.analysis_json);
}

/// Free a message previously written into a `PipeqlError`.
///
/// # Safety
///
/// `err` must point to a `PipeqlError` populated by PipeQL.
#[no_mangle]
pub unsafe extern "C" fn pipeql_error_clear(err: *mut PipeqlError) {
    if err.is_null() {
        return;
    }
    let e = &mut *err;
    if !e.message.is_null() {
        drop(CString::from_raw(e.message));
        e.message = ptr::null_mut();
    }
    e.kind = PIPEQL_ERR_NONE;
}

fn classify(e: &PipeQLError) -> i32 {
    match e {
        PipeQLError::Parse(_) => PIPEQL_ERR_PARSE,
        PipeQLError::Analysis(_) => PIPEQL_ERR_ANALYSIS,
        PipeQLError::Codegen(_) => PIPEQL_ERR_CODEGEN,
    }
}

unsafe fn read_cstr(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    CStr::from_ptr(ptr).to_str().ok().map(|s| s.to_string())
}

fn into_cstring(s: String) -> *mut c_char {
    CString::new(s).map(|c| c.into_raw()).unwrap_or_else(|_| {
        CString::new("<unprintable>")
            .expect("static string is NUL-free")
            .into_raw()
    })
}

unsafe fn free_cstring(ptr: *mut c_char) {
    if !ptr.is_null() {
        drop(CString::from_raw(ptr));
    }
}

unsafe fn write_error(err: *mut PipeqlError, kind: i32, message: &str) {
    if err.is_null() {
        return;
    }
    let e = &mut *err;
    if !e.message.is_null() {
        drop(CString::from_raw(e.message));
    }
    e.kind = kind;
    e.message = into_cstring(message.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    unsafe fn cstr(s: &str) -> *const c_char {
        CString::new(s).unwrap().into_raw() as *const c_char
    }

    unsafe fn free_cstr(ptr: *const c_char) {
        if !ptr.is_null() {
            drop(CString::from_raw(ptr as *mut c_char));
        }
    }

    #[test]
    fn test_compile_basic() {
        unsafe {
            let src = cstr("from users | filter age >= $min | select [id, name]");
            let dialect = cstr("postgres");
            let mut err = PipeqlError {
                kind: PIPEQL_ERR_NONE,
                message: ptr::null_mut(),
            };
            let res = pipeql_compile(src, dialect, &mut err);
            assert!(!res.is_null(), "compile should succeed");
            let r = &*res;
            assert!(CStr::from_ptr(r.sql)
                .to_str()
                .unwrap()
                .contains("SELECT id, name FROM users"));
            assert_eq!(CStr::from_ptr(r.statement_type).to_str().unwrap(), "select");
            assert_eq!(r.is_mutation, 0);
            assert_eq!(r.parameter_count, 1);
            pipeql_result_free(res);
            free_cstr(src);
            free_cstr(dialect);
        }
    }

    #[test]
    fn test_compile_upsert_returns_upsert_type() {
        unsafe {
            let src =
                cstr("into users | upsert [name = $n] | conflict [email] | do update [name = $n]");
            let dialect = cstr("postgres");
            let mut err = PipeqlError {
                kind: PIPEQL_ERR_NONE,
                message: ptr::null_mut(),
            };
            let res = pipeql_compile(src, dialect, &mut err);
            assert!(!res.is_null(), "compile upsert should succeed");
            let r = &*res;
            assert_eq!(CStr::from_ptr(r.statement_type).to_str().unwrap(), "upsert");
            assert_eq!(r.is_mutation, 1);
            pipeql_result_free(res);
            free_cstr(src);
            free_cstr(dialect);
        }
    }

    #[test]
    fn test_compile_union_returns_union_type() {
        unsafe {
            let src = cstr("from a | select [id] | union all from b | select [id]");
            let dialect = cstr("postgres");
            let mut err = PipeqlError {
                kind: PIPEQL_ERR_NONE,
                message: ptr::null_mut(),
            };
            let res = pipeql_compile(src, dialect, &mut err);
            assert!(!res.is_null(), "compile union should succeed");
            let r = &*res;
            assert_eq!(CStr::from_ptr(r.statement_type).to_str().unwrap(), "union");
            assert_eq!(r.is_mutation, 0);
            pipeql_result_free(res);
            free_cstr(src);
            free_cstr(dialect);
        }
    }

    #[test]
    fn test_compile_with_catalog_valid() {
        unsafe {
            let src = cstr("from users | select [id, name]");
            let dialect = cstr("postgres");
            let catalog = cstr(
                r#"{"tables":{"users":{"name":"users","columns":[{"name":"id","ty":"Integer"},{"name":"name","ty":"String"}]}}}"#,
            );
            let mut err = PipeqlError {
                kind: PIPEQL_ERR_NONE,
                message: ptr::null_mut(),
            };
            let res = pipeql_compile_with_catalog(src, dialect, catalog, &mut err);
            if res.is_null() {
                let msg = if err.message.is_null() {
                    "<null>".to_string()
                } else {
                    CStr::from_ptr(err.message).to_str().unwrap().to_string()
                };
                panic!(
                    "compile with valid catalog failed: kind={}, msg={}",
                    err.kind, msg
                );
            }
            pipeql_result_free(res);
            free_cstr(src);
            free_cstr(dialect);
            free_cstr(catalog);
        }
    }

    #[test]
    fn test_compile_with_catalog_invalid_column() {
        unsafe {
            let src = cstr("from users | select [nope]");
            let dialect = cstr("postgres");
            let catalog = cstr(
                r#"{"tables":{"users":{"name":"users","columns":[{"name":"id","ty":"Integer"}]}}}"#,
            );
            let mut err = PipeqlError {
                kind: PIPEQL_ERR_NONE,
                message: ptr::null_mut(),
            };
            let res = pipeql_compile_with_catalog(src, dialect, catalog, &mut err);
            assert!(res.is_null(), "compile with invalid column should fail");
            let msg = if err.message.is_null() {
                "<null>".to_string()
            } else {
                CStr::from_ptr(err.message).to_str().unwrap().to_string()
            };
            assert_eq!(
                err.kind, PIPEQL_ERR_ANALYSIS,
                "expected ANALYSIS error, got kind={}, msg={}",
                err.kind, msg
            );
            assert!(msg.contains("nope"), "error should mention column: {msg}");
            pipeql_error_clear(&mut err);
            free_cstr(src);
            free_cstr(dialect);
            free_cstr(catalog);
        }
    }

    #[test]
    fn test_compile_with_catalog_null() {
        unsafe {
            let src = cstr("from users | select [id]");
            let dialect = cstr("sqlite");
            let mut err = PipeqlError {
                kind: PIPEQL_ERR_NONE,
                message: ptr::null_mut(),
            };
            let res = pipeql_compile_with_catalog(src, dialect, ptr::null(), &mut err);
            assert!(!res.is_null(), "null catalog should behave like compile");
            pipeql_result_free(res);
            free_cstr(src);
            free_cstr(dialect);
        }
    }

    #[test]
    fn test_parse_returns_json() {
        unsafe {
            let src = cstr("from users | filter id == $id | select [id]");
            let mut err = PipeqlError {
                kind: PIPEQL_ERR_NONE,
                message: ptr::null_mut(),
            };
            let json_ptr = pipeql_parse(src, &mut err);
            assert!(!json_ptr.is_null(), "parse should succeed");
            let json_str = CStr::from_ptr(json_ptr).to_str().unwrap();
            let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();
            assert!(
                parsed.is_object() || parsed.is_array(),
                "should be valid JSON"
            );
            pipeql_string_free(json_ptr);
            free_cstr(src);
        }
    }

    #[test]
    fn test_parse_error() {
        unsafe {
            let src = cstr("from users | explode");
            let mut err = PipeqlError {
                kind: PIPEQL_ERR_NONE,
                message: ptr::null_mut(),
            };
            let json_ptr = pipeql_parse(src, &mut err);
            assert!(json_ptr.is_null(), "parse of bad source should fail");
            assert_eq!(err.kind, PIPEQL_ERR_PARSE);
            pipeql_error_clear(&mut err);
            free_cstr(src);
        }
    }

    #[test]
    fn test_supported_dialects() {
        unsafe {
            let json_ptr = pipeql_supported_dialects();
            assert!(!json_ptr.is_null());
            let json_str = CStr::from_ptr(json_ptr).to_str().unwrap();
            let dialects: Vec<String> = serde_json::from_str(json_str).unwrap();
            assert_eq!(dialects.len(), 4);
            assert!(dialects.contains(&"postgres".to_string()));
            assert!(dialects.contains(&"sqlite".to_string()));
            assert!(dialects.contains(&"duckdb".to_string()));
            assert!(dialects.contains(&"mysql".to_string()));
            pipeql_string_free(json_ptr);
        }
    }

    #[test]
    fn test_parameter_count() {
        unsafe {
            let src = cstr("from users | filter age >= $min and status == $s | select [id]");
            let dialect = cstr("postgres");
            let mut err = PipeqlError {
                kind: PIPEQL_ERR_NONE,
                message: ptr::null_mut(),
            };
            let res = pipeql_compile(src, dialect, &mut err);
            assert!(!res.is_null());
            let r = &*res;
            assert_eq!(r.parameter_count, 2);
            let params: Vec<String> =
                serde_json::from_str(CStr::from_ptr(r.params_json).to_str().unwrap()).unwrap();
            assert_eq!(r.parameter_count, params.len() as i32);
            pipeql_result_free(res);
            free_cstr(src);
            free_cstr(dialect);
        }
    }

    #[test]
    fn test_query_builder_read_pipeline() {
        unsafe {
            let mut q = pipeql_query_from(cstr("notes"));
            q = pipeql_query_filter(q, cstr("is_archived == 0"));
            q = pipeql_query_sort(q, cstr("created_at desc"));
            q = pipeql_query_take(q, 10);

            let src = pipeql_query_source(q);
            assert_eq!(
                CStr::from_ptr(src).to_str().unwrap(),
                "from notes | filter is_archived == 0 | sort [created_at desc] | take 10"
            );
            pipeql_string_free(src);

            let mut err = PipeqlError {
                kind: PIPEQL_ERR_NONE,
                message: ptr::null_mut(),
            };
            let res = pipeql_query_compile(q, cstr("postgres"), &mut err);
            assert!(!res.is_null(), "builder compile should succeed");
            let r = &*res;
            assert!(CStr::from_ptr(r.sql).to_str().unwrap().contains("SELECT * FROM notes"));
            pipeql_result_free(res);
            pipeql_query_free(q);
            free_cstr(cstr("notes"));
        }
    }

    #[test]
    fn test_query_builder_delete_update_all() {
        unsafe {
            // delete_all compiles to a full-table DELETE (no filter needed).
            let mut q = pipeql_query_from(cstr("notes"));
            q = pipeql_query_delete_all(q);
            let mut err = PipeqlError {
                kind: PIPEQL_ERR_NONE,
                message: ptr::null_mut(),
            };
            let res = pipeql_query_compile(q, cstr("sqlite"), &mut err);
            assert!(!res.is_null(), "delete all should compile");
            let sql = CStr::from_ptr((*res).sql).to_str().unwrap().to_string();
            assert_eq!(sql, "DELETE FROM notes;");
            pipeql_result_free(res);
            pipeql_query_free(q);

            // update_all compiles to a full-table UPDATE (no WHERE).
            let mut q = pipeql_query_from(cstr("notes"));
            q = pipeql_query_update_all(q, cstr("title = $t"));
            let res = pipeql_query_compile(q, cstr("sqlite"), &mut err);
            assert!(!res.is_null(), "update all should compile");
            let sql = CStr::from_ptr((*res).sql).to_str().unwrap().to_string();
            assert!(sql.starts_with("UPDATE notes"));
            assert!(!sql.contains("WHERE"));
            pipeql_result_free(res);
            pipeql_query_free(q);
            free_cstr(cstr("notes"));
        }
    }

    #[test]
    fn test_query_builder_joins_and_group() {
        unsafe {
            let mut q = pipeql_query_from(cstr("a"));
            q = pipeql_query_left_join(q, cstr("b"), cstr("a.id == b.a_id"));
            q = pipeql_query_group(q, cstr("region"), cstr("total = sum(amt), n = count(*)"));

            let src = pipeql_query_source(q);
            assert_eq!(
                CStr::from_ptr(src).to_str().unwrap(),
                "from a | left join b on a.id == b.a_id | group [region] (total = sum(amt), n = count(*))"
            );
            pipeql_string_free(src);
            pipeql_query_free(q);
            free_cstr(cstr("a"));
        }
    }

    #[test]
    fn test_query_builder_insert_and_upsert() {
        unsafe {
            let mut q = pipeql_query_into(cstr("users"));
            q = pipeql_query_upsert(q, cstr("id = $id, name = $name"));
            q = pipeql_query_conflict(q, cstr("id"));
            q = pipeql_query_do_update(q, cstr("name = $name"));

            let src = pipeql_query_source(q);
            assert_eq!(
                CStr::from_ptr(src).to_str().unwrap(),
                "into users | upsert [id = $id, name = $name] | conflict [id] | do update [name = $name]"
            );
            pipeql_string_free(src);

            let mut err = PipeqlError {
                kind: PIPEQL_ERR_NONE,
                message: ptr::null_mut(),
            };
            let res = pipeql_query_compile(q, cstr("postgres"), &mut err);
            assert!(!res.is_null(), "builder upsert compile should succeed");
            let r = &*res;
            assert_eq!(CStr::from_ptr(r.statement_type).to_str().unwrap(), "upsert");
            assert!(CStr::from_ptr(r.sql).to_str().unwrap().contains("ON CONFLICT (id)"));
            pipeql_result_free(res);
            pipeql_query_free(q);
            free_cstr(cstr("users"));
        }
    }

    #[test]
    fn test_query_builder_compile_error() {
        unsafe {
            // Unfiltered delete → analysis rejects missing filter guard.
            let mut q = pipeql_query_from(cstr("notes"));
            q = pipeql_query_delete(q);
            let mut err = PipeqlError {
                kind: PIPEQL_ERR_NONE,
                message: ptr::null_mut(),
            };
            let res = pipeql_query_compile(q, cstr("postgres"), &mut err);
            assert!(res.is_null(), "unfiltered delete should fail");
            assert_eq!(err.kind, PIPEQL_ERR_ANALYSIS);
            pipeql_error_clear(&mut err);
            pipeql_query_free(q);

            // With a filter present, take before delete → codegen rejects
            // non-filter steps before the mutation.
            let mut q = pipeql_query_from(cstr("notes"));
            q = pipeql_query_filter(q, cstr("a == 1"));
            q = pipeql_query_take(q, 5);
            q = pipeql_query_delete(q);
            let res = pipeql_query_compile(q, cstr("postgres"), &mut err);
            assert!(res.is_null(), "take before delete should fail");
            assert_eq!(err.kind, PIPEQL_ERR_CODEGEN);
            pipeql_error_clear(&mut err);
            pipeql_query_free(q);
            free_cstr(cstr("notes"));
        }
    }

    #[test]
    fn test_query_builder_null_safety() {
        unsafe {
            // NULL handle: source returns NULL, compile reports an error
            let src = pipeql_query_source(ptr::null());
            assert!(src.is_null());
            let mut err = PipeqlError {
                kind: PIPEQL_ERR_NONE,
                message: ptr::null_mut(),
            };
            let res = pipeql_query_compile(ptr::null(), cstr("postgres"), &mut err);
            assert!(res.is_null());
            assert_eq!(err.kind, PIPEQL_ERR_PARSE);
            pipeql_error_clear(&mut err);
            pipeql_query_free(ptr::null_mut());
        }
    }

    #[test]
    fn test_query_builder_null_string_args() {
        unsafe {
            // Constructors: NULL required string => NULL builder, nothing leaks
            assert!(pipeql_query_from(ptr::null()).is_null());
            assert!(pipeql_query_into(ptr::null()).is_null());
            assert!(pipeql_query_raw(ptr::null()).is_null());

            // Stage functions: NULL required string => NULL returned, the
            // builder handle is untouched (not freed) and unchanged.
            let q = pipeql_query_from(cstr("notes"));
            assert!(!q.is_null());

            // Single-string stages
            assert!(pipeql_query_filter(q, ptr::null()).is_null());
            assert!(pipeql_query_select(q, ptr::null()).is_null());
            assert!(pipeql_query_derive(q, ptr::null()).is_null());
            assert!(pipeql_query_sort(q, ptr::null()).is_null());
            assert!(pipeql_query_union(q, ptr::null()).is_null());
            assert!(pipeql_query_union_all(q, ptr::null()).is_null());
            assert!(pipeql_query_raw_stage(q, ptr::null()).is_null());
            assert!(pipeql_query_insert(q, ptr::null()).is_null());
            assert!(pipeql_query_update(q, ptr::null()).is_null());
            assert!(pipeql_query_update_all(q, ptr::null()).is_null());
            assert!(pipeql_query_upsert(q, ptr::null()).is_null());
            assert!(pipeql_query_conflict(q, ptr::null()).is_null());
            assert!(pipeql_query_do_update(q, ptr::null()).is_null());

            // Two-string stages: NULL on either argument
            assert!(pipeql_query_join(q, ptr::null(), cstr("a.id = b.id")).is_null());
            assert!(pipeql_query_join(q, cstr("tags"), ptr::null()).is_null());
            assert!(pipeql_query_left_join(q, ptr::null(), cstr("a.id = b.id")).is_null());
            assert!(pipeql_query_right_join(q, cstr("tags"), ptr::null()).is_null());
            assert!(pipeql_query_full_join(q, ptr::null(), cstr("a.id = b.id")).is_null());
            assert!(pipeql_query_inner_join(q, cstr("tags"), ptr::null()).is_null());
            assert!(pipeql_query_group(q, ptr::null(), cstr("count(*)")).is_null());
            assert!(pipeql_query_group(q, cstr("category"), ptr::null()).is_null());

            // Builder is still alive and unchanged after all the NULL calls
            let src = pipeql_query_source(q);
            let got = CString::from_raw(src);
            assert_eq!(got.to_str().unwrap(), "from notes");

            // NULL handle on no-string stages is also NULL (no crash)
            assert!(pipeql_query_take(ptr::null_mut(), 5).is_null());
            assert!(pipeql_query_skip(ptr::null_mut(), 5).is_null());
            assert!(pipeql_query_delete(ptr::null_mut()).is_null());
            assert!(pipeql_query_delete_all(ptr::null_mut()).is_null());

            pipeql_query_free(q);
        }
    }

    #[test]
    fn test_compile_all_dialects() {
        for dialect in &["postgres", "sqlite", "duckdb", "mysql"] {
            unsafe {
                let src = cstr("from users | filter id == $id | take 5");
                let d = cstr(dialect);
                let mut err = PipeqlError {
                    kind: PIPEQL_ERR_NONE,
                    message: ptr::null_mut(),
                };
                let res = pipeql_compile(src, d, &mut err);
                assert!(
                    !res.is_null(),
                    "compile should succeed for dialect: {dialect}"
                );
                pipeql_result_free(res);
                free_cstr(src);
                free_cstr(d);
            }
        }
    }
}
