use std::collections::HashMap;

use crate::ast::*;

/// Value types PipeQL can statically reason about. These are the surface types
/// exposed to callers; deeper relational types are left to the target DB.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueType {
    Integer,
    Float,
    String,
    Bool,
    Null,
    Timestamp,
    /// Any (default when no type information is available).
    Any,
}

impl ValueType {
    fn from_literal(lit: &Literal) -> ValueType {
        match lit {
            Literal::Integer(_) => ValueType::Integer,
            Literal::Float(_) => ValueType::Float,
            Literal::String(_) => ValueType::String,
            Literal::Bool(_) => ValueType::Bool,
            Literal::Null => ValueType::Null,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ValueType::Integer => "integer",
            ValueType::Float => "float",
            ValueType::String => "string",
            ValueType::Bool => "bool",
            ValueType::Null => "null",
            ValueType::Timestamp => "timestamp",
            ValueType::Any => "any",
        }
    }
}

/// The parameter name a literal binds to in strict (mutation) mode.
/// `NULL` returns `None` because it is never parameterized — it stays
/// inline in the emitted SQL.
fn literal_param_name(lit: &Literal) -> Option<String> {
    match lit {
        Literal::String(v) => Some(v.clone()),
        Literal::Integer(v) => Some(v.to_string()),
        Literal::Float(v) => Some(v.to_string()),
        Literal::Bool(v) => Some(v.to_string()),
        Literal::Null => None,
    }
}

impl std::fmt::Display for ValueType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A column in a schema catalog, with an optional type.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnMeta {
    pub name: String,
    pub ty: ValueType,
}

/// A table in the schema catalog used for column/scope validation.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct TableMeta {
    pub name: String,
    pub columns: Vec<ColumnMeta>,
}

/// The schema catalog: maps table name to its metadata. This is optional input
/// to the analyzer; when absent, column validation is skipped but the parameter
/// map and type inference still run.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    tables: HashMap<String, TableMeta>,
}

impl Catalog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a table in the catalog.
    pub fn add_table(&mut self, table: TableMeta) -> &mut Self {
        self.tables.insert(table.name.clone(), table);
        self
    }

    pub fn table(&self, name: &str) -> Option<&TableMeta> {
        self.tables.get(name)
    }

    pub fn tables(&self) -> impl Iterator<Item = &TableMeta> {
        self.tables.values()
    }

    pub fn has_column(&self, table: &str, column: &str) -> bool {
        self.tables
            .get(table)
            .map(|t| t.columns.iter().any(|c| c.name == column))
            .unwrap_or(false)
    }
}

/// A single extracted parameter. `occurrences` records every source position
/// (byte offset) where the parameter appears, so tooling can highlight each use.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct ParamMeta {
    pub name: String,
    pub ty: ValueType,
    pub occurrences: Vec<usize>,
}

/// An error produced by semantic analysis (scope/column validation).
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct AnalyzerError {
    pub message: String,
    pub span: Span,
    pub suggestion: Option<String>,
}

/// The result of semantic analysis. The parameter map is the isolated "Parameter
/// Map" required by the PRD: it collects every dynamic value separately from the
/// SQL text, which is what makes the emitted SQL injection-safe by construction.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Analysis {
    /// Parameter name -> metadata. Insertion order is preserved for stable
    /// placeholder numbering.
    pub param_map: Vec<ParamMeta>,
    /// Name lookup index over `param_map`.
    param_index: HashMap<String, usize>,
    /// Whether a schema catalog was supplied (affects whether columns were
    /// validated).
    pub validated_columns: bool,
}

impl Analysis {
    fn param(&mut self, name: &str, ty: ValueType, span: Span) {
        if let Some(&idx) = self.param_index.get(name) {
            self.param_map[idx].occurrences.push(span.start);
            return;
        }
        let idx = self.param_map.len();
        self.param_index.insert(name.to_string(), idx);
        self.param_map.push(ParamMeta {
            name: name.to_string(),
            ty,
            occurrences: vec![span.start],
        });
    }

    /// The ordered parameter names, used to drive placeholder numbering.
    pub fn param_names(&self) -> Vec<&str> {
        self.param_map.iter().map(|p| p.name.as_str()).collect()
    }

    /// The ordered list of (name, type) pairs.
    pub fn params(&self) -> Vec<(&str, ValueType)> {
        self.param_map
            .iter()
            .map(|p| (p.name.as_str(), p.ty))
            .collect()
    }

    /// Number of unique parameters.
    pub fn param_count(&self) -> usize {
        self.param_map.len()
    }

    /// Merge another Analysis into this one, deduplicating parameters by name.
    pub fn merge(&mut self, other: &Analysis) {
        for p in &other.param_map {
            if let Some(&idx) = self.param_index.get(&p.name) {
                self.param_map[idx].occurrences.extend(&p.occurrences);
            } else {
                let idx = self.param_map.len();
                self.param_index.insert(p.name.clone(), idx);
                self.param_map.push(ParamMeta {
                    name: p.name.clone(),
                    ty: p.ty,
                    occurrences: p.occurrences.clone(),
                });
            }
        }
        if other.validated_columns {
            self.validated_columns = true;
        }
    }
}

/// Semantic analyzer: builds the isolated Parameter Map, infers expression
/// types, and validates column references against an optional catalog.
pub struct Analyzer<'a> {
    catalog: Option<&'a Catalog>,
    /// In mutation statements (insert/update/delete) every literal value is
    /// extracted into the parameter map so no raw value ever lands in the SQL.
    strict_literals: bool,
    /// Span of the step currently being analyzed, used as the occurrence span
    /// for literal parameters (which have no span of their own in the AST).
    step_span: Span,
}

impl<'a> Analyzer<'a> {
    pub fn new(catalog: Option<&'a Catalog>) -> Self {
        Self {
            catalog,
            strict_literals: false,
            step_span: Span::new(0, 0),
        }
    }

    /// Analyze a parsed statement. Returns the Analysis with collected
    /// parameters.
    pub fn analyze_statement(&mut self, stmt: &Statement) -> Result<Analysis, Vec<AnalyzerError>> {
        match stmt {
            Statement::Pipeline(pipeline) => self.analyze(pipeline),
            Statement::Insert(insert) => {
                self.strict_literals = true;
                self.step_span = insert.span;
                let mut analysis = Analysis::default();
                let mut errors = Vec::new();
                for a in &insert.assignments {
                    self.infer_expr(&mut analysis, &mut errors, &[], &[], &a.expr);
                }
                if errors.is_empty() {
                    Ok(analysis)
                } else {
                    Err(errors)
                }
            }
            Statement::Upsert(upsert) => {
                self.strict_literals = true;
                self.step_span = upsert.span;
                let mut analysis = Analysis::default();
                let mut errors = Vec::new();
                for a in &upsert.assignments {
                    self.infer_expr(&mut analysis, &mut errors, &[], &[], &a.expr);
                }
                for a in &upsert.do_update {
                    self.infer_expr(&mut analysis, &mut errors, &[], &[], &a.expr);
                }
                if errors.is_empty() {
                    Ok(analysis)
                } else {
                    Err(errors)
                }
            }
            Statement::Union(union) => {
                let mut left = self.analyze_statement(&union.left)?;
                let right = self.analyze_statement(&union.right)?;
                left.merge(&right);
                Ok(left)
            }
            Statement::CreateTable(_) => Ok(Analysis::default()),
        }
    }

    /// Analyze a parsed pipeline. Returns the Analysis with collected parameters.
    pub fn analyze(&mut self, pipeline: &Pipeline) -> Result<Analysis, Vec<AnalyzerError>> {
        let mut analysis = Analysis::default();
        let mut errors = Vec::new();

        // Mutation pipelines parameterize every literal; read pipelines only
        // parameterize string literals and `$params`.
        self.strict_literals = pipeline
            .steps
            .iter()
            .any(|s| matches!(s, PipelineStep::Update { .. } | PipelineStep::Delete { .. }));

        // Build the visible column scope as steps accumulate derived columns.
        // Table aliases come from the source + joins.
        let mut tables: Vec<(&Ident, &Ident)> = Vec::new(); // (real_name, alias_or_name)
        tables.push((
            &pipeline.source.name,
            pipeline
                .source
                .alias
                .as_ref()
                .unwrap_or(&pipeline.source.name),
        ));
        let mut scope: Vec<&Ident> = Vec::new();
        // For mutation pipelines the SQL binds SET values before WHERE values,
        // so filter expressions are deferred until after the update
        // assignments have been inferred.
        let mut filter_exprs: Vec<(&Expr, Span)> = Vec::new();
        let mut update_assignments: Option<&[Assignment]> = None;
        // Enforce the documented safety rule: `update`/`delete` must be
        // guarded by at least one preceding `filter` step so a typo can never
        // turn into an accidental mass UPDATE/DELETE.
        let mut saw_filter = false;
        let mut saw_take = false;
        let mut saw_skip = false;

        for step in &pipeline.steps {
            match step {
                PipelineStep::Filter { expr, span, .. } => {
                    self.step_span = *span;
                    saw_filter = true;
                    if self.strict_literals {
                        filter_exprs.push((expr, *span));
                    } else {
                        self.infer_expr(&mut analysis, &mut errors, &tables, &scope, expr);
                    }
                }
                PipelineStep::Update {
                    assignments,
                    all,
                    span,
                    ..
                } => {
                    self.step_span = *span;
                    // `update all` is the explicit opt-in full-table form.
                    if !saw_filter && !*all {
                        errors.push(AnalyzerError {
                            message: "'update' requires a preceding 'filter' stage".to_string(),
                            span: *span,
                            suggestion: Some(
                                "Add a filter to prevent accidental mass updates: \
                                 from <table> | filter ... | update [...] \
                                 (or write `update all [...]` to explicitly opt in)"
                                    .to_string(),
                            ),
                        });
                    }
                    update_assignments = Some(assignments);
                }
                PipelineStep::Delete { all, span, .. } => {
                    self.step_span = *span;
                    // `delete all` is the explicit opt-in full-table form.
                    if !saw_filter && !*all {
                        errors.push(AnalyzerError {
                            message: "'delete' requires a preceding 'filter' stage".to_string(),
                            span: *span,
                            suggestion: Some(
                                "Add a filter to prevent accidental mass deletes: \
                                 from <table> | filter ... | delete \
                                 (or write `delete all` to explicitly opt in)"
                                    .to_string(),
                            ),
                        });
                    }
                }
                PipelineStep::Select { columns, span, .. } => {
                    self.step_span = *span;
                    for item in columns {
                        self.infer_expr(&mut analysis, &mut errors, &tables, &scope, &item.expr);
                    }
                }
                PipelineStep::Derive {
                    assignments, span, ..
                } => {
                    self.step_span = *span;
                    for a in assignments {
                        self.infer_expr(&mut analysis, &mut errors, &tables, &scope, &a.expr);
                        scope.push(&a.name);
                    }
                }
                PipelineStep::Join {
                    table,
                    alias,
                    on,
                    span,
                    ..
                } => {
                    self.step_span = *span;
                    tables.push((table, alias.as_ref().unwrap_or(table)));
                    self.infer_expr(&mut analysis, &mut errors, &tables, &scope, on);
                }
                PipelineStep::Group {
                    columns,
                    aggregates,
                    span,
                    ..
                } => {
                    self.step_span = *span;
                    for col in columns {
                        self.infer_expr(&mut analysis, &mut errors, &tables, &scope, col);
                    }
                    for agg in aggregates {
                        for arg in &agg.args {
                            self.infer_expr(&mut analysis, &mut errors, &tables, &scope, arg);
                        }
                    }
                }
                PipelineStep::Sort { items, span, .. } => {
                    self.step_span = *span;
                    for item in items {
                        self.infer_expr(&mut analysis, &mut errors, &tables, &scope, &item.expr);
                    }
                }
                PipelineStep::Take { span, .. } => {
                    self.step_span = *span;
                    if saw_take {
                        errors.push(AnalyzerError {
                            message: "duplicate 'take' stage; only the last one takes effect"
                                .to_string(),
                            span: *span,
                            suggestion: Some(
                                "Remove the earlier 'take' or combine into a single 'take N' stage"
                                    .to_string(),
                            ),
                        });
                    }
                    saw_take = true;
                }
                PipelineStep::Skip { span, .. } => {
                    self.step_span = *span;
                    if saw_skip {
                        errors.push(AnalyzerError {
                            message: "duplicate 'skip' stage; only the last one takes effect"
                                .to_string(),
                            span: *span,
                            suggestion: Some(
                                "Remove the earlier 'skip' or combine into a single 'skip N' stage"
                                    .to_string(),
                            ),
                        });
                    }
                    saw_skip = true;
                }
            }
        }

        if let Some(assignments) = update_assignments {
            for a in assignments {
                self.infer_expr(&mut analysis, &mut errors, &tables, &scope, &a.expr);
            }
        }
        for (expr, span) in filter_exprs {
            self.step_span = span;
            self.infer_expr(&mut analysis, &mut errors, &tables, &scope, expr);
        }

        if errors.is_empty() {
            Ok(analysis)
        } else {
            Err(errors)
        }
    }

    /// Walk an expression, collecting parameters and (optionally) validating
    /// column references against the schema catalog.
    fn infer_expr(
        &self,
        analysis: &mut Analysis,
        errors: &mut Vec<AnalyzerError>,
        tables: &[(&Ident, &Ident)],
        scope: &[&Ident],
        expr: &Expr,
    ) -> ValueType {
        match expr {
            Expr::Star => ValueType::Any,
            Expr::Literal(lit) => {
                // Mutation statements parameterize every literal value so the
                // SQL text never contains raw data. The parameter name is the
                // raw value, matching the codegen placeholder value exactly.
                // NULL is never parameterized — it stays inline in the SQL.
                if self.strict_literals {
                    if let Some(name) = literal_param_name(lit) {
                        analysis.param(&name, ValueType::from_literal(lit), self.step_span);
                    }
                }
                ValueType::from_literal(lit)
            }
            Expr::Parameter(p) => {
                analysis.param(&p.name, ValueType::Any, p.span);
                ValueType::Any
            }
            Expr::UnaryOp { expr, .. } => self.infer_expr(analysis, errors, tables, scope, expr),
            Expr::IsNull { expr, .. } => {
                self.infer_expr(analysis, errors, tables, scope, expr);
                ValueType::Bool
            }
            Expr::InList { expr, list, .. } => {
                self.infer_expr(analysis, errors, tables, scope, expr);
                for item in list {
                    self.infer_expr(analysis, errors, tables, scope, item);
                }
                ValueType::Bool
            }
            Expr::InSubquery { expr, subquery, .. } => {
                self.infer_expr(analysis, errors, tables, scope, expr);
                // Analyze the subquery with its own table scope, derived columns, and catalog checks
                let mut sub_analyzer = Analyzer::new(self.catalog);
                sub_analyzer.strict_literals = self.strict_literals;
                match sub_analyzer.analyze(subquery) {
                    Ok(sub_analysis) => {
                        analysis.merge(&sub_analysis);
                    }
                    Err(sub_errors) => {
                        errors.extend(sub_errors);
                    }
                }
                ValueType::Bool
            }
            Expr::BinaryOp { left, right, .. } => {
                self.infer_expr(analysis, errors, tables, scope, left);
                self.infer_expr(analysis, errors, tables, scope, right);
                ValueType::Bool
            }
            Expr::FunctionCall { args, .. } => {
                for a in args {
                    self.infer_expr(analysis, errors, tables, scope, a);
                }
                ValueType::Any
            }
            Expr::Ident(id) => {
                self.validate_column(errors, tables, scope, id);
                ValueType::Any
            }
            Expr::ColumnRef { table, column, .. } => {
                let table_name = table.as_ref().map(|t| t.name.as_str());
                if let Some(t) = table_name {
                    // Resolve the alias to its real table name for catalog lookup.
                    let real = tables.iter().find(|(_, a)| a.name == t);
                    if let Some((real_name, _)) = real {
                        if let Some(cat) = self.catalog {
                            if !cat.has_column(&real_name.name, &column.name) {
                                errors.push(AnalyzerError {
                                    message: format!("Unknown column '{t}.{}'", column.name),
                                    span: column.span,
                                    suggestion: None,
                                });
                            }
                        }
                    }
                    // If the first segment is not a known alias, it is treated as
                    // an unqualified or JSON-path access and left to the database.
                }
                ValueType::Any
            }
        }
    }

    /// Validate a bare identifier against the current scope and catalog.
    fn validate_column(
        &self,
        errors: &mut Vec<AnalyzerError>,
        tables: &[(&Ident, &Ident)],
        scope: &[&Ident],
        id: &Ident,
    ) {
        if let Some(cat) = self.catalog {
            // A bare column must belong to one of the visible tables.
            let in_scope = scope.iter().any(|s| s.name == id.name);
            let in_tables = tables.iter().any(|(real, alias)| {
                // Check against both the real table name and the alias
                cat.has_column(&real.name, &id.name)
                    || cat.has_column(&alias.name, &id.name)
            });
            if !in_scope && !in_tables {
                errors.push(AnalyzerError {
                    message: format!("Unknown column '{}'", id.name),
                    span: id.span,
                    suggestion: None,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;

    fn analyze(src: &str, catalog: Option<&Catalog>) -> Result<Analysis, Vec<AnalyzerError>> {
        let mut parser = Parser::new(src).expect("lexer should not fail");
        let pipeline = parser.parse_pipeline().expect("parser should not fail");
        let mut analyzer = Analyzer::new(catalog);
        analyzer.analyze(&pipeline)
    }

    #[test]
    fn test_parameter_map_collection_and_dedup() {
        let analysis = analyze(
            "from users | filter id == $user_id and name == $user_id | select [age]",
            None,
        )
        .unwrap();
        assert_eq!(analysis.param_count(), 1);
        assert_eq!(analysis.param_names(), vec!["user_id"]);
        assert_eq!(analysis.param_map[0].occurrences.len(), 2);
    }

    #[test]
    fn test_parameter_types() {
        let analysis = analyze("from t | filter a == $x", None).unwrap();
        assert_eq!(analysis.param_map[0].ty, ValueType::Any);
    }

    #[test]
    fn test_unknown_column_without_catalog_is_permissive() {
        let analysis = analyze("from t | filter nope > 1", None).unwrap();
        assert!(analysis.param_map.is_empty());
    }

    #[test]
    fn test_unknown_column_with_catalog_is_rejected() {
        let mut catalog = Catalog::new();
        catalog.add_table(TableMeta {
            name: "users".into(),
            columns: vec![ColumnMeta {
                name: "id".into(),
                ty: ValueType::Integer,
            }],
        });
        let err = analyze("from users | filter nope > 1", Some(&catalog)).unwrap_err();
        assert!(err.iter().any(|e| e.message.contains("nope")));
    }

    #[test]
    fn test_derived_column_is_in_scope() {
        let mut catalog = Catalog::new();
        catalog.add_table(TableMeta {
            name: "users".into(),
            columns: vec![
                ColumnMeta {
                    name: "age".into(),
                    ty: ValueType::Integer,
                },
                ColumnMeta {
                    name: "id".into(),
                    ty: ValueType::Integer,
                },
            ],
        });
        let analysis = analyze(
            "from users | derive [double = age * 2] | select [id, double]",
            Some(&catalog),
        )
        .unwrap();
        assert!(analysis.param_map.is_empty());
    }

    #[test]
    fn test_column_validated_against_joined_tables() {
        let mut catalog = Catalog::new();
        catalog.add_table(TableMeta {
            name: "a".into(),
            columns: vec![ColumnMeta {
                name: "id".into(),
                ty: ValueType::Integer,
            }],
        });
        catalog.add_table(TableMeta {
            name: "b".into(),
            columns: vec![
                ColumnMeta {
                    name: "id".into(),
                    ty: ValueType::Integer,
                },
                ColumnMeta {
                    name: "name".into(),
                    ty: ValueType::String,
                },
            ],
        });
        let analysis = analyze(
            "from a | join b on a.id == b.id | select [b.name]",
            Some(&catalog),
        )
        .unwrap();
        assert!(analysis.param_map.is_empty());
    }

    #[test]
    fn test_json_path_skips_column_check() {
        let mut catalog = Catalog::new();
        catalog.add_table(TableMeta {
            name: "users".into(),
            columns: vec![ColumnMeta {
                name: "profile".into(),
                ty: ValueType::Any,
            }],
        });
        // JSON sub-paths (profile.name) are not validated against the catalog.
        let analysis = analyze(
            "from users | select [profile.name as author]",
            Some(&catalog),
        )
        .unwrap();
        assert!(analysis.param_map.is_empty());
    }

    fn analyze_statement(
        src: &str,
        catalog: Option<&Catalog>,
    ) -> Result<Analysis, Vec<AnalyzerError>> {
        let mut parser = Parser::new(src).expect("lexer should not fail");
        let stmt = parser.parse_statement().expect("parser should not fail");
        let mut analyzer = Analyzer::new(catalog);
        analyzer.analyze_statement(&stmt)
    }

    #[test]
    fn test_insert_extracts_all_literals_as_params() {
        let analysis = analyze_statement(
            "into notes | insert [title = $title, category = 'Personal', is_pinned = 0]",
            None,
        )
        .unwrap();
        // $title plus every literal value.
        assert_eq!(analysis.param_names(), vec!["title", "Personal", "0"]);
    }

    #[test]
    fn test_update_extracts_set_and_where_literals() {
        let analysis = analyze_statement(
            "from notes | filter id == $id and is_archived == 0 | update [title = $title, is_pinned = 1]",
            None,
        )
        .unwrap();
        // SET params first, then WHERE params.
        assert_eq!(analysis.param_names(), vec!["title", "1", "id", "0"]);
    }

    #[test]
    fn test_delete_extracts_filter_literals() {
        let analysis = analyze_statement(
            "from notes | filter id == $id or is_archived == 1 | delete",
            None,
        )
        .unwrap();
        assert_eq!(analysis.param_names(), vec!["id", "1"]);
    }

    #[test]
    fn test_create_table_has_no_params() {
        let analysis = analyze_statement("table notes [id int primary auto]", None).unwrap();
        assert!(analysis.param_map.is_empty());
    }

    #[test]
    fn test_update_delete_require_preceding_filter() {
        // Documented safety rule: unfiltered mutations are rejected.
        let err = analyze_statement("from users | update [name = $n]", None).unwrap_err();
        assert!(err
            .iter()
            .any(|e| e.message.contains("requires a preceding 'filter' stage")));

        let err = analyze_statement("from users | delete", None).unwrap_err();
        assert!(err
            .iter()
            .any(|e| e.message.contains("requires a preceding 'filter' stage")));

        // A filter before the mutation makes it valid.
        assert!(
            analyze_statement("from users | filter id == $id | update [name = $n]", None).is_ok()
        );
        assert!(analyze_statement("from users | filter id == $id | delete", None).is_ok());
        // Multiple filters also satisfy the guard.
        assert!(
            analyze_statement(
                "from users | filter a == 1 | filter b == 2 | update [name = $n]",
                None
            )
            .is_ok()
        );
    }

    #[test]
    fn test_update_delete_all_escape_hatch() {
        // `delete all` / `update all [...]` explicitly opt in to full-table
        // operations and bypass the filter guard.
        assert!(analyze_statement("from users | delete all", None).is_ok());
        assert!(
            analyze_statement("from users | update all [name = $n]", None).is_ok()
        );
        // A filter combined with `all` is still fine (filter simply applies).
        assert!(analyze_statement("from users | filter a == 1 | delete all", None).is_ok());
    }

    #[test]
    fn test_filter_after_mutation_still_rejected() {
        // A filter that appears only after the mutation does not satisfy the
        // guard (the pipeline is also rejected at codegen for step ordering).
        let err = analyze_statement("from users | update [name = $n] | filter a == 1", None)
            .unwrap_err();
        assert!(err
            .iter()
            .any(|e| e.message.contains("requires a preceding 'filter' stage")));
    }
}
