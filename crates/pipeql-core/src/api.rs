//! High-level, ergonomic API for compiling PipeQL sources.
//!
//! This is the facade used by the CLI, the polyglot bindings (WASM, Python,
//! C-FFI), and application code. It ties together lexer, parser, analyzer, and
//! codegen into a single call.

use std::fmt;

use crate::analyzer::{Analysis, AnalyzerError, Catalog};
use crate::ast::Statement;
use crate::codegen::{get_dialect, CodegenError};
use crate::parser::{ParseError, Parser};

/// The kind of statement a compiled query represents. Drivers use this to
/// choose the right execution path (return rows vs. execute + metadata) without
/// inspecting the SQL text.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum StatementType {
    /// A read pipeline: `from <table> | ...` without update/delete.
    Select,
    /// `into <table> | insert [...]`.
    Insert,
    /// `from <table> | [filters] | update [...]`.
    Update,
    /// `from <table> | [filters] | delete`.
    Delete,
    /// `table <name> [column defs]` DDL.
    CreateTable,
    /// `into <table> | upsert [...] | conflict [...] | do update [...]`.
    Upsert,
    /// `<statement> | union [all] <statement>`.
    Union,
}

impl StatementType {
    /// Classify a parsed statement AST.
    pub fn from_statement(stmt: &crate::ast::Statement) -> StatementType {
        match stmt {
            crate::ast::Statement::Insert(_) => StatementType::Insert,
            crate::ast::Statement::CreateTable(_) => StatementType::CreateTable,
            crate::ast::Statement::Upsert(_) => StatementType::Upsert,
            crate::ast::Statement::Union(_) => StatementType::Union,
            crate::ast::Statement::Pipeline(p) => {
                if p.steps
                    .iter()
                    .any(|s| matches!(s, crate::ast::PipelineStep::Update { .. }))
                {
                    StatementType::Update
                } else if p
                    .steps
                    .iter()
                    .any(|s| matches!(s, crate::ast::PipelineStep::Delete { .. }))
                {
                    StatementType::Delete
                } else {
                    StatementType::Select
                }
            }
        }
    }

    /// The stable snake_case string form (`"insert"`, `"create_table"`, ...).
    pub fn as_str(&self) -> &'static str {
        match self {
            StatementType::Select => "select",
            StatementType::Insert => "insert",
            StatementType::Update => "update",
            StatementType::Delete => "delete",
            StatementType::CreateTable => "create_table",
            StatementType::Upsert => "upsert",
            StatementType::Union => "union",
        }
    }

    /// True for statements that write (insert/update/delete/upsert).
    pub fn is_mutation(&self) -> bool {
        matches!(
            self,
            StatementType::Insert
                | StatementType::Update
                | StatementType::Delete
                | StatementType::Upsert
        )
    }
}

impl fmt::Display for StatementType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A fully compiled query: dialect SQL plus the ordered parameter vector.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledQuery {
    pub sql: String,
    pub params: Vec<String>,
    /// The statement kind, so callers can dispatch `.all()` vs `.run()` without
    /// parsing the SQL prefix.
    pub statement_type: StatementType,
    /// Convenience flag: true for insert/update/delete.
    pub is_mutation: bool,
    pub analysis: Analysis,
}

/// Unified error type covering parsing, analysis, and codegen.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, PartialEq)]
pub enum PipeQLError {
    /// Lexer or parser failures (lexer errors are surfaced as parse errors
    /// with spans).
    Parse(Vec<ParseError>),
    /// Semantic analysis failures (column/scope validation).
    Analysis(Vec<AnalyzerError>),
    /// Codegen failures (unsupported dialect, invalid AST).
    Codegen(CodegenError),
}

impl fmt::Display for PipeQLError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PipeQLError::Parse(errs) => {
                for e in errs {
                    write!(f, "{e}")?;
                }
                Ok(())
            }
            PipeQLError::Analysis(errs) => {
                for e in errs {
                    write!(
                        f,
                        "Analysis error at {}..{}: {}",
                        e.span.start, e.span.end, e.message
                    )?;
                }
                Ok(())
            }
            PipeQLError::Codegen(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for PipeQLError {}

/// Compile a PipeQL source string for the given dialect.
pub fn compile(source: &str, dialect: &str) -> Result<CompiledQuery, PipeQLError> {
    compile_with_catalog(source, dialect, None)
}

/// Compile a PipeQL source string, optionally validating columns against a
/// schema catalog.
pub fn compile_with_catalog(
    source: &str,
    dialect: &str,
    catalog: Option<&Catalog>,
) -> Result<CompiledQuery, PipeQLError> {
    let stmt = parse_statement(source)?;
    let d = get_dialect(dialect).map_err(PipeQLError::Codegen)?;
    let statement_type = StatementType::from_statement(&stmt);
    let (sql, params, analysis) = match &stmt {
        Statement::Pipeline(pipeline) => {
            let analysis = d
                .analyze(pipeline, catalog)
                .map_err(PipeQLError::Analysis)?;
            let (sql, params) = d
                .compile_only(pipeline)
                .map_err(PipeQLError::Codegen)?;
            (sql, params, analysis)
        }
        Statement::Insert(_)
        | Statement::Upsert(_)
        | Statement::CreateTable(_)
        | Statement::Union(_) => {
            let analysis = d
                .analyze_statement(&stmt, catalog)
                .map_err(PipeQLError::Analysis)?;
            let (sql, params) = d
                .compile_statement_only(&stmt)
                .map_err(PipeQLError::Codegen)?;
            (sql, params, analysis)
        }
    };
    Ok(CompiledQuery {
        sql,
        params,
        statement_type,
        is_mutation: statement_type.is_mutation(),
        analysis,
    })
}

/// Parse a PipeQL source into a lossless AST (preserving comments and spans).
/// Returns read pipelines only; use [`parse_statement`] for mutations/DDL.
pub fn parse(source: &str) -> Result<crate::ast::Pipeline, PipeQLError> {
    let mut parser = Parser::new(source).map_err(PipeQLError::Parse)?;
    parser.parse_pipeline().map_err(PipeQLError::Parse)
}

/// Parse a PipeQL source into a lossless statement AST (read pipeline, insert,
/// or table DDL), preserving comments and spans.
pub fn parse_statement(source: &str) -> Result<crate::ast::Statement, PipeQLError> {
    let mut parser = Parser::new(source).map_err(PipeQLError::Parse)?;
    parser.parse_statement().map_err(PipeQLError::Parse)
}

/// Derive an analyzer schema [`Catalog`] from one or more PipeQL `table` DDL statements.
pub fn catalog_from_schema(schema: &str) -> Result<Catalog, PipeQLError> {
    let mut parser = Parser::new(schema).map_err(PipeQLError::Parse)?;
    let tables = parser.parse_schema().map_err(PipeQLError::Parse)?;

    let mut catalog = Catalog::new();
    for t in tables {
        let name = t.name.name;
        if catalog.table(&name).is_some() {
            return Err(PipeQLError::Analysis(vec![AnalyzerError {
                message: format!("duplicate table '{name}' in schema"),
                span: t.span,
                suggestion: None,
            }]));
        }
        let columns = t
            .columns
            .into_iter()
            .map(|c| {
                let ty = match c.ty {
                    crate::ast::ColumnType::Integer => crate::analyzer::ValueType::Integer,
                    crate::ast::ColumnType::Float => crate::analyzer::ValueType::Float,
                    crate::ast::ColumnType::String => crate::analyzer::ValueType::String,
                    crate::ast::ColumnType::Bool => crate::analyzer::ValueType::Bool,
                    crate::ast::ColumnType::Timestamp => crate::analyzer::ValueType::Timestamp,
                };
                crate::analyzer::ColumnMeta {
                    name: c.name.name,
                    ty,
                }
            })
            .collect();
        catalog.add_table(crate::analyzer::TableMeta { name, columns });
    }
    Ok(catalog)
}

/// Compile a PipeQL source string with analyzer validation derived from a schema DDL string.
pub fn compile_with_schema(
    source: &str,
    dialect: &str,
    schema: &str,
) -> Result<CompiledQuery, PipeQLError> {
    let catalog = catalog_from_schema(schema)?;
    compile_with_catalog(source, dialect, Some(&catalog))
}

/// The list of supported target dialect names.
pub fn supported_dialects() -> &'static [&'static str] {
    &["postgres", "sqlite", "duckdb", "mysql"]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_facade() {
        let result = compile("from users | filter age > $min | select [id]", "postgres").unwrap();
        assert!(result.sql.contains("SELECT id FROM users"));
        assert_eq!(result.params, vec!["min"]);
        assert_eq!(result.analysis.param_names(), vec!["min"]);
    }

    #[test]
    fn test_compile_error_surfaces() {
        let err = compile("from users | explode [id]", "postgres").unwrap_err();
        assert!(matches!(err, PipeQLError::Parse(_)));
    }

    #[test]
    fn test_unsupported_dialect() {
        let err = compile("from t", "oracle").unwrap_err();
        assert!(matches!(
            err,
            PipeQLError::Codegen(CodegenError::UnsupportedDialect(_))
        ));
    }

    #[test]
    fn test_unfiltered_update_rejected() {
        // Documented safety rule: `update` requires a preceding `filter` step.
        let err = compile("from users | update [name = $name]", "postgres").unwrap_err();
        assert!(matches!(err, PipeQLError::Analysis(_)));
        assert!(format!("{err}").contains("requires a preceding 'filter' stage"));
    }

    #[test]
    fn test_unfiltered_delete_rejected() {
        let err = compile("from users | delete", "sqlite").unwrap_err();
        assert!(matches!(err, PipeQLError::Analysis(_)));
        assert!(format!("{err}").contains("requires a preceding 'filter' stage"));
    }

    #[test]
    fn test_update_delete_all_escape_hatch() {
        // `delete all` / `update all [...]` compile to full-table SQL with no
        // WHERE clause — the explicit opt-in bypasses the filter guard.
        for dialect in &["postgres", "sqlite", "duckdb", "mysql"] {
            let r = compile("from users | delete all", dialect).unwrap();
            assert!(r.sql.contains("DELETE FROM users"));
            assert!(!r.sql.contains("WHERE"), "{} must have no WHERE", r.sql);
            assert_eq!(r.statement_type, StatementType::Delete);

            let r = compile("from users | update all [name = $name]", dialect).unwrap();
            assert!(r.sql.contains("UPDATE users"));
            assert!(r.sql.contains("SET name ="));
            assert!(!r.sql.contains("WHERE"), "{} must have no WHERE", r.sql);
            assert_eq!(r.statement_type, StatementType::Update);

            // `all` with a filter still emits the WHERE clause.
            let r = compile("from users | filter id == $id | delete all", dialect).unwrap();
            assert!(r.sql.contains("WHERE"));
        }
    }

    #[test]
    fn test_filtered_update_delete_compile_all_dialects() {
        for dialect in &["postgres", "sqlite", "duckdb", "mysql"] {
            let r =
                compile("from users | filter id == $id | update [name = $name]", dialect).unwrap();
            assert!(r.sql.contains("UPDATE users"));
            assert!(r.sql.contains("WHERE"));
            let r = compile("from users | filter id == $id | delete", dialect).unwrap();
            assert!(r.sql.contains("DELETE FROM users"));
            assert!(r.sql.contains("WHERE"));
        }
    }

    #[test]
    fn test_statement_type_select() {
        let result = compile("from users | filter age > $min | select [id]", "postgres").unwrap();
        assert_eq!(result.statement_type, StatementType::Select);
        assert!(!result.is_mutation);
        assert_eq!(result.statement_type.as_str(), "select");
    }

    #[test]
    fn test_statement_type_insert() {
        let result = compile("into notes | insert [title = $t, is_pinned = 0]", "sqlite").unwrap();
        assert_eq!(result.statement_type, StatementType::Insert);
        assert!(result.is_mutation);
    }

    #[test]
    fn test_statement_type_update() {
        let result = compile(
            "from notes | filter id == $id | update [is_pinned = 1]",
            "sqlite",
        )
        .unwrap();
        assert_eq!(result.statement_type, StatementType::Update);
        assert!(result.is_mutation);
    }

    #[test]
    fn test_statement_type_delete() {
        let result = compile("from notes | filter id == $id | delete", "sqlite").unwrap();
        assert_eq!(result.statement_type, StatementType::Delete);
        assert!(result.is_mutation);
    }

    #[test]
    fn test_statement_type_create_table() {
        let result = compile("table notes [id int primary auto]", "sqlite").unwrap();
        assert_eq!(result.statement_type, StatementType::CreateTable);
        assert!(!result.is_mutation);
        assert_eq!(result.statement_type.as_str(), "create_table");
    }

    #[test]
    #[cfg(feature = "serde")]
    fn test_statement_type_serializes_snake_case() {
        let json = serde_json::to_string(&StatementType::CreateTable).unwrap();
        assert_eq!(json, "\"create_table\"");
    }

    #[test]
    fn test_statement_type_upsert() {
        let result = compile(
            "into users | upsert [name = $name, email = $email] | conflict [email] | do update [name = $name]",
            "postgres",
        )
        .unwrap();
        assert_eq!(result.statement_type, StatementType::Upsert);
        assert!(result.is_mutation);
        assert_eq!(result.statement_type.as_str(), "upsert");
    }

    #[test]
    fn test_statement_type_union() {
        let result = compile(
            "from active_users | select [id, name] | union from archived_users | select [id, name]",
            "postgres",
        )
        .unwrap();
        assert_eq!(result.statement_type, StatementType::Union);
        assert!(!result.is_mutation);
        assert_eq!(result.statement_type.as_str(), "union");
    }

    #[test]
    fn test_compile_upsert_postgres() {
        let result = compile(
            "into users | upsert [name = $name, email = $email] | conflict [email] | do update [name = $name]",
            "postgres",
        )
        .unwrap();
        assert!(result
            .sql
            .contains("INSERT INTO users (name, email) VALUES ($1, $2)"));
        // Postgres deduplicates params by name, so $name is $1 in both places
        assert!(result
            .sql
            .contains("ON CONFLICT (email) DO UPDATE SET name = $1"));
        assert!(result.sql.contains("RETURNING *"));
        assert_eq!(result.params, vec!["name", "email"]);
    }

    #[test]
    fn test_compile_upsert_mysql() {
        let result = compile(
            "into users | upsert [name = $name, email = $email] | conflict [email] | do update [name = $name]",
            "mysql",
        )
        .unwrap();
        assert!(result
            .sql
            .contains("INSERT INTO users (name, email) VALUES (?, ?)"));
        // MySQL ON DUPLICATE KEY UPDATE uses VALUES(col) syntax
        assert!(result.sql.contains("ON DUPLICATE KEY UPDATE name = VALUES(name)"));
        assert!(!result.sql.contains("ON CONFLICT"));
    }

    #[test]
    fn test_compile_union() {
        let result = compile(
            "from active_users | select [id, name] | union from archived_users | select [id, name]",
            "postgres",
        )
        .unwrap();
        assert!(result.sql.contains("UNION"));
        assert!(!result.sql.contains("UNION ALL"));
        assert!(result.sql.contains("SELECT id, name FROM active_users"));
        assert!(result.sql.contains("SELECT id, name FROM archived_users"));
    }

    #[test]
    fn test_compile_union_all() {
        let result = compile(
            "from active_users | select [id, name] | union all from archived_users | select [id, name]",
            "postgres",
        )
        .unwrap();
        assert!(result.sql.contains("UNION ALL"));
    }

    #[test]
    fn test_compile_left_join_sql_style_prefix() {
        let result = compile(
            "from notes | left join archive on notes.id == archive.note_id",
            "postgres",
        )
        .unwrap();
        assert!(result
            .sql
            .contains("LEFT JOIN archive ON (notes.id = archive.note_id)"));
    }

    #[test]
    fn test_compile_full_join_sql_style_prefix() {
        let result = compile(
            "from a | full join b on a.id == b.a_id | select [a.id]",
            "sqlite",
        )
        .unwrap();
        assert!(result.sql.contains("FULL OUTER JOIN b ON (a.id = b.a_id)"));
    }

    #[test]
    fn test_compile_in_list_parenthesized() {
        let result = compile("from notes | filter id in (1, 2, 3)", "sqlite").unwrap();
        assert!(result.sql.contains("(id IN (1, 2, 3))"));
        assert!(result.params.is_empty());
    }

    #[test]
    fn test_compile_not_in_parenthesized_with_params() {
        let result = compile(
            "from notes | filter id not in ($a, $b, $c)",
            "postgres",
        )
        .unwrap();
        assert!(result.sql.contains("(id NOT IN ($1, $2, $3))"));
        assert_eq!(result.params, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_compile_subquery() {
        let result = compile(
            "from orders | filter customer_id in (from customers | filter region == 'EU' | select [id])",
            "postgres",
        )
        .unwrap();
        assert!(result.sql.contains("IN (SELECT id FROM customers"));
        assert!(result.sql.contains("WHERE (region = $1)"));
        assert_eq!(result.params, vec!["EU"]);
    }

    #[test]
    fn test_upsert_params() {
        let result = compile(
            "into users | upsert [name = $name, email = $email] | conflict [email] | do update [name = $name]",
            "sqlite",
        )
        .unwrap();
        assert_eq!(result.params, vec!["name", "email", "name"]);
    }

    #[test]
    fn test_union_params() {
        let result = compile(
            "from users | filter status == $s1 | select [id] | union from archived | filter status == $s2 | select [id]",
            "postgres",
        )
        .unwrap();
        assert!(result.params.contains(&"s1".to_string()));
        assert!(result.params.contains(&"s2".to_string()));
    }

    // === Upsert: all 4 dialects ===

    #[test]
    fn test_upsert_sqlite() {
        let result = compile(
            "into users | upsert [name = $name, email = $email] | conflict [email] | do update [name = $name]",
            "sqlite",
        )
        .unwrap();
        assert!(result
            .sql
            .contains("INSERT INTO users (name, email) VALUES (?, ?)"));
        assert!(result
            .sql
            .contains("ON CONFLICT (email) DO UPDATE SET name = ?"));
        assert!(!result.sql.contains("RETURNING"));
        assert_eq!(result.params, vec!["name", "email", "name"]);
    }

    #[test]
    fn test_upsert_duckdb() {
        let result = compile(
            "into users | upsert [name = $name, email = $email] | conflict [email] | do update [name = $name]",
            "duckdb",
        )
        .unwrap();
        assert!(result
            .sql
            .contains("INSERT INTO users (name, email) VALUES (?, ?)"));
        assert!(result
            .sql
            .contains("ON CONFLICT (email) DO UPDATE SET name = ?"));
        assert!(!result.sql.contains("RETURNING"));
        assert_eq!(result.params, vec!["name", "email", "name"]);
    }

    #[test]
    fn test_upsert_multiple_conflict_columns() {
        let result = compile(
            "into users | upsert [name = $name] | conflict [org_id, email] | do update [name = $name]",
            "postgres",
        )
        .unwrap();
        assert!(result
            .sql
            .contains("ON CONFLICT (org_id, email) DO UPDATE SET name = $1"));
    }

    #[test]
    fn test_upsert_multiple_do_update_cols() {
        let result = compile(
            "into users | upsert [name = $n, email = $e] | conflict [email] | do update [name = $n, email = $e]",
            "sqlite",
        )
        .unwrap();
        assert!(result
            .sql
            .contains("ON CONFLICT (email) DO UPDATE SET name = ?, email = ?"));
        // params: $n (insert), $e (insert), $n (do update), $e (do update)
        // but sqlite doesn't dedup, so all 4 appear
        assert_eq!(result.params, vec!["n", "e", "n", "e"]);
    }

    #[test]
    fn test_upsert_with_string_literal() {
        let result = compile(
            "into users | upsert [name = 'Alice', email = $email] | conflict [email] | do update [name = 'Alice']",
            "postgres",
        )
        .unwrap();
        // 'Alice' becomes a parameter in strict_literals mode
        assert!(result.sql.contains("$1"));
        assert!(result.params.contains(&"Alice".to_string()));
    }

    // === Union: all 4 dialects ===

    #[test]
    fn test_union_sqlite() {
        let result = compile(
            "from active_users | select [id] | union from archived_users | select [id]",
            "sqlite",
        )
        .unwrap();
        assert!(result.sql.contains("UNION"));
        assert!(!result.sql.contains("UNION ALL"));
        assert!(result.sql.contains("SELECT id FROM active_users"));
        assert!(result.sql.contains("SELECT id FROM archived_users"));
    }

    #[test]
    fn test_union_duckdb() {
        let result = compile(
            "from active_users | select [id] | union from archived_users | select [id]",
            "duckdb",
        )
        .unwrap();
        assert!(result.sql.contains("UNION"));
        assert!(result.sql.contains("SELECT id FROM active_users"));
    }

    #[test]
    fn test_union_mysql() {
        let result = compile(
            "from active_users | select [id] | union from archived_users | select [id]",
            "mysql",
        )
        .unwrap();
        assert!(result.sql.contains("UNION"));
        assert!(!result.sql.contains("UNION ALL"));
    }

    #[test]
    fn test_union_all_sqlite() {
        let result = compile(
            "from a | select [id] | union all from b | select [id]",
            "sqlite",
        )
        .unwrap();
        assert!(result.sql.contains("UNION ALL"));
    }

    #[test]
    fn test_union_all_mysql() {
        let result = compile(
            "from a | select [id] | union all from b | select [id]",
            "mysql",
        )
        .unwrap();
        assert!(result.sql.contains("UNION ALL"));
    }

    #[test]
    fn test_union_preserves_params_from_both_sides() {
        let result = compile(
            "from t1 | filter x == $a | select [id] | union from t2 | filter y == $b | select [id]",
            "sqlite",
        )
        .unwrap();
        assert!(result.params.contains(&"a".to_string()));
        assert!(result.params.contains(&"b".to_string()));
        // Left params come first
        let pos_a = result.params.iter().position(|p| p == "a").unwrap();
        let pos_b = result.params.iter().position(|p| p == "b").unwrap();
        assert!(pos_a < pos_b);
    }

    #[test]
    fn test_union_with_filters() {
        let result = compile(
            "from users | filter active == true | select [id, name] | union from admins | filter role == 'admin' | select [id, name]",
            "postgres",
        )
        .unwrap();
        // Boolean `true` is inlined, not parameterized; string 'admin' becomes $1
        assert!(result.sql.contains("WHERE (active = true)"));
        assert!(result.sql.contains("WHERE (role = $1)"));
        assert_eq!(result.params, vec!["admin"]);
    }

    // === Subquery: all 4 dialects ===

    #[test]
    fn test_subquery_sqlite() {
        let result = compile(
            "from orders | filter customer_id in (from customers | filter region == 'EU' | select [id])",
            "sqlite",
        )
        .unwrap();
        assert!(result.sql.contains("IN (SELECT id FROM customers"));
        assert!(result.sql.contains("WHERE (region = ?)"));
        assert_eq!(result.params, vec!["EU"]);
    }

    #[test]
    fn test_subquery_duckdb() {
        let result = compile(
            "from orders | filter customer_id in (from customers | filter region == 'EU' | select [id])",
            "duckdb",
        )
        .unwrap();
        assert!(result.sql.contains("IN (SELECT id FROM customers"));
        assert!(result.sql.contains("WHERE (region = ?)"));
    }

    #[test]
    fn test_subquery_mysql() {
        let result = compile(
            "from orders | filter customer_id in (from customers | filter region == 'EU' | select [id])",
            "mysql",
        )
        .unwrap();
        assert!(result.sql.contains("IN (SELECT id FROM customers"));
        assert!(result.sql.contains("WHERE (region = ?)"));
    }

    #[test]
    fn test_multiline_in_subquery() {
        // The docs-web Subquery sample — newlines inside the `in (...)` group
        // (including after the `(`) must not confuse subquery detection.
        let source = "from orders\n| filter customer_id in (\n  from customers\n  | filter region == 'EU'\n  | select [id]\n)";
        for dialect in &["postgres", "sqlite", "duckdb", "mysql"] {
            let result = compile(source, dialect).unwrap();
            let placeholder = if *dialect == "postgres" { "$1" } else { "?" };
            assert!(
                result.sql.contains("IN (SELECT id FROM customers"),
                "dialect {dialect}: {}",
                result.sql
            );
            assert!(result.sql.contains(&format!("WHERE (region = {placeholder})")));
            assert_eq!(result.params, vec!["EU"]);
        }
    }

    #[test]
    fn test_multiline_in_newline_before_paren() {
        // A newline between `in` and `(` is tolerated too.
        let source =
            "from orders\n| filter customer_id in\n(from customers | select [id])";
        let result = compile(source, "postgres").unwrap();
        assert!(result.sql.contains("IN (SELECT id FROM customers)"));
    }

    #[test]
    fn test_multiline_not_in_subquery() {
        let source =
            "from orders\n| filter customer_id not in (\n  from banned\n  | select [id]\n)";
        let result = compile(source, "postgres").unwrap();
        assert!(result.sql.contains("NOT IN (SELECT id FROM banned)"));
    }

    #[test]
    fn test_multiline_literal_list_in_parens() {
        // `in (` + newline + literal list must still parse as a list, not a
        // subquery.
        let source = "from orders\n| filter customer_id in (\n  1, 2, 3\n)";
        let result = compile(source, "sqlite").unwrap();
        assert!(result.sql.contains("(customer_id IN (1, 2, 3))"));
        assert!(result.params.is_empty());
    }

    #[test]
    fn test_not_in_subquery() {
        let result = compile(
            "from orders | filter customer_id not in (from banned | select [id])",
            "postgres",
        )
        .unwrap();
        assert!(result.sql.contains("NOT IN (SELECT id FROM banned)"));
    }

    #[test]
    fn test_subquery_with_multiple_filters() {
        let result = compile(
            "from orders | filter customer_id in (from customers | filter region == 'EU' and status == 'active' | select [id])",
            "postgres",
        )
        .unwrap();
        assert!(result.sql.contains("IN (SELECT id FROM customers"));
        assert!(result
            .sql
            .contains("WHERE ((region = $1) AND (status = $2))"));
        assert_eq!(result.params, vec!["EU", "active"]);
    }

    #[test]
    fn test_subquery_params_deduplicated_postgres() {
        let result = compile(
            "from orders | filter customer_id in (from customers | filter region == $r | select [id]) | filter region == $r",
            "postgres",
        )
        .unwrap();
        // Postgres deduplicates $r to $1
        assert_eq!(result.params, vec!["r"]);
        assert!(result.sql.contains("$1"));
        assert!(!result.sql.contains("$2"));
    }

    #[test]
    fn test_subquery_params_not_deduped_sqlite() {
        let result = compile(
            "from orders | filter customer_id in (from customers | filter region == $r | select [id]) | filter region == $r",
            "sqlite",
        )
        .unwrap();
        // SQLite doesn't dedup — $r appears twice
        assert_eq!(result.params, vec!["r", "r"]);
    }

    #[test]
    fn test_subquery_placeholder_collision_postgres() {
        // Regression: sub-compiler assigned its own $1, $2 but never rewrote them
        // to outer numbering when both inner and outer have distinct params.
        let result = compile(
            "from orders | filter customer_id in (from customers | filter region == $region | select [id]) | filter status == $status",
            "postgres",
        )
        .unwrap();
        // Inner subquery params must be remapped to outer numbering.
        // $region and $status are distinct → outer assigns $1 = region, $2 = status
        // The subquery WHERE should use $1 (region), outer WHERE should use $2 (status).
        assert_eq!(result.params, vec!["region", "status"]);
        assert!(result.sql.contains("$1"), "subquery should use $1: {}", result.sql);
        assert!(result.sql.contains("$2"), "outer filter should use $2: {}", result.sql);
        // Verify no duplicate $1 in wrong places
        let where_parts: Vec<&str> = result.sql.split("WHERE").collect();
        assert_eq!(where_parts.len(), 3, "should have two WHERE clauses: {}", result.sql);
    }

    #[test]
    fn test_subquery_multiple_inner_params_postgres() {
        // Subquery has two distinct params, outer has one — should not collide.
        let result = compile(
            "from orders | filter customer_id in (from customers | filter region == $region and status == $status | select [id]) | filter total > $min_total",
            "postgres",
        )
        .unwrap();
        assert_eq!(result.params, vec!["region", "status", "min_total"]);
        // Subquery uses $1, $2; outer uses $3
        assert!(result.sql.contains("$1"), "region should be $1: {}", result.sql);
        assert!(result.sql.contains("$2"), "status should be $2: {}", result.sql);
        assert!(result.sql.contains("$3"), "min_total should be $3: {}", result.sql);
    }

    // === Edge cases ===

    #[test]
    fn test_upsert_is_mutation() {
        for dialect in &["postgres", "sqlite", "duckdb", "mysql"] {
            let result = compile(
                "into t | upsert [a = $a] | conflict [id] | do update [a = $a]",
                dialect,
            )
            .unwrap();
            assert_eq!(result.statement_type, StatementType::Upsert);
            assert!(result.is_mutation);
        }
    }

    #[test]
    fn test_union_is_not_mutation() {
        for dialect in &["postgres", "sqlite", "duckdb", "mysql"] {
            let result = compile(
                "from t1 | select [id] | union from t2 | select [id]",
                dialect,
            )
            .unwrap();
            assert_eq!(result.statement_type, StatementType::Union);
            assert!(!result.is_mutation);
        }
    }

    #[test]
    fn test_union_statement_type_str() {
        assert_eq!(StatementType::Upsert.as_str(), "upsert");
        assert_eq!(StatementType::Union.as_str(), "union");
    }

    #[test]
    fn test_union_chains_three_statements() {
        let result = compile(
            "from a | select [id] | union from b | select [id] | union from c | select [id]",
            "postgres",
        )
        .unwrap();
        // Should produce: (a UNION b) UNION c
        assert!(result.sql.contains("UNION"));
        assert!(result.sql.contains("FROM a"));
        assert!(result.sql.contains("FROM b"));
        assert!(result.sql.contains("FROM c"));
    }
}
