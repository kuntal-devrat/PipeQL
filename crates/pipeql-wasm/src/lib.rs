//! WASM bindings for PipeQL, powering the `@pipeql/js` JavaScript SDK.
//!
//! Compile with:
//! ```sh
//! wasm-pack build crates/pipeql-wasm --target web --release --out-dir ../../js/dist
//! ```

use std::cell::RefCell;

use wasm_bindgen::prelude::*;

use pipeql_core::{api, Analysis, StatementType};

/// A compiled query, exposed to JavaScript as a plain object.
///
/// The `analysis` payload is built **lazily**: it is the most expensive thing
/// to cross the JS↔WASM boundary (a nested param map with per-param types and
/// occurrence spans), and most callers never read it. It is serialized into a
/// JS object only on first access and cached thereafter.
#[derive(Debug)]
#[wasm_bindgen]
pub struct Compiled {
    sql: String,
    params: js_sys::Array,
    statement_type: StatementType,
    is_mutation: bool,
    parameter_count: usize,
    analysis_src: Analysis,
    analysis: RefCell<Option<JsValue>>,
}

#[wasm_bindgen]
impl Compiled {
    /// The target-dialect SQL text.
    #[wasm_bindgen(getter)]
    pub fn sql(&self) -> String {
        self.sql.clone()
    }

    /// The ordered array of parameter names.
    #[wasm_bindgen(getter)]
    pub fn params(&self) -> js_sys::Array {
        self.params.clone()
    }

    /// The statement kind: "select", "insert", "update", "delete",
    /// "create_table", "upsert", "union".
    #[wasm_bindgen(getter)]
    pub fn statement_type(&self) -> String {
        self.statement_type.as_str().to_string()
    }

    /// True for mutations (insert/update/delete/upsert).
    #[wasm_bindgen(getter)]
    pub fn is_mutation(&self) -> bool {
        self.is_mutation
    }

    /// Number of distinct bind parameters.
    #[wasm_bindgen(getter)]
    pub fn parameter_count(&self) -> usize {
        self.parameter_count
    }

    /// The full analysis document (param map, inferred types, occurrences).
    /// Serialized to a JS object on first access (and cached), so the common
    /// sql/params path never pays for it.
    #[wasm_bindgen(getter)]
    pub fn analysis(&self) -> JsValue {
        if let Some(cached) = self.analysis.borrow().as_ref() {
            return cached.clone();
        }
        let value = serde_wasm_bindgen::to_value(&self.analysis_src).unwrap_or(JsValue::NULL);
        *self.analysis.borrow_mut() = Some(value.clone());
        value
    }
}

/// Build a `Compiled` from a core result.
fn to_compiled(c: pipeql_core::CompiledQuery) -> Compiled {
    let parameter_count = c.params.len();
    Compiled {
        sql: c.sql,
        params: c
            .params
            .iter()
            .map(|s| JsValue::from_str(s))
            .collect::<js_sys::Array>(),
        statement_type: c.statement_type,
        is_mutation: c.is_mutation,
        analysis_src: c.analysis,
        analysis: RefCell::new(None),
        parameter_count,
    }
}

/// Compile a PipeQL source string for a target dialect.
///
/// `dialect` defaults to `"postgres"`. Returns a `Compiled` object with `sql`,
/// `params`, `statementType`, `isMutation`, and `analysis` properties, or
/// throws on error.
#[wasm_bindgen]
pub fn compile(source: &str, dialect: Option<String>) -> Result<Compiled, JsValue> {
    let dialect = dialect.as_deref().unwrap_or("postgres");
    api::compile(source, dialect)
        .map(to_compiled)
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Compile a PipeQL source string, validating columns against a JSON schema
/// catalog. The catalog format matches the Rust `Catalog` type:
///
/// ```json
/// { "tables": { "users": { "name": "users", "columns": [ { "name": "id", "ty": "Integer" } ] } } }
/// ```
#[wasm_bindgen(js_name = compileWithCatalog)]
pub fn compile_with_catalog(
    source: &str,
    dialect: Option<String>,
    catalog_json: &str,
) -> Result<Compiled, JsValue> {
    let dialect = dialect.as_deref().unwrap_or("postgres");
    let catalog: pipeql_core::Catalog = serde_json::from_str(catalog_json)
        .map_err(|e| JsValue::from_str(&format!("invalid catalog JSON: {e}")))?;
    api::compile_with_catalog(source, dialect, Some(&catalog))
        .map(to_compiled)
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Compile a PipeQL source string with schema validation derived from DDL.
#[wasm_bindgen(js_name = compileWithSchema)]
pub fn compile_with_schema(
    source: &str,
    dialect: Option<String>,
    schema: &str,
) -> Result<Compiled, JsValue> {
    let dialect = dialect.as_deref().unwrap_or("postgres");
    api::compile_with_schema(source, dialect, schema)
        .map(to_compiled)
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Derive a catalog JSON object from one or more PipeQL table DDL statements.
#[wasm_bindgen(js_name = catalogFromSchema)]
pub fn catalog_from_schema(schema: &str) -> Result<JsValue, JsValue> {
    let catalog = api::catalog_from_schema(schema)
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    let map: std::collections::HashMap<&str, &pipeql_core::TableMeta> =
        catalog.tables().map(|t| (t.name.as_str(), t)).collect();
    let json = serde_json::to_string(&map)
        .map_err(|e| JsValue::from_str(&format!("serialization error: {e}")))?;
    js_sys::JSON::parse(&json)
        .map_err(|e| JsValue::from_str(&format!("JSON parse error: {e:?}")))
}

/// Parse-only: returns a JSON description of the lossless AST (spans, comments,
/// steps). Useful for editors and tooling.
#[wasm_bindgen(js_name = parseAst)]
pub fn parse_ast(source: &str) -> Result<JsValue, JsValue> {
    match api::parse_statement(source) {
        Ok(stmt) => serde_wasm_bindgen::to_value(&stmt)
            .map_err(|e| JsValue::from_str(&format!("serialization error: {e}"))),
        Err(e) => Err(JsValue::from_str(&e.to_string())),
    }
}

/// List of supported target dialects.
#[wasm_bindgen(js_name = supportedDialects)]
pub fn supported_dialects() -> js_sys::Array {
    api::supported_dialects()
        .iter()
        .map(|s| JsValue::from_str(s))
        .collect::<js_sys::Array>()
}

/// PipeQL version string.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(all(test, target_arch = "wasm32"))]
mod tests {
    use super::*;

    #[test]
    fn compile_smoke() {
        let c = compile("from users | filter age > $min | select [id]", None).unwrap();
        assert!(c.sql().contains("SELECT id FROM users"));
        assert_eq!(c.params().length(), 1);
        assert_eq!(c.statement_type(), "select");
        assert!(!c.is_mutation());
    }

    #[test]
    fn compile_mutation_metadata() {
        let c = compile("into notes | insert [title = $t]", None).unwrap();
        assert_eq!(c.statement_type(), "insert");
        assert!(c.is_mutation());
    }

    #[test]
    fn compile_errors_are_js_values() {
        let err = compile("from users | explode", None).unwrap_err();
        assert!(err.is_string());
    }
}
