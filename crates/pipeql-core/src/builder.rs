//! Fluent query builder for PipeQL.
//!
//! Composes a PipeQL **source string** stage by stage, then compiles it through
//! the same [`crate::api`] facade as any hand-written query — so a builder
//! query and a literal string query are provably identical. No dual parser,
//! no semantic drift.
//!
//! Object inserts/updates (`insert`, `update`, `upsert`, `do_update`) accept
//! column → value pairs and auto-generate `$b0`, `$b1`, ... bind parameters,
//! giving every SDK the `$data` ergonomics without a driver.
//!
//! ```
//! use pipeql_core::builder::{Query, Value};
//!
//! let q = Query::from("notes")
//!     .filter("is_archived == 0")
//!     .sort(["created_at desc"])
//!     .take(10);
//!
//! let src = q.source();
//! assert_eq!(src, "from notes | filter is_archived == 0 | sort [created_at desc] | take 10");
//!
//! let compiled = q.compile("postgres").unwrap();
//! assert!(compiled.sql.contains("WHERE (is_archived = 0)"));
//!
//! // Object insert → auto params
//! let ins = Query::into_("notes").insert([("title", Value::Str("Hi".into()))]);
//! assert_eq!(ins.source(), "into notes | insert [title = $b0]");
//! assert_eq!(ins.values(), &[("title".to_string(), Value::Str("Hi".into()))]);
//! ```

use crate::api::{self, CompiledQuery, PipeQLError};

/// A typed value for object inserts/updates. Kept dependency-free so the
/// builder works without `serde`.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Bool(v)
    }
}
impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::Int(v)
    }
}
impl From<i32> for Value {
    fn from(v: i32) -> Self {
        Value::Int(v as i64)
    }
}
impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::Float(v)
    }
}
impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::Str(v.to_string())
    }
}
impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::Str(v)
    }
}

/// Types that can be rendered as a PipeQL bracketed list (`[a, b]`).
pub trait ToList {
    fn to_list(&self) -> String;
}

impl ToList for &str {
    fn to_list(&self) -> String {
        (*self).to_string()
    }
}

impl ToList for String {
    fn to_list(&self) -> String {
        self.clone()
    }
}

impl<const N: usize> ToList for [&str; N] {
    fn to_list(&self) -> String {
        self.join(", ")
    }
}

impl ToList for Vec<&str> {
    fn to_list(&self) -> String {
        self.join(", ")
    }
}

impl ToList for &[&str] {
    fn to_list(&self) -> String {
        self.join(", ")
    }
}

/// A single `col = value` assignment.
pub type Assign<'a> = (&'a str, Value);

/// Fluent PipeQL query builder.
///
/// Every stage method appends to the composed source and returns `self` for
/// chaining. Call [`Query::source`] to get the PipeQL text, [`Query::compile`]
/// to compile it, or [`Query::values`] for object-insert bound values.
#[derive(Debug, Clone, Default)]
pub struct Query {
    source: String,
    values: Vec<(String, Value)>,
}

impl Query {
    /// Start a read pipeline: `from <table>`.
    pub fn from(table: &str) -> Self {
        Query {
            source: format!("from {table}"),
            ..Default::default()
        }
    }

    /// Start an insert/upsert pipeline: `into <table>`.
    pub fn into_(table: &str) -> Self {
        Query {
            source: format!("into {table}"),
            ..Default::default()
        }
    }

    fn stage(mut self, stage: &str) -> Self {
        self.source.push_str(" | ");
        self.source.push_str(stage);
        self
    }

    /// `| filter <expr>`
    pub fn filter(self, expr: &str) -> Self {
        let mut s = self;
        s.source.push_str(" | filter ");
        s.source.push_str(expr);
        s
    }

    /// `| select [<cols>]`
    pub fn select<C: ToList>(self, cols: C) -> Self {
        let mut s = self;
        s.source.push_str(" | select [");
        s.source.push_str(&cols.to_list());
        s.source.push(']');
        s
    }

    /// `| derive [<cols>]`
    pub fn derive<C: ToList>(self, cols: C) -> Self {
        let mut s = self;
        s.source.push_str(" | derive [");
        s.source.push_str(&cols.to_list());
        s.source.push(']');
        s
    }

    /// `| sort [<cols>]`
    pub fn sort<C: ToList>(self, cols: C) -> Self {
        let mut s = self;
        s.source.push_str(" | sort [");
        s.source.push_str(&cols.to_list());
        s.source.push(']');
        s
    }

    /// `| take <n>`
    pub fn take(self, n: u64) -> Self {
        let mut s = self;
        s.source.push_str(" | take ");
        s.source.push_str(&n.to_string());
        s
    }

    /// `| skip <n>`
    pub fn skip(self, n: u64) -> Self {
        let mut s = self;
        s.source.push_str(" | skip ");
        s.source.push_str(&n.to_string());
        s
    }

    /// `| join <table> on <on>`
    pub fn join(self, table: &str, on: &str) -> Self {
        let mut s = self;
        s.source.push_str(" | join ");
        s.source.push_str(table);
        s.source.push_str(" on ");
        s.source.push_str(on);
        s
    }

    /// `| left join <table> on <on>`
    pub fn left_join(self, table: &str, on: &str) -> Self {
        let mut s = self;
        s.source.push_str(" | left join ");
        s.source.push_str(table);
        s.source.push_str(" on ");
        s.source.push_str(on);
        s
    }

    /// `| right join <table> on <on>`
    pub fn right_join(self, table: &str, on: &str) -> Self {
        let mut s = self;
        s.source.push_str(" | right join ");
        s.source.push_str(table);
        s.source.push_str(" on ");
        s.source.push_str(on);
        s
    }

    /// `| full join <table> on <on>`
    pub fn full_join(self, table: &str, on: &str) -> Self {
        let mut s = self;
        s.source.push_str(" | full join ");
        s.source.push_str(table);
        s.source.push_str(" on ");
        s.source.push_str(on);
        s
    }

    /// `| inner join <table> on <on>`
    pub fn inner_join(self, table: &str, on: &str) -> Self {
        let mut s = self;
        s.source.push_str(" | inner join ");
        s.source.push_str(table);
        s.source.push_str(" on ");
        s.source.push_str(on);
        s
    }

    /// `| group [<cols>] (<aggs>)`
    pub fn group<C: ToList>(self, cols: C, aggs: &str) -> Self {
        let mut s = self;
        s.source.push_str(" | group [");
        s.source.push_str(&cols.to_list());
        s.source.push_str("] (");
        s.source.push_str(aggs);
        s.source.push(')');
        s
    }

    /// `| union <other>` where `other` is a raw source string or another query.
    pub fn union(mut self, other: impl IntoSource) -> Self {
        let (other_src, other_vals) = other.into_source();
        self.source.push_str(" | union ");
        self.source.push_str(&other_src);
        self.values.extend(other_vals);
        self
    }

    /// `| union all <other>`
    pub fn union_all(mut self, other: impl IntoSource) -> Self {
        let (other_src, other_vals) = other.into_source();
        self.source.push_str(" | union all ");
        self.source.push_str(&other_src);
        self.values.extend(other_vals);
        self
    }

    /// `| insert [<assignments>]` with auto-generated `$b0, $b1, ...` params.
    pub fn insert<'a, I>(mut self, assigns: I) -> Self
    where
        I: IntoIterator<Item = Assign<'a>>,
    {
        self.push_assignments("insert", assigns);
        self
    }

    /// `| update [<assignments>]` (requires a preceding filter stage).
    pub fn update<'a, I>(mut self, assigns: I) -> Self
    where
        I: IntoIterator<Item = Assign<'a>>,
    {
        self.push_assignments("update", assigns);
        self
    }

    /// `| update all [<assignments>]` — explicit opt-in for a full-table
    /// update that bypasses the filter guard.
    pub fn update_all<'a, I>(mut self, assigns: I) -> Self
    where
        I: IntoIterator<Item = Assign<'a>>,
    {
        self.push_assignments("update all", assigns);
        self
    }

    /// `| delete`
    pub fn delete(self) -> Self {
        self.stage("delete")
    }

    /// `| delete all` — explicit opt-in for a full-table delete that bypasses
    /// the filter guard.
    pub fn delete_all(self) -> Self {
        self.stage("delete all")
    }

    /// `| upsert [<assignments>]`
    pub fn upsert<'a, I>(mut self, assigns: I) -> Self
    where
        I: IntoIterator<Item = Assign<'a>>,
    {
        self.push_assignments("upsert", assigns);
        self
    }

    /// `| conflict [<cols>]`
    pub fn conflict<C: ToList>(self, cols: C) -> Self {
        self.stage(&format!("conflict [{}]", cols.to_list()))
    }

    /// `| do update [<assignments>]`
    pub fn do_update<'a, I>(mut self, assigns: I) -> Self
    where
        I: IntoIterator<Item = Assign<'a>>,
    {
        self.push_assignments("do update", assigns);
        self
    }

    /// Append an explicit stage string (for stages without a dedicated method).
    pub fn raw_stage(self, stage: &str) -> Self {
        self.stage(stage)
    }

    fn push_assignments<'a, I>(&mut self, kind: &str, assigns: I)
    where
        I: IntoIterator<Item = Assign<'a>>,
    {
        self.source.push_str(" | ");
        self.source.push_str(kind);
        self.source.push_str(" [");
        let mut first = true;
        for (col, val) in assigns {
            if !first {
                self.source.push_str(", ");
            }
            first = false;
            let idx = self.values.len();
            self.source.push_str(col);
            self.source.push_str(" = $b");
            self.source.push_str(&idx.to_string());
            self.values.push((col.to_string(), val));
        }
        self.source.push(']');
    }

    /// The composed PipeQL source string.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Bound values from object inserts/updates, in `$bN` order.
    pub fn values(&self) -> &[(String, Value)] {
        &self.values
    }

    /// Compile the composed source through the standard facade.
    pub fn compile(&self, dialect: &str) -> Result<CompiledQuery, PipeQLError> {
        api::compile(&self.source, dialect)
    }
}

/// Something that can be used as a union operand: a raw source string or a
/// builder query.
pub trait IntoSource {
    fn into_source(self) -> (String, Vec<(String, Value)>);
}

impl IntoSource for String {
    fn into_source(self) -> (String, Vec<(String, Value)>) {
        (self, Vec::new())
    }
}

impl IntoSource for &str {
    fn into_source(self) -> (String, Vec<(String, Value)>) {
        (self.to_string(), Vec::new())
    }
}

impl IntoSource for Query {
    fn into_source(self) -> (String, Vec<(String, Value)>) {
        (self.source, self.values)
    }
}

impl IntoSource for &Query {
    fn into_source(self) -> (String, Vec<(String, Value)>) {
        (self.source.clone(), self.values.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_pipeline_source() {
        let q = Query::from("notes")
            .filter("is_archived == 0")
            .sort(["created_at desc"])
            .take(10);
        assert_eq!(
            q.source(),
            "from notes | filter is_archived == 0 | sort [created_at desc] | take 10"
        );
    }

    #[test]
    fn test_read_pipeline_compiles() {
        let q = Query::from("notes")
            .filter("is_archived == 0")
            .sort(["created_at desc"])
            .take(10);
        let compiled = q.compile("postgres").unwrap();
        assert!(compiled.sql.contains("SELECT * FROM notes"));
        assert!(compiled.sql.contains("WHERE (is_archived = 0)"));
        assert!(compiled.sql.contains("ORDER BY created_at DESC"));
        assert!(compiled.sql.contains("LIMIT 10"));
    }

    #[test]
    fn test_joins() {
        let q = Query::from("a").join("b", "a.id == b.a_id");
        assert_eq!(q.source(), "from a | join b on a.id == b.a_id");
        let q = Query::from("a").left_join("b", "a.id == b.a_id");
        assert_eq!(q.source(), "from a | left join b on a.id == b.a_id");
        let q = Query::from("a").full_join("b", "a.id == b.a_id");
        assert_eq!(q.source(), "from a | full join b on a.id == b.a_id");
    }

    #[test]
    fn test_select_string_and_list() {
        let q = Query::from("t").select("id, name");
        assert_eq!(q.source(), "from t | select [id, name]");
        let q = Query::from("t").select(["id", "name"]);
        assert_eq!(q.source(), "from t | select [id, name]");
        let q = Query::from("t").select(vec!["id", "name"]);
        assert_eq!(q.source(), "from t | select [id, name]");
    }

    #[test]
    fn test_group() {
        let q = Query::from("orders")
            .group(["region"], "total = sum(amt), n = count(*)")
            .filter("total > 100");
        assert_eq!(
            q.source(),
            "from orders | group [region] (total = sum(amt), n = count(*)) | filter total > 100"
        );
    }

    #[test]
    fn test_union_accepts_query_and_string() {
        let other = Query::from("archived").select(["id"]);
        let q = Query::from("active")
            .select(["id"])
            .union(other);
        assert_eq!(q.source(), "from active | select [id] | union from archived | select [id]");
        let q2 = Query::from("a").select(["id"]).union("from b | select [id]");
        assert_eq!(q2.source(), "from a | select [id] | union from b | select [id]");
    }

    #[test]
    fn test_object_insert_generates_params_and_values() {
        let q = Query::into_("notes")
            .insert([("title", Value::Str("Hi".into())), ("flag", Value::Int(1))]);
        assert_eq!(q.source(), "into notes | insert [title = $b0, flag = $b1]");
        assert_eq!(
            q.values(),
            &[
                ("title".to_string(), Value::Str("Hi".to_string())),
                ("flag".to_string(), Value::Int(1)),
            ]
        );
        let compiled = q.compile("sqlite").unwrap();
        assert_eq!(compiled.params, vec!["b0", "b1"]);
        assert!(compiled.sql.contains("INSERT INTO notes (title, flag) VALUES (?, ?)"));
    }

    #[test]
    fn test_from_string_literals_into_values() {
        let q = Query::into_("t")
            .insert([("s", "hi".into()), ("n", 42.into()), ("b", true.into()), ("z", Value::Null)]);
        assert_eq!(
            q.source(),
            "into t | insert [s = $b0, n = $b1, b = $b2, z = $b3]"
        );
    }

    #[test]
    fn test_update_and_delete() {
        let q = Query::from("notes").filter("id == $id").update([("title", "new".into())]);
        assert_eq!(q.source(), "from notes | filter id == $id | update [title = $b0]");
        let d = Query::from("notes").filter("id == $id").delete();
        assert_eq!(d.source(), "from notes | filter id == $id | delete");
    }

    #[test]
    fn test_update_delete_all_escape_hatch() {
        let q = Query::from("notes").delete_all();
        assert_eq!(q.source(), "from notes | delete all");
        let q = Query::from("notes").update_all([("title", "new".into())]);
        assert_eq!(q.source(), "from notes | update all [title = $b0]");
        // Compiles without a filter (explicit opt-in).
        let compiled = Query::from("notes")
            .delete_all()
            .compile("sqlite")
            .unwrap();
        assert_eq!(compiled.sql, "DELETE FROM notes;");
        let compiled = Query::from("notes")
            .update_all([("title", "new".into())])
            .compile("postgres")
            .unwrap();
        assert!(compiled.sql.contains("UPDATE notes"));
        assert!(!compiled.sql.contains("WHERE"));
    }

    #[test]
    fn test_upsert_chain() {
        let q = Query::into_("users")
            .upsert([("id", 1.into()), ("name", "Ann".into())])
            .conflict(["id"])
            .do_update([("name", "Ann".into())]);
        assert_eq!(
            q.source(),
            "into users | upsert [id = $b0, name = $b1] | conflict [id] | do update [name = $b2]"
        );
        let compiled = q.compile("postgres").unwrap();
        assert_eq!(compiled.statement_type, api::StatementType::Upsert);
        // b0/b1/b2 are distinct names, so postgres uses $1/$2/$3
        assert!(compiled.sql.contains("ON CONFLICT (id) DO UPDATE SET name = $3"));
    }

    #[test]
    fn test_compile_error_surfaces() {
        // unfiltered delete → analysis rejects missing filter guard
        let err = Query::from("notes").delete().compile("postgres").unwrap_err();
        assert!(matches!(err, PipeQLError::Analysis(_)));
        // take before delete (with a filter) → codegen rejects non-filter
        // steps before the mutation
        let err = Query::from("notes")
            .filter("a == 1")
            .take(5)
            .delete()
            .compile("postgres")
            .unwrap_err();
        assert!(matches!(err, PipeQLError::Codegen(_)));
        // unsupported dialect
        let err2 = Query::from("t").compile("oracle").unwrap_err();
        assert!(matches!(err2, PipeQLError::Codegen(_)));
    }

    #[test]
    fn test_derive_and_skip() {
        let q = Query::from("orders")
            .derive(["total = price * qty"])
            .filter("total > $min")
            .skip(5);
        assert_eq!(
            q.source(),
            "from orders | derive [total = price * qty] | filter total > $min | skip 5"
        );
    }

    #[test]
    fn test_raw_stage() {
        let q = Query::from("t").raw_stage("filter active == true");
        assert_eq!(q.source(), "from t | filter active == true");
    }
}
