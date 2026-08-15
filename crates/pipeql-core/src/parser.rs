use std::fmt;

use crate::ast::*;
use crate::lexer::{Lexer, Token, TokenKind};

#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
    /// Optional actionable suggestion for the user, e.g. "Did you mean `==`?"
    pub suggestion: Option<String>,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Parse error at {}..{}: {}",
            self.span.start, self.span.end, self.message
        )?;
        if let Some(s) = &self.suggestion {
            write!(f, "\n  hint: {s}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ParseError {}

fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_len = a.len();
    let b_len = b.len();
    if a_len == 0 { return b_len; }
    if b_len == 0 { return a_len; }

    // Rolling buffer: only keep two rows at a time
    let mut prev = vec![0usize; b_len + 1];
    let mut curr = vec![0usize; b_len + 1];

    for (j, slot) in prev.iter_mut().enumerate().take(b_len + 1) {
        *slot = j;
    }

    for (i, a_ch) in a.chars().enumerate() {
        curr[0] = i + 1;
        for (j, b_ch) in b.chars().enumerate() {
            let cost = if a_ch == b_ch { 0 } else { 1 };
            curr[j + 1] = (prev[j + 1] + 1)
                .min(curr[j] + 1)
                .min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b_len]
}

pub fn suggest_keyword(input: &str) -> Option<&'static str> {
    let candidates = [
        "from", "into", "table", "insert", "upsert", "update", "delete",
        "filter", "select", "derive", "join", "group", "sort", "take", "skip",
        "conflict", "do", "union", "all", "left", "right", "full", "inner", "as", "on",
        "and", "or", "not", "in", "is", "null", "true", "false", "asc", "desc",
    ];
    let mut best: Option<(&'static str, usize)> = None;

    for &cand in &candidates {
        // Case-insensitive Levenshtein: compare against lowercased candidate
        let dist = if input.len() <= 16 && cand.len() <= 16 {
            // Short strings: direct case-insensitive char comparison
            levenshtein_distance_ci(input, cand)
        } else {
            levenshtein_distance(&input.to_lowercase(), cand)
        };
        if dist <= 2 && dist < cand.len() {
            if let Some((best_cand, best_dist)) = best {
                if dist < best_dist {
                    best = Some((cand, dist));
                } else if dist == best_dist {
                    // Tie breaker: prefer candidate sharing the same starting character
                    if cand.bytes().next().eq(&input.bytes().next().map(|b| b.to_ascii_lowercase()))
                        && best_cand.bytes().next() != input.bytes().next().map(|b| b.to_ascii_lowercase())
                    {
                        best = Some((cand, dist));
                    }
                }
            } else {
                best = Some((cand, dist));
            }
        }
    }
    best.map(|(c, _)| c)
}

/// Case-insensitive Levenshtein for short strings (avoids allocation).
fn levenshtein_distance_ci(a: &str, b: &str) -> usize {
    let b_len = b.len();
    if a.is_empty() { return b_len; }
    if b_len == 0 { return a.len(); }

    let mut prev = vec![0usize; b_len + 1];
    let mut curr = vec![0usize; b_len + 1];

    for (j, slot) in prev.iter_mut().enumerate().take(b_len + 1) {
        *slot = j;
    }

    for (i, a_ch) in a.bytes().enumerate() {
        curr[0] = i + 1;
        let a_lower = a_ch.to_ascii_lowercase();
        for (j, b_ch) in b.bytes().enumerate() {
            let cost = if a_lower == b_ch.to_ascii_lowercase() { 0 } else { 1 };
            curr[j + 1] = (prev[j + 1] + 1)
                .min(curr[j] + 1)
                .min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b_len]
}

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    eof_token: Token,
    comments: Vec<Comment>,
}

impl Parser {
    pub fn new(source: &str) -> Result<Self, Vec<ParseError>> {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().map_err(|errors| {
            errors
                .into_iter()
                .map(|e| ParseError {
                    message: e.message,
                    span: e.span,
                    suggestion: e.suggestion,
                })
                .collect::<Vec<_>>()
        })?;

        // Split comment tokens out of the main token stream so they do not
        // interfere with parsing, while preserving them (lossless AST).
        let mut comments = Vec::new();
        let mut tokens: Vec<Token> = tokens
            .into_iter()
            .filter_map(|t| match t.kind {
                TokenKind::Comment(text) => {
                    comments.push(Comment { text, span: t.span });
                    None
                }
                _ => Some(t),
            })
            .collect();

        // The lexer always ends with an Eof token; keep it as the last element.
        if !matches!(tokens.last().map(|t| &t.kind), Some(TokenKind::Eof)) {
            tokens.push(Token::new(
                TokenKind::Eof,
                Span::new(source.len(), source.len()),
            ));
        }

        let eof_token = Token::new(TokenKind::Eof, Span::new(source.len(), source.len()));
        Ok(Self {
            tokens,
            pos: 0,
            eof_token,
            comments,
        })
    }

    fn peek(&self) -> &TokenKind {
        self.tokens
            .get(self.pos)
            .map(|t| &t.kind)
            .unwrap_or(&self.eof_token.kind)
    }

    fn peek_token(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&self.eof_token)
    }

    fn advance(&mut self) -> Token {
        let token = self
            .tokens
            .get(self.pos)
            .cloned()
            .unwrap_or_else(|| self.eof_token.clone());
        self.pos += 1;
        token
    }

    fn expect(&mut self, expected: &TokenKind) -> Result<Token, ParseError> {
        let token = self.advance();
        if &token.kind == expected {
            Ok(token)
        } else {
            Err(ParseError {
                message: format!("Expected '{expected}', found '{}'", token.kind),
                span: token.span,
                suggestion: self.closing_suggestion(expected, &token.kind),
            })
        }
    }

    fn closing_suggestion(&self, expected: &TokenKind, found: &TokenKind) -> Option<String> {
        match (expected, found) {
            (TokenKind::RBracket, _) => {
                Some("Did you forget to close the list with `]`?".to_string())
            }
            (TokenKind::RParen, _) => {
                Some("Did you forget to close the parenthesis with `)`?".to_string())
            }
            (TokenKind::LBracket, _) => {
                Some("Did you forget to open the list with `[`?".to_string())
            }
            _ => None,
        }
    }

    fn expect_ident(&mut self) -> Result<Ident, ParseError> {
        let token = self.advance();
        match &token.kind {
            TokenKind::Ident(name) => Ok(Ident::new(name.clone(), token.span)),
            other => Err(ParseError {
                message: format!("Expected identifier, found '{}'", token.kind),
                span: token.span,
                suggestion: self.ident_suggestion(other),
            }),
        }
    }

    fn ident_suggestion(&self, found: &TokenKind) -> Option<String> {
        match found {
            TokenKind::Eq => Some(
                "`==` must come between two expressions, e.g. `filter status == 'active'`"
                    .to_string(),
            ),
            TokenKind::Filter => Some("Keyword `filter` cannot be used as a name here".to_string()),
            _ => None,
        }
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek(), TokenKind::Newline) {
            self.advance();
        }
    }

    fn is_pipe_separator(&self) -> bool {
        matches!(self.peek(), TokenKind::Pipe | TokenKind::Newline)
    }

    /// Peek `offset` tokens ahead, skipping any intervening newline tokens, and
    /// return the first non-newline token kind. Used for lookaheads that must
    /// tolerate line breaks, e.g. `in (\n from ...)` subqueries.
    fn peek_past_newlines(&self, offset: usize) -> Option<&TokenKind> {
        let mut i = self.pos + offset;
        while let Some(t) = self.tokens.get(i) {
            if matches!(t.kind, TokenKind::Newline) {
                i += 1;
            } else {
                return Some(&t.kind);
            }
        }
        None
    }

    fn parse_pipe_op(&mut self) -> Result<(), ParseError> {
        // Consume ALL consecutive separators (| and newlines) as a single step
        // separator. This supports `\n | step` and `| \n step` styles.
        let mut consumed = false;
        while matches!(self.peek(), TokenKind::Pipe | TokenKind::Newline) {
            self.advance();
            consumed = true;
        }
        if consumed {
            Ok(())
        } else {
            Err(ParseError {
                message: "Expected pipe operator '|' or newline".to_string(),
                span: self.peek_token().span,
                suggestion: Some("Separate pipeline steps with `|` or a newline".to_string()),
            })
        }
    }

    pub fn parse_pipeline(&mut self) -> Result<Pipeline, Vec<ParseError>> {
        self.skip_newlines();

        // Parse table source
        let source = self.parse_table_source()?;

        let mut steps = Vec::new();

        // Parse pipe steps
        while self.is_pipe_separator() {
            // Peek ahead to check if this pipe is followed by 'union' or 'union all'
            // If so, stop parsing steps and let parse_statement handle the union
            let saved_pos = self.pos;
            while matches!(self.peek(), TokenKind::Pipe | TokenKind::Newline) {
                self.advance();
            }
            self.skip_newlines();
            if matches!(self.peek(), TokenKind::Union | TokenKind::All) {
                // Backtrack - don't consume this pipe separator
                self.pos = saved_pos;
                break;
            }
            self.pos = saved_pos;

            self.parse_pipe_op().map_err(|e| vec![e])?;
            self.skip_newlines();

            if matches!(self.peek(), TokenKind::Eof) {
                break;
            }

            let step = self.parse_step()?;
            steps.push(step);
        }

        // Reject any leftover tokens after the last step so that malformed
        // input is never silently truncated (e.g. `filter status = 'x'`
        // missing the comparison must still surface as an error).
        if !matches!(
            self.peek(),
            TokenKind::Eof | TokenKind::Pipe | TokenKind::Newline
        ) {
            return Err(vec![ParseError {
                message: format!("Unexpected token '{}' after pipeline", self.peek()),
                span: self.peek_token().span,
                suggestion: Some(
                    "Remove the extra text or separate it into another step with `|`".to_string(),
                ),
            }]);
        }

        Ok(Pipeline {
            source,
            steps,
            comments: std::mem::take(&mut self.comments),
        })
    }

    /// Parse a full statement: a read/mutation pipeline, an `insert`/`upsert`, a
    /// `table` DDL declaration, or a `union` combination.
    pub fn parse_statement(&mut self) -> Result<Statement, Vec<ParseError>> {
        self.skip_newlines();

        let stmt = match self.peek() {
            TokenKind::From => self.parse_pipeline().map(Statement::Pipeline),
            TokenKind::Into => self.parse_into_statement(),
            TokenKind::Table => self.parse_create_table(),
            TokenKind::Eof => Err(vec![ParseError {
                message: "Unexpected end of input".to_string(),
                span: self.peek_token().span,
                suggestion: Some(
                    "A PipeQL statement starts with `from`, `into`, or `table`".to_string(),
                ),
            }]),
            TokenKind::Ident(name) => {
                let hint = if let Some(suggested) = suggest_keyword(name) {
                    format!("Did you mean `{suggested}`?")
                } else {
                    "Statements begin with `from <table>`, `into <table>`, or `table <name>`".to_string()
                };
                Err(vec![ParseError {
                    message: format!("Unknown statement keyword '{name}'"),
                    span: self.peek_token().span,
                    suggestion: Some(hint),
                }])
            }
            _ => Err(vec![ParseError {
                message: format!(
                    "Expected 'from', 'into', or 'table' to start a statement, found '{}'",
                    self.peek()
                ),
                span: self.peek_token().span,
                suggestion: Some(
                    "Statements begin with `from <table>`, `into <table>`, or `table <name>`"
                        .to_string(),
                ),
            }]),
        }?;

        // Check for union chaining: `<stmt> | union [all] <stmt>`
        self.skip_newlines();
        if self.is_pipe_separator() {
            let saved_pos = self.pos;
            // Consume pipe/newlines
            while matches!(self.peek(), TokenKind::Pipe | TokenKind::Newline) {
                self.advance();
            }
            self.skip_newlines();
            if matches!(self.peek(), TokenKind::Union) {
                let union_token = self.advance(); // consume 'union'
                let all = if matches!(self.peek(), TokenKind::All) {
                    self.advance();
                    true
                } else {
                    false
                };
                self.skip_newlines();
                // Parse the right-hand statement (recursive)
                let right = self.parse_statement()?;
                let span = Span::new(union_token.span.start, self.peek_token().span.start);
                return Ok(Statement::Union(UnionStmt {
                    left: Box::new(stmt),
                    right: Box::new(right),
                    all,
                    span,
                }));
            } else {
                // Not a union — backtrack
                self.pos = saved_pos;
            }
        }

        Ok(stmt)
    }

    /// Dispatch `into` to either `insert` or `upsert`.
    fn parse_into_statement(&mut self) -> Result<Statement, Vec<ParseError>> {
        // Look ahead to decide: after `into <table> |`, is the next keyword
        // `insert` or `upsert`?
        let saved_pos = self.pos;
        self.advance(); // consume 'into'
        let _table = self.expect_ident().map_err(|e| vec![e])?;

        // Consume pipe separator
        while matches!(self.peek(), TokenKind::Pipe | TokenKind::Newline) {
            self.advance();
        }
        self.skip_newlines();

        let is_upsert = matches!(self.peek(), TokenKind::Upsert);
        // Restore position and parse the correct statement type
        self.pos = saved_pos;

        if is_upsert {
            self.parse_upsert()
        } else {
            self.parse_insert()
        }
    }

    fn parse_insert(&mut self) -> Result<Statement, Vec<ParseError>> {
        let token = self.advance(); // consume 'into'
        let span_start = token.span.start;

        let table = self.expect_ident().map_err(|e| vec![e])?;
        self.parse_pipe_op().map_err(|e| vec![e])?;
        self.skip_newlines();

        self.expect(&TokenKind::Insert).map_err(|e| vec![e])?;
        self.expect(&TokenKind::LBracket).map_err(|e| vec![e])?;
        let assignments = self.parse_assignment_list().map_err(|e| vec![e])?;
        if assignments.is_empty() {
            return Err(vec![ParseError {
                message: "insert requires at least one column assignment".to_string(),
                span: self.peek_token().span,
                suggestion: None,
            }]);
        }
        self.expect(&TokenKind::RBracket).map_err(|e| vec![e])?;

        self.skip_newlines();
        self.reject_leftover()?;
        let span = Span::new(span_start, self.peek_token().span.start);
        Ok(Statement::Insert(InsertStmt {
            table,
            assignments,
            comments: std::mem::take(&mut self.comments),
            span,
        }))
    }

    fn parse_upsert(&mut self) -> Result<Statement, Vec<ParseError>> {
        let token = self.advance(); // consume 'into'
        let span_start = token.span.start;

        let table = self.expect_ident().map_err(|e| vec![e])?;
        self.parse_pipe_op().map_err(|e| vec![e])?;
        self.skip_newlines();

        self.expect(&TokenKind::Upsert).map_err(|e| vec![e])?;
        self.expect(&TokenKind::LBracket).map_err(|e| vec![e])?;
        let assignments = self.parse_assignment_list().map_err(|e| vec![e])?;
        self.expect(&TokenKind::RBracket).map_err(|e| vec![e])?;

        // Parse `| conflict [col1, col2, ...]`
        self.parse_pipe_op().map_err(|e| vec![e])?;
        self.skip_newlines();
        self.expect(&TokenKind::Conflict).map_err(|e| vec![e])?;
        self.expect(&TokenKind::LBracket).map_err(|e| vec![e])?;
        let mut conflict_columns = Vec::new();
        self.skip_newlines();
        if !matches!(self.peek(), TokenKind::RBracket) {
            conflict_columns.push(self.expect_ident().map_err(|e| vec![e])?);
            while matches!(self.peek(), TokenKind::Comma) {
                self.advance();
                self.skip_newlines();
                if matches!(self.peek(), TokenKind::RBracket) {
                    break;
                }
                conflict_columns.push(self.expect_ident().map_err(|e| vec![e])?);
            }
        }
        self.skip_newlines();
        self.expect(&TokenKind::RBracket).map_err(|e| vec![e])?;

        // Parse `| do update [...]`
        self.parse_pipe_op().map_err(|e| vec![e])?;
        self.skip_newlines();
        self.expect(&TokenKind::Do).map_err(|e| vec![e])?;
        self.expect(&TokenKind::Update).map_err(|e| vec![e])?;
        self.expect(&TokenKind::LBracket).map_err(|e| vec![e])?;
        let do_update = self.parse_assignment_list().map_err(|e| vec![e])?;
        self.expect(&TokenKind::RBracket).map_err(|e| vec![e])?;

        self.skip_newlines();
        self.reject_leftover()?;
        let span = Span::new(span_start, self.peek_token().span.start);
        Ok(Statement::Upsert(UpsertStmt {
            table,
            assignments,
            conflict_columns,
            do_update,
            comments: std::mem::take(&mut self.comments),
            span,
        }))
    }

    fn parse_create_table_stmt(&mut self) -> Result<CreateTableStmt, Vec<ParseError>> {
        let token = self.advance(); // consume 'table'
        let span_start = token.span.start;

        let name = self.expect_ident().map_err(|e| vec![e])?;
        self.expect(&TokenKind::LBracket).map_err(|e| vec![e])?;

        let mut columns = Vec::new();
        self.skip_newlines();
        if !matches!(self.peek(), TokenKind::RBracket) {
            columns.push(self.parse_column_def().map_err(|e| vec![e])?);
            while matches!(self.peek(), TokenKind::Comma) {
                self.advance(); // consume comma
                self.skip_newlines();
                if matches!(self.peek(), TokenKind::RBracket) {
                    break;
                }
                columns.push(self.parse_column_def().map_err(|e| vec![e])?);
            }
        }
        self.skip_newlines();
        self.expect(&TokenKind::RBracket).map_err(|e| vec![e])?;

        self.skip_newlines();
        let span = Span::new(span_start, self.peek_token().span.start);
        Ok(CreateTableStmt {
            name,
            columns,
            comments: std::mem::take(&mut self.comments),
            span,
        })
    }

    fn parse_create_table(&mut self) -> Result<Statement, Vec<ParseError>> {
        let stmt = self.parse_create_table_stmt()?;
        self.reject_leftover()?;
        Ok(Statement::CreateTable(stmt))
    }

    /// Parse one or more `table` DDL statements from a schema definition.
    pub fn parse_schema(&mut self) -> Result<Vec<CreateTableStmt>, Vec<ParseError>> {
        let mut tables = Vec::new();
        self.skip_newlines();

        while !matches!(self.peek(), TokenKind::Eof) {
            match self.peek() {
                TokenKind::Table => {
                    let table = self.parse_create_table_stmt()?;
                    tables.push(table);
                }
                other => {
                    return Err(vec![ParseError {
                        message: format!("Expected 'table' statement in schema, found '{other}'"),
                        span: self.peek_token().span,
                        suggestion: Some("Schemas consist of `table <name> [column defs]` statements".to_string()),
                    }]);
                }
            }
            self.skip_newlines();
        }

        if tables.is_empty() {
            return Err(vec![ParseError {
                message: "Schema contains no 'table' statements".to_string(),
                span: self.eof_token.span,
                suggestion: Some("Provide at least one `table <name> [...]` statement".to_string()),
            }]);
        }

        Ok(tables)
    }

    fn parse_column_def(&mut self) -> Result<ColumnDef, ParseError> {
        let name = self.expect_ident()?;
        let ty = self.parse_column_type()?;

        let mut modifiers = Vec::new();
        loop {
            match self.peek() {
                TokenKind::Not => {
                    self.advance(); // consume 'not'
                    self.expect(&TokenKind::Null)?;
                    modifiers.push(ColumnModifier::NotNull);
                }
                TokenKind::Ident(name) => match name.as_str() {
                    s if s.eq_ignore_ascii_case("primary") => {
                        self.advance();
                        modifiers.push(ColumnModifier::PrimaryKey);
                    }
                    s if s.eq_ignore_ascii_case("auto") => {
                        self.advance();
                        modifiers.push(ColumnModifier::AutoIncrement);
                    }
                    s if s.eq_ignore_ascii_case("unique") => {
                        self.advance();
                        modifiers.push(ColumnModifier::Unique);
                    }
                    s if s.eq_ignore_ascii_case("default") => {
                        self.advance();
                        let expr = self.parse_expr()?;
                        modifiers.push(ColumnModifier::Default(expr));
                    }
                    _ => break,
                },
                _ => break,
            }
        }

        Ok(ColumnDef {
            name,
            ty,
            modifiers,
        })
    }

    fn parse_column_type(&mut self) -> Result<ColumnType, ParseError> {
        let token = self.advance();
        match &token.kind {
            TokenKind::Ident(name) => match name.as_str() {
                s if s.eq_ignore_ascii_case("int") || s.eq_ignore_ascii_case("integer") => Ok(ColumnType::Integer),
                s if s.eq_ignore_ascii_case("float") || s.eq_ignore_ascii_case("real") => Ok(ColumnType::Float),
                s if s.eq_ignore_ascii_case("string") || s.eq_ignore_ascii_case("text") => Ok(ColumnType::String),
                s if s.eq_ignore_ascii_case("bool") || s.eq_ignore_ascii_case("boolean") => Ok(ColumnType::Bool),
                s if s.eq_ignore_ascii_case("timestamp") || s.eq_ignore_ascii_case("datetime") => Ok(ColumnType::Timestamp),
                other => {
                    let hint = if let Some(suggested) = suggest_keyword(other) {
                        format!("Did you mean `{suggested}`? Supported types: int, float, string, bool, timestamp")
                    } else {
                        "Supported types: int, float, string, bool, timestamp".to_string()
                    };
                    Err(ParseError {
                        message: format!("Unknown column type '{other}'"),
                        span: token.span,
                        suggestion: Some(hint),
                    })
                }
            },
            other => Err(ParseError {
                message: format!("Expected column type after column name, found '{other}'"),
                span: token.span,
                suggestion: Some(
                    "Every column needs a type, e.g. `id int primary auto`".to_string(),
                ),
            }),
        }
    }

    fn reject_leftover(&self) -> Result<(), Vec<ParseError>> {
        if !matches!(self.peek(), TokenKind::Eof) {
            return Err(vec![ParseError {
                message: format!("Unexpected token '{}' after statement", self.peek()),
                span: self.peek_token().span,
                suggestion: Some(
                    "Remove the extra text or separate it into another statement".to_string(),
                ),
            }]);
        }
        Ok(())
    }

    fn parse_table_source(&mut self) -> Result<TableSource, Vec<ParseError>> {
        self.skip_newlines();

        if !matches!(self.peek(), TokenKind::From) {
            return Err(vec![ParseError {
                message: "Expected 'from' keyword to start pipeline".to_string(),
                span: self.peek_token().span,
                suggestion: Some("Every pipeline must begin with `from <table>`".to_string()),
            }]);
        }
        self.advance(); // consume 'from'

        let name = self.expect_ident().map_err(|e| vec![e])?;
        let alias = if matches!(self.peek(), TokenKind::As) {
            self.advance();
            Some(self.expect_ident().map_err(|e| vec![e])?)
        } else if matches!(self.peek(), TokenKind::Ident(_)) {
            Some(self.expect_ident().map_err(|e| vec![e])?)
        } else {
            None
        };

        Ok(TableSource { name, alias })
    }

    fn parse_step(&mut self) -> Result<PipelineStep, Vec<ParseError>> {
        match self.peek() {
            TokenKind::Filter => self.parse_filter(),
            TokenKind::Select => self.parse_select(),
            TokenKind::Derive => self.parse_derive(),
            TokenKind::Join => self.parse_join(),
            // SQL-style prefix join types: `left join t on ...`
            TokenKind::Left | TokenKind::Right | TokenKind::Full | TokenKind::Inner => {
                self.parse_join_with_prefix()
            }
            TokenKind::Group => self.parse_group(),
            TokenKind::Sort => self.parse_sort(),
            TokenKind::Take => self.parse_take(),
            TokenKind::Skip => self.parse_skip(),
            TokenKind::Update => self.parse_update(),
            TokenKind::Delete => self.parse_delete(),
            TokenKind::Eof => Err(vec![ParseError {
                message: "Unexpected end of input".to_string(),
                span: self.peek_token().span,
                suggestion: Some("Expected a pipeline step such as `filter`, `select`, `derive`, or `take` after `|`".to_string()),
            }]),
            TokenKind::Ident(name) => {
                let hint = if let Some(suggested) = suggest_keyword(name) {
                    format!("Did you mean `{suggested}`?")
                } else {
                    "Supported steps: filter, select, derive, join (or left/right/full join), group, sort, take, skip, update, delete".to_string()
                };
                Err(vec![ParseError {
                    message: format!("Unknown pipeline step '{name}'"),
                    span: self.peek_token().span,
                    suggestion: Some(hint),
                }])
            }
            _ => Err(vec![ParseError {
                message: format!("Expected pipeline step, found '{}'", self.peek()),
                span: self.peek_token().span,
                suggestion: Some("Supported steps: filter, select, derive, join (or left/right/full join), group, sort, take, skip, update, delete".to_string()),
            }]),
        }
    }

    fn parse_update(&mut self) -> Result<PipelineStep, Vec<ParseError>> {
        let token = self.advance(); // consume 'update'
        let span_start = token.span.start;

        // Optional explicit `all` marker: `update all [...]` is the opt-in
        // escape hatch for full-table updates (bypasses the filter guard).
        let all = if matches!(self.peek(), TokenKind::All) {
            self.advance();
            true
        } else {
            false
        };

        self.expect(&TokenKind::LBracket).map_err(|e| vec![e])?;
        let assignments = self.parse_assignment_list().map_err(|e| vec![e])?;
        self.expect(&TokenKind::RBracket).map_err(|e| vec![e])?;

        Ok(PipelineStep::Update {
            assignments,
            all,
            span: Span::new(span_start, self.peek_token().span.start),
        })
    }

    fn parse_delete(&mut self) -> Result<PipelineStep, Vec<ParseError>> {
        let token = self.advance(); // consume 'delete'

        // Optional explicit `all` marker: `delete all` is the opt-in escape
        // hatch for full-table deletes (bypasses the filter guard).
        let all = if matches!(self.peek(), TokenKind::All) {
            self.advance();
            true
        } else {
            false
        };

        Ok(PipelineStep::Delete {
            all,
            span: Span::new(token.span.start, self.peek_token().span.start),
        })
    }

    fn parse_filter(&mut self) -> Result<PipelineStep, Vec<ParseError>> {
        let token = self.advance(); // consume 'filter'
        let span_start = token.span.start;

        let expr = self.parse_expr().map_err(|e| vec![e])?;

        Ok(PipelineStep::Filter {
            expr,
            span: Span::new(span_start, self.peek_token().span.start),
        })
    }

    fn parse_select(&mut self) -> Result<PipelineStep, Vec<ParseError>> {
        let token = self.advance(); // consume 'select'
        let span_start = token.span.start;

        self.expect(&TokenKind::LBracket).map_err(|e| vec![e])?;
        let columns = self.parse_select_list().map_err(|e| vec![e])?;
        self.expect(&TokenKind::RBracket).map_err(|e| vec![e])?;

        Ok(PipelineStep::Select {
            columns,
            span: Span::new(span_start, self.peek_token().span.start),
        })
    }

    fn parse_select_list(&mut self) -> Result<Vec<SelectItem>, ParseError> {
        let mut items = Vec::new();

        self.skip_newlines();
        if matches!(self.peek(), TokenKind::RBracket) {
            return Ok(items);
        }

        items.push(self.parse_select_item()?);

        while matches!(self.peek(), TokenKind::Comma) {
            self.advance(); // consume comma
            self.skip_newlines();
            if matches!(self.peek(), TokenKind::RBracket) {
                break;
            }
            items.push(self.parse_select_item()?);
        }

        self.skip_newlines();
        Ok(items)
    }

    fn parse_select_item(&mut self) -> Result<SelectItem, ParseError> {
        let expr = self.parse_expr()?;
        let alias = if matches!(self.peek(), TokenKind::As) {
            self.advance(); // consume 'as'
            Some(self.expect_ident()?)
        } else {
            None
        };
        Ok(SelectItem { expr, alias })
    }

    fn parse_derive(&mut self) -> Result<PipelineStep, Vec<ParseError>> {
        let token = self.advance(); // consume 'derive'
        let span_start = token.span.start;

        self.expect(&TokenKind::LBracket).map_err(|e| vec![e])?;
        let assignments = self.parse_assignment_list().map_err(|e| vec![e])?;
        self.expect(&TokenKind::RBracket).map_err(|e| vec![e])?;

        Ok(PipelineStep::Derive {
            assignments,
            span: Span::new(span_start, self.peek_token().span.start),
        })
    }

    /// Parse a join step with the type *before* `join`, SQL-style:
    /// `left join users on a.id == users.id`.
    fn parse_join_with_prefix(&mut self) -> Result<PipelineStep, Vec<ParseError>> {
        let token = self.advance(); // consume 'left' | 'right' | 'full' | 'inner'
        let join_type = match token.kind {
            TokenKind::Left => JoinType::Left,
            TokenKind::Right => JoinType::Right,
            TokenKind::Full => JoinType::Full,
            TokenKind::Inner => JoinType::Inner,
            // Defensive: only reachable if a future caller dispatches other
            // tokens here. Produce a real error instead of panicking.
            other => {
                return Err(vec![ParseError {
                    message: format!("Expected a join type keyword, found '{other}'"),
                    span: token.span,
                    suggestion: Some(
                        "Supported join types: left, right, full, inner".to_string(),
                    ),
                }]);
            }
        };
        self.expect(&TokenKind::Join).map_err(|e| vec![e])?;
        self.parse_join_body(join_type, token.span.start)
    }

    fn parse_join(&mut self) -> Result<PipelineStep, Vec<ParseError>> {
        let token = self.advance(); // consume 'join'
        let span_start = token.span.start;

        // Optional join type (`join left`, `join right`, ...)
        let join_type = match self.peek() {
            TokenKind::Left => {
                self.advance();
                JoinType::Left
            }
            TokenKind::Right => {
                self.advance();
                JoinType::Right
            }
            TokenKind::Full => {
                self.advance();
                JoinType::Full
            }
            TokenKind::Inner => {
                self.advance();
                JoinType::Inner
            }
            _ => JoinType::Inner,
        };

        self.parse_join_body(join_type, span_start)
    }

    /// Shared tail of both join spellings: `<table> [as alias] on <expr>`.
    fn parse_join_body(
        &mut self,
        join_type: JoinType,
        span_start: usize,
    ) -> Result<PipelineStep, Vec<ParseError>> {
        let table = self.expect_ident().map_err(|e| vec![e])?;

        // Optional alias (`as alias` or a bare identifier, per the grammar).
        let alias = if matches!(self.peek(), TokenKind::As) {
            self.advance();
            Some(self.expect_ident().map_err(|e| vec![e])?)
        } else if matches!(self.peek(), TokenKind::Ident(_)) {
            Some(self.expect_ident().map_err(|e| vec![e])?)
        } else {
            None
        };

        self.expect(&TokenKind::On).map_err(|e| vec![e])?;
        let on = self.parse_expr().map_err(|e| vec![e])?;

        Ok(PipelineStep::Join {
            join_type,
            table,
            alias,
            on,
            span: Span::new(span_start, self.peek_token().span.start),
        })
    }

    fn parse_group(&mut self) -> Result<PipelineStep, Vec<ParseError>> {
        let token = self.advance(); // consume 'group'
        let span_start = token.span.start;

        self.expect(&TokenKind::LBracket).map_err(|e| vec![e])?;
        let columns = self.parse_expr_list().map_err(|e| vec![e])?;
        self.expect(&TokenKind::RBracket).map_err(|e| vec![e])?;

        self.expect(&TokenKind::LParen).map_err(|e| vec![e])?;
        let aggregates = self.parse_aggregate_list().map_err(|e| vec![e])?;
        self.expect(&TokenKind::RParen).map_err(|e| vec![e])?;

        Ok(PipelineStep::Group {
            columns,
            aggregates,
            span: Span::new(span_start, self.peek_token().span.start),
        })
    }

    fn parse_sort(&mut self) -> Result<PipelineStep, Vec<ParseError>> {
        let token = self.advance(); // consume 'sort'
        let span_start = token.span.start;

        self.expect(&TokenKind::LBracket).map_err(|e| vec![e])?;
        let items = self.parse_sort_list().map_err(|e| vec![e])?;
        self.expect(&TokenKind::RBracket).map_err(|e| vec![e])?;

        Ok(PipelineStep::Sort {
            items,
            span: Span::new(span_start, self.peek_token().span.start),
        })
    }

    fn parse_take(&mut self) -> Result<PipelineStep, Vec<ParseError>> {
        let token = self.advance(); // consume 'take'
        let span_start = token.span.start;

        let count_token = self.advance();
        let count = match &count_token.kind {
            TokenKind::Integer(v) => *v,
            _ => {
                return Err(vec![ParseError {
                    message: format!(
                        "Expected integer after 'take', found '{}'",
                        count_token.kind
                    ),
                    span: count_token.span,
                    suggestion: Some(
                        "`take` requires a positive integer, e.g. `take 10`".to_string(),
                    ),
                }]);
            }
        };

        Ok(PipelineStep::Take {
            count,
            span: Span::new(span_start, self.peek_token().span.start),
        })
    }

    fn parse_skip(&mut self) -> Result<PipelineStep, Vec<ParseError>> {
        let token = self.advance(); // consume 'skip'
        let span_start = token.span.start;

        let count_token = self.advance();
        let count = match &count_token.kind {
            TokenKind::Integer(v) => *v,
            _ => {
                return Err(vec![ParseError {
                    message: format!(
                        "Expected integer after 'skip', found '{}'",
                        count_token.kind
                    ),
                    span: count_token.span,
                    suggestion: Some(
                        "`skip` requires a positive integer, e.g. `skip 5`".to_string(),
                    ),
                }]);
            }
        };

        Ok(PipelineStep::Skip {
            count,
            span: Span::new(span_start, self.peek_token().span.start),
        })
    }

    fn parse_expr_list(&mut self) -> Result<Vec<Expr>, ParseError> {
        let mut exprs = Vec::new();

        self.skip_newlines();
        if matches!(self.peek(), TokenKind::RBracket) || matches!(self.peek(), TokenKind::Eof) {
            return Ok(exprs);
        }

        exprs.push(self.parse_expr()?);

        while matches!(self.peek(), TokenKind::Comma) {
            self.advance(); // consume comma
            self.skip_newlines();
            if matches!(self.peek(), TokenKind::RBracket) {
                break;
            }
            exprs.push(self.parse_expr()?);
        }

        self.skip_newlines();
        Ok(exprs)
    }

    fn parse_assignment_list(&mut self) -> Result<Vec<Assignment>, ParseError> {
        let mut assignments = Vec::new();

        self.skip_newlines();
        if matches!(self.peek(), TokenKind::RBracket) {
            return Ok(assignments);
        }

        assignments.push(self.parse_assignment()?);

        while matches!(self.peek(), TokenKind::Comma) {
            self.advance(); // consume comma
            self.skip_newlines();
            if matches!(self.peek(), TokenKind::RBracket) {
                break;
            }
            assignments.push(self.parse_assignment()?);
        }

        self.skip_newlines();
        Ok(assignments)
    }

    fn parse_assignment(&mut self) -> Result<Assignment, ParseError> {
        let name = self.expect_ident()?;
        self.expect(&TokenKind::Assign)?;
        let expr = self.parse_expr()?;
        Ok(Assignment { name, expr })
    }

    fn parse_aggregate_list(&mut self) -> Result<Vec<Aggregate>, ParseError> {
        let mut aggregates = Vec::new();

        self.skip_newlines();
        if matches!(self.peek(), TokenKind::RParen) {
            return Ok(aggregates);
        }

        aggregates.push(self.parse_aggregate()?);

        while matches!(self.peek(), TokenKind::Comma) {
            self.advance(); // consume comma
            self.skip_newlines();
            if matches!(self.peek(), TokenKind::RParen) {
                break;
            }
            aggregates.push(self.parse_aggregate()?);
        }

        self.skip_newlines();
        Ok(aggregates)
    }

    fn parse_aggregate(&mut self) -> Result<Aggregate, ParseError> {
        let name = self.expect_ident()?;
        self.expect(&TokenKind::Assign)?;
        let func = self.expect_ident()?;
        self.expect(&TokenKind::LParen)?;

        let args = if matches!(self.peek(), TokenKind::RParen) {
            Vec::new()
        } else {
            let mut args = Vec::new();
            args.push(self.parse_expr()?);
            while matches!(self.peek(), TokenKind::Comma) {
                self.advance();
                args.push(self.parse_expr()?);
            }
            args
        };

        self.expect(&TokenKind::RParen)?;

        Ok(Aggregate { name, func, args })
    }

    fn parse_sort_list(&mut self) -> Result<Vec<SortItem>, ParseError> {
        let mut items = Vec::new();

        self.skip_newlines();
        if matches!(self.peek(), TokenKind::RBracket) {
            return Ok(items);
        }

        items.push(self.parse_sort_item()?);

        while matches!(self.peek(), TokenKind::Comma) {
            self.advance(); // consume comma
            self.skip_newlines();
            if matches!(self.peek(), TokenKind::RBracket) {
                break;
            }
            items.push(self.parse_sort_item()?);
        }

        self.skip_newlines();
        Ok(items)
    }

    fn parse_sort_item(&mut self) -> Result<SortItem, ParseError> {
        let expr = self.parse_expr()?;
        let direction = match self.peek() {
            TokenKind::Asc => {
                self.advance();
                SortDirection::Asc
            }
            TokenKind::Desc => {
                self.advance();
                SortDirection::Desc
            }
            _ => SortDirection::Asc,
        };
        Ok(SortItem { expr, direction })
    }

    // Expression parsing with Pratt precedence
    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_expr_bp(0)
    }

    fn parse_expr_bp(&mut self, min_bp: u8) -> Result<Expr, ParseError> {
        // Prefix: `not <expr>`. NOT binds looser than comparisons and tighter
        // than AND, so `not a == 1 and b == 2` is `(NOT (a == 1)) AND (b == 2)`.
        let mut lhs = if matches!(self.peek(), TokenKind::Not) {
            self.advance();
            let inner = self.parse_expr_bp(Self::PREFIX_NOT_BP)?;
            Expr::UnaryOp {
                op: UnaryOp::Not,
                expr: Box::new(inner),
            }
        } else {
            self.parse_primary()?
        };

        loop {
            // Postfix: `expr is null` / `expr is not null`
            if matches!(self.peek(), TokenKind::Is) {
                self.advance(); // consume 'is'
                let negated = if matches!(self.peek(), TokenKind::Not) {
                    self.advance();
                    true
                } else {
                    false
                };
                self.expect(&TokenKind::Null)?;
                lhs = Expr::IsNull {
                    expr: Box::new(lhs),
                    negated,
                };
                continue;
            }

            // Infix: `expr in [...]` / `expr not in [...]`
            if matches!(self.peek(), TokenKind::In)
                || (matches!(self.peek(), TokenKind::Not)
                    && matches!(self.peek_past_newlines(1), Some(TokenKind::In)))
            {
                let (l_bp, _r_bp) = (7, 8);
                if l_bp < min_bp {
                    break;
                }
                let negated = matches!(self.peek(), TokenKind::Not);
                self.advance(); // consume 'in' (or 'not')
                if negated {
                    self.advance(); // consume 'in'
                }
                // Tolerate a line break between `in` and its operand:
                // `x in\n(from ...)` and `x in\n[1, 2]` both parse.
                self.skip_newlines();
                // A parenthesized group after `in` is either a subquery
                // (`in (from ...)`) or a literal list (`in (1, 2, 3)`), the
                // spelling every SQL developer expects.
                if matches!(self.peek(), TokenKind::LParen) {
                    // Peek ahead (tolerating newlines) to see if `from`
                    // follows the `(`: `in (\n from ...)` is a subquery,
                    // `in (\n 1, 2, 3)` is a literal list.
                    if matches!(self.peek_past_newlines(1), Some(TokenKind::From)) {
                        self.advance(); // consume '('
                                        // Parse the inner pipeline as a subquery
                        let source = self.parse_table_source().map_err(|e| {
                            // parse_table_source always reports at least one
                            // error; fall back to the current token's real
                            // position only if that invariant ever changes.
                            e.into_iter().next().unwrap_or_else(|| ParseError {
                                message: "Failed to parse subquery source".to_string(),
                                span: self.peek_token().span,
                                suggestion: None,
                            })
                        })?;
                        let mut steps = Vec::new();
                        while self.is_pipe_separator() {
                            self.parse_pipe_op()?;
                            self.skip_newlines();
                            if matches!(self.peek(), TokenKind::RParen | TokenKind::Eof) {
                                break;
                            }
                            let step = self.parse_step().map_err(|e| {
                                // parse_step always reports at least one
                                // error; fall back to the current token's real
                                // position only if that invariant ever changes.
                                e.into_iter().next().unwrap_or_else(|| ParseError {
                                    message: "Failed to parse subquery step".to_string(),
                                    span: self.peek_token().span,
                                    suggestion: None,
                                })
                            })?;
                            steps.push(step);
                        }
                        self.expect(&TokenKind::RParen)?;
                        let subquery = Pipeline {
                            source,
                            steps,
                            comments: Vec::new(),
                        };
                        lhs = Expr::InSubquery {
                            expr: Box::new(lhs),
                            subquery: Box::new(subquery),
                            negated,
                        };
                        continue;
                    }
                    // Literal list in parentheses: `in (1, 2, 3)`.
                    self.advance(); // consume '('
                    let list = self.parse_delimited_list(&TokenKind::RParen)?;
                    lhs = Expr::InList {
                        expr: Box::new(lhs),
                        list,
                        negated,
                    };
                    continue;
                }
                let list = self.parse_in_list()?;
                lhs = Expr::InList {
                    expr: Box::new(lhs),
                    list,
                    negated,
                };
                continue;
            }

            let op = match self.peek() {
                TokenKind::Eq => BinaryOp::Eq,
                TokenKind::Assign => BinaryOp::Eq,
                TokenKind::NotEq => BinaryOp::NotEq,
                TokenKind::Lt => BinaryOp::Lt,
                TokenKind::LtEq => BinaryOp::LtEq,
                TokenKind::Gt => BinaryOp::Gt,
                TokenKind::GtEq => BinaryOp::GtEq,
                TokenKind::And => BinaryOp::And,
                TokenKind::Or => BinaryOp::Or,
                TokenKind::Plus => BinaryOp::Add,
                TokenKind::Minus => BinaryOp::Sub,
                TokenKind::Star => BinaryOp::Mul,
                TokenKind::Slash => BinaryOp::Div,
                _ => break,
            };

            let (l_bp, r_bp) = Self::infix_binding_power(op);

            if l_bp < min_bp {
                break;
            }

            self.advance(); // consume operator
            let rhs = self.parse_expr_bp(r_bp)?;

            lhs = Expr::BinaryOp {
                left: Box::new(lhs),
                op,
                right: Box::new(rhs),
            };
        }

        Ok(lhs)
    }

    fn parse_in_list(&mut self) -> Result<Vec<Expr>, ParseError> {
        self.expect(&TokenKind::LBracket)?;
        let list = self.parse_delimited_list(&TokenKind::RBracket)?;
        Ok(list)
    }

    /// Parse a comma-separated expression list terminated by `close` (the `]`
    /// of `in [...]` or the `)` of `in (...)`).
    fn parse_delimited_list(&mut self, close: &TokenKind) -> Result<Vec<Expr>, ParseError> {
        self.skip_newlines();
        let mut list = Vec::new();
        if self.peek() != close {
            list.push(self.parse_expr()?);
            while matches!(self.peek(), TokenKind::Comma) {
                self.advance();
                self.skip_newlines();
                if self.peek() == close {
                    break;
                }
                list.push(self.parse_expr()?);
            }
        }
        self.skip_newlines();
        self.expect(close)?;
        Ok(list)
    }

    const PREFIX_NOT_BP: u8 = 4;

    fn infix_binding_power(op: BinaryOp) -> (u8, u8) {
        match op {
            BinaryOp::Or => (1, 2),
            BinaryOp::And => (3, 4),
            BinaryOp::Eq | BinaryOp::NotEq => (5, 6),
            BinaryOp::Lt | BinaryOp::LtEq | BinaryOp::Gt | BinaryOp::GtEq => (7, 8),
            BinaryOp::Add | BinaryOp::Sub => (9, 10),
            BinaryOp::Mul | BinaryOp::Div => (11, 12),
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        match self.peek().clone() {
            TokenKind::Ident(name) => {
                let token = self.advance();
                let ident = Ident::new(name, token.span);

                // Check for function call: ident(args)
                if matches!(self.peek(), TokenKind::LParen) {
                    self.advance(); // consume (
                    let args = if matches!(self.peek(), TokenKind::RParen) {
                        Vec::new()
                    } else {
                        let mut args = Vec::new();
                        args.push(self.parse_expr()?);
                        while matches!(self.peek(), TokenKind::Comma) {
                            self.advance();
                            args.push(self.parse_expr()?);
                        }
                        args
                    };
                    self.expect(&TokenKind::RParen)?;
                    return Ok(Expr::FunctionCall { name: ident, args });
                }

                // Check for dot notation: table.column or table.json.path
                if matches!(self.peek(), TokenKind::Dot) {
                    self.advance(); // consume .
                    let column = self.expect_ident()?;
                    let mut json_path = Vec::new();
                    while matches!(self.peek(), TokenKind::Dot) {
                        self.advance(); // consume .
                        let segment = self.expect_ident()?;
                        json_path.push(segment);
                    }
                    return Ok(Expr::ColumnRef {
                        table: Some(ident),
                        column,
                        json_path,
                    });
                }

                Ok(Expr::Ident(ident))
            }
            TokenKind::Star => {
                self.advance();
                Ok(Expr::Star)
            }
            TokenKind::Integer(v) => {
                self.advance();
                Ok(Expr::Literal(Literal::Integer(v)))
            }
            TokenKind::Float(v) => {
                self.advance();
                Ok(Expr::Literal(Literal::Float(v)))
            }
            TokenKind::String(v) => {
                self.advance();
                Ok(Expr::Literal(Literal::String(v)))
            }
            TokenKind::True => {
                self.advance();
                Ok(Expr::Literal(Literal::Bool(true)))
            }
            TokenKind::False => {
                self.advance();
                Ok(Expr::Literal(Literal::Bool(false)))
            }
            TokenKind::Null => {
                self.advance();
                Ok(Expr::Literal(Literal::Null))
            }
            TokenKind::Param(name) => {
                let token = self.advance();
                Ok(Expr::Parameter(Parameter {
                    name,
                    span: token.span,
                }))
            }
            TokenKind::ParamBraced(name) => {
                let token = self.advance();
                Ok(Expr::Parameter(Parameter {
                    name,
                    span: token.span,
                }))
            }
            TokenKind::LParen => {
                self.advance(); // consume (
                let expr = self.parse_expr()?;
                self.expect(&TokenKind::RParen)?;
                Ok(expr)
            }
            _ => Err(ParseError {
                message: format!("Unexpected token '{}' in expression", self.peek()),
                span: self.peek_token().span,
                suggestion: Some("Expressions can be identifiers, literals, `$params`, function calls, or binary operations".to_string()),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_pipeline() {
        let source = "from users | filter age > 18 | select [id, name]";
        let mut parser = Parser::new(source).unwrap();
        let pipeline = parser.parse_pipeline().unwrap();

        assert_eq!(pipeline.source.name.name, "users");
        assert_eq!(pipeline.steps.len(), 2);
        assert!(matches!(&pipeline.steps[0], PipelineStep::Filter { .. }));
        assert!(matches!(&pipeline.steps[1], PipelineStep::Select { .. }));
    }

    #[test]
    fn test_parse_table_alias() {
        let source = "from users as u | select [u.id]";
        let mut parser = Parser::new(source).unwrap();
        let pipeline = parser.parse_pipeline().unwrap();

        assert_eq!(pipeline.source.alias.unwrap().name, "u");
    }

    #[test]
    fn test_parse_take_skip() {
        let source = "from users | take 10 | skip 5";
        let mut parser = Parser::new(source).unwrap();
        let pipeline = parser.parse_pipeline().unwrap();

        assert!(matches!(
            &pipeline.steps[0],
            PipelineStep::Take { count: 10, .. }
        ));
        assert!(matches!(
            &pipeline.steps[1],
            PipelineStep::Skip { count: 5, .. }
        ));
    }

    #[test]
    fn test_parse_derive() {
        let source = "from users | derive [age_next = age + 1]";
        let mut parser = Parser::new(source).unwrap();
        let pipeline = parser.parse_pipeline().unwrap();

        if let PipelineStep::Derive { assignments, .. } = &pipeline.steps[0] {
            assert_eq!(assignments.len(), 1);
            assert_eq!(assignments[0].name.name, "age_next");
        } else {
            panic!("Expected Derive step");
        }
    }

    #[test]
    fn test_parse_sort() {
        let source = "from users | sort [name asc, age desc]";
        let mut parser = Parser::new(source).unwrap();
        let pipeline = parser.parse_pipeline().unwrap();

        if let PipelineStep::Sort { items, .. } = &pipeline.steps[0] {
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].direction, SortDirection::Asc);
            assert_eq!(items[1].direction, SortDirection::Desc);
        } else {
            panic!("Expected Sort step");
        }
    }

    #[test]
    fn test_parse_join() {
        let source = "from posts | join left users on author_id == users.id";
        let mut parser = Parser::new(source).unwrap();
        let pipeline = parser.parse_pipeline().unwrap();

        if let PipelineStep::Join {
            join_type, table, ..
        } = &pipeline.steps[0]
        {
            assert_eq!(*join_type, JoinType::Left);
            assert_eq!(table.name, "users");
        } else {
            panic!("Expected Join step");
        }
    }

    #[test]
    fn test_parse_join_sql_style_prefix() {
        // `left join t on ...` (SQL order) must parse identically to
        // `join left t on ...` (PipeQL order).
        for (keyword, expected) in [
            ("inner", JoinType::Inner),
            ("left", JoinType::Left),
            ("right", JoinType::Right),
            ("full", JoinType::Full),
        ] {
            let source = format!("from a | {keyword} join b on a.id == b.a_id");
            let mut parser = Parser::new(&source).unwrap();
            let pipeline = parser.parse_pipeline().unwrap();
            if let PipelineStep::Join {
                join_type, table, ..
            } = &pipeline.steps[0]
            {
                assert_eq!(*join_type, expected, "{keyword} join");
                assert_eq!(table.name, "b");
            } else {
                panic!("Expected Join step for {keyword} join");
            }
        }
    }

    #[test]
    fn test_parse_join_sql_style_prefix_with_alias() {
        let source = "from notes | left join archive as a on notes.id == a.note_id";
        let mut parser = Parser::new(source).unwrap();
        let pipeline = parser.parse_pipeline().unwrap();

        if let PipelineStep::Join {
            join_type,
            table,
            alias,
            ..
        } = &pipeline.steps[0]
        {
            assert_eq!(*join_type, JoinType::Left);
            assert_eq!(table.name, "archive");
            assert_eq!(alias.as_ref().map(|a| a.name.as_str()), Some("a"));
        } else {
            panic!("Expected Join step");
        }
    }

    #[test]
    fn test_parse_join_sql_style_bare_alias() {
        let source = "from posts p | left join users u on p.author_id == u.id";
        let mut parser = Parser::new(source).unwrap();
        let pipeline = parser.parse_pipeline().unwrap();
        if let PipelineStep::Join { alias, .. } = &pipeline.steps[0] {
            assert_eq!(alias.as_ref().map(|a| a.name.as_str()), Some("u"));
        } else {
            panic!("Expected Join step");
        }
    }

    #[test]
    fn test_parse_in_list_parenthesized() {
        let source = "from t | filter id in (1, 2, 3)";
        let mut parser = Parser::new(source).unwrap();
        let pipeline = parser.parse_pipeline().unwrap();

        if let PipelineStep::Filter { expr, .. } = &pipeline.steps[0] {
            match expr {
                Expr::InList { list, negated, .. } => {
                    assert_eq!(list.len(), 3);
                    assert!(!*negated);
                }
                _ => panic!("Expected InList expression"),
            }
        } else {
            panic!("Expected Filter step");
        }
    }

    #[test]
    fn test_parse_multiline_in_subquery() {
        // Newlines after the `(` and between subquery steps must parse.
        let source = "from orders\n| filter customer_id in (\n  from customers\n  | filter region == 'EU'\n  | select [id]\n)\n| select [id]";
        let mut parser = Parser::new(source).unwrap();
        let pipeline = parser.parse_pipeline().unwrap();
        if let PipelineStep::Filter { expr, .. } = &pipeline.steps[0] {
            match expr {
                Expr::InSubquery { subquery, negated, .. } => {
                    assert!(!*negated);
                    assert_eq!(subquery.source.name.name, "customers");
                    assert_eq!(subquery.steps.len(), 2);
                }
                _ => panic!("Expected InSubquery expression"),
            }
        } else {
            panic!("Expected Filter step");
        }
    }

    #[test]
    fn test_parse_in_newline_before_paren() {
        // A newline between `in` and `(` must also be tolerated.
        let source =
            "from orders\n| filter customer_id in\n(from customers | select [id])";
        let mut parser = Parser::new(source).unwrap();
        let pipeline = parser.parse_pipeline().unwrap();
        assert!(matches!(&pipeline.steps[0], PipelineStep::Filter { .. }));
    }

    #[test]
    fn test_parse_not_in_list_parenthesized() {
        let source = "from t | filter id not in (1, 2)";
        let mut parser = Parser::new(source).unwrap();
        let pipeline = parser.parse_pipeline().unwrap();

        if let PipelineStep::Filter { expr, .. } = &pipeline.steps[0] {
            match expr {
                Expr::InList { negated, .. } => assert!(*negated),
                _ => panic!("Expected InList expression"),
            }
        } else {
            panic!("Expected Filter step");
        }
    }

    #[test]
    fn test_parse_in_list_parenthesized_empty() {
        let source = "from t | filter id in ()";
        let mut parser = Parser::new(source).unwrap();
        let pipeline = parser.parse_pipeline().unwrap();
        if let PipelineStep::Filter { expr, .. } = &pipeline.steps[0] {
            match expr {
                Expr::InList { list, .. } => assert!(list.is_empty()),
                _ => panic!("Expected InList expression"),
            }
        } else {
            panic!("Expected Filter step");
        }
    }

    #[test]
    fn test_parse_join_prefix_and_in_parens_combined() {
        let source =
            "from orders | left join customers on orders.customer_id == customers.id | filter customers.region in ('EU', 'APAC')";
        let mut parser = Parser::new(source).unwrap();
        let pipeline = parser.parse_pipeline().unwrap();
        assert!(matches!(&pipeline.steps[0], PipelineStep::Join { .. }));
        assert!(matches!(&pipeline.steps[1], PipelineStep::Filter { .. }));
    }

    #[test]
    fn test_parse_group() {
        let source = "from orders | group [customer_id] (total = sum(amount))";
        let mut parser = Parser::new(source).unwrap();
        let pipeline = parser.parse_pipeline().unwrap();

        if let PipelineStep::Group {
            columns,
            aggregates,
            ..
        } = &pipeline.steps[0]
        {
            assert_eq!(columns.len(), 1);
            assert_eq!(aggregates.len(), 1);
            assert_eq!(aggregates[0].name.name, "total");
            assert_eq!(aggregates[0].func.name, "sum");
        } else {
            panic!("Expected Group step");
        }
    }

    #[test]
    fn test_parse_parameters() {
        let source = "from users | filter id == $user_id and name == ${full_name}";
        let mut parser = Parser::new(source).unwrap();
        let pipeline = parser.parse_pipeline().unwrap();

        if let PipelineStep::Filter { expr, .. } = &pipeline.steps[0] {
            // Should have two parameter references
            let params = collect_params(expr);
            assert_eq!(params.len(), 2);
            assert_eq!(params[0], "user_id");
            assert_eq!(params[1], "full_name");
        } else {
            panic!("Expected Filter step");
        }
    }

    fn collect_params(expr: &Expr) -> Vec<&str> {
        match expr {
            Expr::Parameter(p) => vec![&p.name],
            Expr::BinaryOp { left, right, .. } => {
                let mut params = collect_params(left);
                params.extend(collect_params(right));
                params
            }
            _ => vec![],
        }
    }

    #[test]
    fn test_parse_insert_statement() {
        let source = "into notes\n| insert [title = $title, is_pinned = 0]";
        let mut parser = Parser::new(source).unwrap();
        let stmt = parser.parse_statement().unwrap();
        let Statement::Insert(insert) = stmt else {
            panic!("Expected Insert statement");
        };
        assert_eq!(insert.table.name, "notes");
        assert_eq!(insert.assignments.len(), 2);
        assert_eq!(insert.assignments[0].name.name, "title");
        assert!(matches!(insert.assignments[0].expr, Expr::Parameter(_)));
        assert_eq!(insert.assignments[1].name.name, "is_pinned");
    }

    #[test]
    fn test_parse_update_delete_steps() {
        let source = "from notes | filter id == $id | update [title = $title]";
        let mut parser = Parser::new(source).unwrap();
        let pipeline = parser.parse_pipeline().unwrap();
        assert!(matches!(&pipeline.steps[1], PipelineStep::Update { .. }));

        let source = "from notes | filter is_archived == 0 | delete";
        let mut parser = Parser::new(source).unwrap();
        let pipeline = parser.parse_pipeline().unwrap();
        assert!(matches!(&pipeline.steps[1], PipelineStep::Delete { .. }));
    }

    #[test]
    fn test_parse_update_delete_all_escape_hatch() {
        // `delete all` / `update all [...]` set the explicit `all` flag.
        let source = "from notes | delete all";
        let mut parser = Parser::new(source).unwrap();
        let pipeline = parser.parse_pipeline().unwrap();
        match &pipeline.steps[0] {
            PipelineStep::Delete { all, .. } => assert!(*all),
            other => panic!("Expected Delete step, got {other:?}"),
        }

        let source = "from notes | update all [title = $title]";
        let mut parser = Parser::new(source).unwrap();
        let pipeline = parser.parse_pipeline().unwrap();
        match &pipeline.steps[0] {
            PipelineStep::Update { all, assignments, .. } => {
                assert!(*all);
                assert_eq!(assignments.len(), 1);
            }
            other => panic!("Expected Update step, got {other:?}"),
        }

        // Without `all` the flag stays false.
        let source = "from notes | delete";
        let mut parser = Parser::new(source).unwrap();
        let pipeline = parser.parse_pipeline().unwrap();
        match &pipeline.steps[0] {
            PipelineStep::Delete { all, .. } => assert!(!*all),
            other => panic!("Expected Delete step, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_create_table() {
        let source = "table notes [
            id int primary auto,
            title string not null,
            category string default 'Personal',
            is_pinned int default 0,
            created_at timestamp default current_timestamp
          ]";
        let mut parser = Parser::new(source).unwrap();
        let stmt = parser.parse_statement().unwrap();
        let Statement::CreateTable(create) = stmt else {
            panic!("Expected CreateTable statement");
        };
        assert_eq!(create.name.name, "notes");
        assert_eq!(create.columns.len(), 5);
        assert_eq!(create.columns[0].ty, ColumnType::Integer);
        assert_eq!(
            create.columns[0].modifiers,
            vec![ColumnModifier::PrimaryKey, ColumnModifier::AutoIncrement]
        );
        assert!(matches!(
            create.columns[1].modifiers[0],
            ColumnModifier::NotNull
        ));
    }

    #[test]
    fn test_parse_statement_rejects_garbage() {
        let mut parser = Parser::new("explode [id]").unwrap();
        assert!(parser.parse_statement().is_err());
    }

    #[test]
    fn test_insert_leftover_is_rejected() {
        let mut parser = Parser::new("into notes | insert [title = 'x'] extra").unwrap();
        assert!(parser.parse_statement().is_err());
    }

    #[test]
    fn test_statements_tolerate_trailing_newlines() {
        for source in [
            "into notes | insert [title = $t]\n",
            "into notes\n| insert [title = $t]\n\n",
            "table notes [id int primary auto]\n",
            "table notes [\n  id int primary auto\n]\n\n",
        ] {
            let mut parser = Parser::new(source).unwrap();
            assert!(
                parser.parse_statement().is_ok(),
                "trailing newline rejected for: {source:?}"
            );
        }
        // Trailing non-newline garbage is still rejected.
        let mut parser = Parser::new("into notes | insert [title = 'x']\nextra").unwrap();
        assert!(parser.parse_statement().is_err());
    }
}
