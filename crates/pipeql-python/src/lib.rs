//! Python bindings for PipeQL (pip package `pipeql-python`).
//!
//! Build with:
//! ```sh
//! maturin develop --release -m crates/pipeql-python/Cargo.toml
//! # or: maturin build --release -m crates/pipeql-python/Cargo.toml
//! ```
//!
//! Usage:
//! ```python
//! import pipeql_python as pipeql
//! result = pipeql.compile("from users | filter age > $min", "postgres")
//! print(result["sql"])     # SELECT * FROM users WHERE (age > $1);
//! print(result["params"])  # ["min"]
//! ```
#![allow(clippy::useless_conversion)]

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAnyMethods, PyDict, PyDictMethods, PyList, PyListMethods};

use pipeql_core::{api, Catalog, ColumnMeta, PipeQLError, TableMeta, ValueType};

fn compile_result(
    py: Python<'_>,
    source: &str,
    dialect: &str,
    catalog: Option<Catalog>,
) -> PyResult<PyObject> {
    let compiled = match catalog {
        Some(cat) => api::compile_with_catalog(source, dialect, Some(&cat)),
        None => api::compile(source, dialect),
    };
    match compiled {
        Ok(compiled) => {
            let analysis_json = serde_json::to_string(&compiled.analysis)
                .map_err(|e| PyRuntimeError::new_err(format!("serialization error: {e}")))?;
            let dict = PyDict::new_bound(py);
            dict.set_item("sql", &compiled.sql)?;
            dict.set_item("params", compiled.params.clone())?;
            dict.set_item("statement_type", compiled.statement_type.as_str())?;
            dict.set_item("is_mutation", compiled.is_mutation)?;
            // Use native json.loads() for analysis — faster than recursive Rust traversal
            let json_mod = py.import_bound("json")?;
            let analysis_obj = json_mod.call_method1("loads", (analysis_json,))?;
            dict.set_item("analysis", analysis_obj)?;
            dict.set_item("parameter_count", compiled.params.len())?;
            Ok(dict.into_any().unbind())
        }
        Err(e) => Err(py_error(&e)),
    }
}

/// Compile a PipeQL source string for a target dialect.
///
/// Args:
///     source (str): The PipeQL query source.
///     dialect (str, optional): `"postgres"` (default), `"sqlite"`, `"duckdb"`, or `"mysql"`.
///
/// Returns:
///     dict: `{"sql": str, "params": list[str], "statement_type": str,
///     "is_mutation": bool, "analysis": dict, "parameter_count": int}`
#[pyfunction]
#[pyo3(signature = (source, dialect=None))]
#[allow(clippy::useless_conversion)]
fn compile(source: &str, dialect: Option<&str>) -> PyResult<PyObject> {
    let dialect = dialect.unwrap_or("postgres");
    Python::with_gil(|py| compile_result(py, source, dialect, None))
}

/// Compile a PipeQL source string with a schema catalog for column validation.
///
/// Args:
///     source (str): The PipeQL query source.
///     dialect (str, optional): Target dialect, default `"postgres"`.
///     catalog (dict, optional): Schema catalog mapping table names to
///         `{"name": ..., "columns": [{"name": ..., "ty": "Integer"|"Float"|"String"|"Bool"|"Null"|"Any"}]}`.
#[pyfunction]
#[pyo3(signature = (source, dialect=None, catalog=None))]
#[allow(clippy::useless_conversion)]
fn compile_with_catalog(
    source: &str,
    dialect: Option<&str>,
    catalog: Option<&Bound<'_, PyDict>>,
) -> PyResult<PyObject> {
    let dialect = dialect.unwrap_or("postgres");
    let parsed_catalog: Option<Catalog> = match catalog {
        Some(d) => Some(parse_catalog(d)?),
        None => None,
    };
    Python::with_gil(|py| compile_result(py, source, dialect, parsed_catalog))
}

/// Convert a Python catalog dict `{table_name: {"name":..., "columns":[...]}}`
/// into a `Catalog`.
fn parse_catalog(d: &Bound<'_, PyDict>) -> PyResult<Catalog> {
    let mut catalog = Catalog::new();
    for (k, v) in d.iter() {
        let name = k
            .extract::<String>()
            .map_err(|_| PyValueError::new_err("catalog keys must be table-name strings"))?;
        let meta = v
            .downcast::<PyDict>()
            .map_err(|_| PyValueError::new_err(format!("catalog entry '{name}' must be a dict")))?;
        let columns_raw = meta
            .get_item("columns")?
            .ok_or_else(|| PyValueError::new_err(format!("table '{name}' missing 'columns'")))?;
        let columns_list = columns_raw.downcast::<PyList>().map_err(|_| {
            PyValueError::new_err(format!("table '{name}' 'columns' must be a list"))
        })?;
        let mut columns = Vec::new();
        for col in columns_list.iter() {
            let col_dict = col.downcast::<PyDict>().map_err(|_| {
                PyValueError::new_err(format!("table '{name}' column must be a dict"))
            })?;
            let col_name = col_dict
                .get_item("name")?
                .ok_or_else(|| PyValueError::new_err("column missing 'name'"))?
                .extract::<String>()?;
            let ty = match col_dict
                .get_item("ty")?
                .ok_or_else(|| PyValueError::new_err(format!("column '{col_name}' missing 'ty'")))?
                .extract::<String>()?
                .as_str()
            {
                "Integer" | "integer" => ValueType::Integer,
                "Float" | "float" => ValueType::Float,
                "String" | "string" => ValueType::String,
                "Bool" | "bool" => ValueType::Bool,
                "Null" | "null" => ValueType::Null,
                "Timestamp" | "timestamp" => ValueType::Timestamp,
                _ => ValueType::Any,
            };
            columns.push(ColumnMeta { name: col_name, ty });
        }
        catalog.add_table(TableMeta { name, columns });
    }
    Ok(catalog)
}

/// Compile a PipeQL source string with schema validation derived from DDL.
#[pyfunction]
#[pyo3(signature = (source, dialect=None, schema=None))]
#[allow(clippy::useless_conversion)]
fn compile_with_schema(
    source: &str,
    dialect: Option<&str>,
    schema: Option<&str>,
) -> PyResult<PyObject> {
    let dialect = dialect.unwrap_or("postgres");
    let schema_str = schema.ok_or_else(|| PyValueError::new_err("schema DDL string is required"))?;
    let parsed_catalog = api::catalog_from_schema(schema_str).map_err(|e| py_error(&e))?;
    Python::with_gil(|py| compile_result(py, source, dialect, Some(parsed_catalog)))
}

/// Derive a catalog dict from one or more PipeQL table statements.
#[pyfunction]
#[allow(clippy::useless_conversion)]
fn catalog_from_schema(schema: &str) -> PyResult<PyObject> {
    let catalog = api::catalog_from_schema(schema).map_err(|e| py_error(&e))?;
    let map: std::collections::HashMap<&str, &pipeql_core::TableMeta> =
        catalog.tables().map(|t| (t.name.as_str(), t)).collect();
    let json = serde_json::to_string(&map)
        .map_err(|e| PyRuntimeError::new_err(format!("serialization error: {e}")))?;
    Python::with_gil(|py| {
        let json_mod = py.import_bound("json")?;
        let obj = json_mod.call_method1("loads", (json,))?;
        Ok(obj.unbind())
    })
}

/// Parse a PipeQL source into a JSON-serializable AST (with spans and comments).
#[pyfunction]
#[allow(clippy::useless_conversion)]
fn parse(source: &str) -> PyResult<PyObject> {
    match api::parse_statement(source) {
        Ok(stmt) => {
            let json = serde_json::to_string(&stmt)
                .map_err(|e| PyRuntimeError::new_err(format!("serialization error: {e}")))?;
            Python::with_gil(|py| {
                let json_mod = py.import_bound("json")?;
                json_mod.call_method1("loads", (json,))
                    .map(|obj| obj.unbind())
            })
        }
        Err(PipeQLError::Parse(errs)) => {
            let message = errs
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("\n");
            Err(PyValueError::new_err(message))
        }
        Err(e) => Err(py_error(&e)),
    }
}

/// The list of supported target dialects.
#[pyfunction]
fn supported_dialects() -> Vec<&'static str> {
    api::supported_dialects().to_vec()
}

/// PipeQL version string.
#[pyfunction]
fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn py_error(e: &PipeQLError) -> PyErr {
    match e {
        PipeQLError::Parse(_) => PyValueError::new_err(e.to_string()),
        PipeQLError::Analysis(_) => PyValueError::new_err(e.to_string()),
        PipeQLError::Codegen(_) => PyRuntimeError::new_err(e.to_string()),
    }
}

#[pymodule]
fn pipeql_python(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(compile, m)?)?;
    m.add_function(wrap_pyfunction!(compile_with_catalog, m)?)?;
    m.add_function(wrap_pyfunction!(compile_with_schema, m)?)?;
    m.add_function(wrap_pyfunction!(catalog_from_schema, m)?)?;
    m.add_function(wrap_pyfunction!(parse, m)?)?;
    m.add_function(wrap_pyfunction!(supported_dialects, m)?)?;
    m.add_function(wrap_pyfunction!(version, m)?)?;
    Ok(())
}
