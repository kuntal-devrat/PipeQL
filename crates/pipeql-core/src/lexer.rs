use std::fmt;

use crate::ast::Span;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Keywords
    From,
    Filter,
    Select,
    Derive,
    Join,
    Group,
    Sort,
    Take,
    Skip,
    Into,
    Insert,
    Update,
    Delete,
    Table,
    As,
    On,
    And,
    Or,
    Not,
    In,
    Is,
    Null,
    True,
    False,
    Left,
    Right,
    Full,
    Inner,
    Asc,
    Desc,
    Upsert,
    Conflict,
    Do,
    Union,
    All,

    // Literals
    Integer(i64),
    Float(f64),
    String(String),

    // Identifiers
    Ident(String),

    // Parameters
    Param(String),
    ParamBraced(String),

    // Operators
    Pipe,     // |
    Eq,       // ==
    NotEq,    // !=
    Lt,       // <
    LtEq,     // <=
    Gt,       // >
    GtEq,     // >=
    Plus,     // +
    Minus,    // -
    Star,     // *
    Slash,    // /
    Assign,   // =
    LParen,   // (
    RParen,   // )
    LBracket, // [
    RBracket, // ]
    Comma,    // ,
    Dot,      // .

    // Special
    Comment(String),
    Newline,
    Eof,
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenKind::From => write!(f, "from"),
            TokenKind::Filter => write!(f, "filter"),
            TokenKind::Select => write!(f, "select"),
            TokenKind::Derive => write!(f, "derive"),
            TokenKind::Join => write!(f, "join"),
            TokenKind::Group => write!(f, "group"),
            TokenKind::Sort => write!(f, "sort"),
            TokenKind::Take => write!(f, "take"),
            TokenKind::Skip => write!(f, "skip"),
            TokenKind::Into => write!(f, "into"),
            TokenKind::Insert => write!(f, "insert"),
            TokenKind::Update => write!(f, "update"),
            TokenKind::Delete => write!(f, "delete"),
            TokenKind::Table => write!(f, "table"),
            TokenKind::As => write!(f, "as"),
            TokenKind::On => write!(f, "on"),
            TokenKind::And => write!(f, "and"),
            TokenKind::Or => write!(f, "or"),
            TokenKind::Not => write!(f, "not"),
            TokenKind::In => write!(f, "in"),
            TokenKind::Is => write!(f, "is"),
            TokenKind::Null => write!(f, "null"),
            TokenKind::True => write!(f, "true"),
            TokenKind::False => write!(f, "false"),
            TokenKind::Left => write!(f, "left"),
            TokenKind::Right => write!(f, "right"),
            TokenKind::Full => write!(f, "full"),
            TokenKind::Inner => write!(f, "inner"),
            TokenKind::Asc => write!(f, "asc"),
            TokenKind::Desc => write!(f, "desc"),
            TokenKind::Upsert => write!(f, "upsert"),
            TokenKind::Conflict => write!(f, "conflict"),
            TokenKind::Do => write!(f, "do"),
            TokenKind::Union => write!(f, "union"),
            TokenKind::All => write!(f, "all"),
            TokenKind::Integer(v) => write!(f, "{v}"),
            TokenKind::Float(v) => {
                let s = format!("{v}");
                if s.contains('e') || s.contains('E') {
                    write!(f, "{v:.15}")
                } else {
                    write!(f, "{v}")
                }
            }
            TokenKind::String(v) => {
                let escaped = v.replace('\'', "''");
                write!(f, "'{escaped}'")
            }
            TokenKind::Ident(v) => write!(f, "{v}"),
            TokenKind::Param(v) => write!(f, "${v}"),
            TokenKind::ParamBraced(v) => write!(f, "${{{v}}}"),
            TokenKind::Pipe => write!(f, "|"),
            TokenKind::Eq => write!(f, "=="),
            TokenKind::NotEq => write!(f, "!="),
            TokenKind::Lt => write!(f, "<"),
            TokenKind::LtEq => write!(f, "<="),
            TokenKind::Gt => write!(f, ">"),
            TokenKind::GtEq => write!(f, ">="),
            TokenKind::Plus => write!(f, "+"),
            TokenKind::Minus => write!(f, "-"),
            TokenKind::Star => write!(f, "*"),
            TokenKind::Slash => write!(f, "/"),
            TokenKind::Assign => write!(f, "="),
            TokenKind::LParen => write!(f, "("),
            TokenKind::RParen => write!(f, ")"),
            TokenKind::LBracket => write!(f, "["),
            TokenKind::RBracket => write!(f, "]"),
            TokenKind::Comma => write!(f, ","),
            TokenKind::Dot => write!(f, "."),
            TokenKind::Comment(v) => write!(f, "--{v}"),
            TokenKind::Newline => write!(f, "\\n"),
            TokenKind::Eof => write!(f, "EOF"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LexerError {
    pub message: String,
    pub span: Span,
    pub suggestion: Option<String>,
}

impl fmt::Display for LexerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Lexer error at {}..{}: {}",
            self.span.start, self.span.end, self.message
        )
    }
}

impl std::error::Error for LexerError {}

pub struct Lexer<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    fn remaining(&self) -> &'a str {
        &self.input[self.pos..]
    }

    fn peek(&self) -> Option<char> {
        self.remaining().chars().next()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.remaining().chars().next()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() && ch != '\n' {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn read_line_comment(&mut self) -> TokenKind {
        let mut text = String::new();
        while let Some(ch) = self.peek() {
            if ch == '\n' {
                break;
            }
            text.push(ch);
            self.advance();
        }
        TokenKind::Comment(text.trim().to_string())
    }

    fn read_block_comment(&mut self, start: usize) -> Result<TokenKind, LexerError> {
        let mut text = String::new();
        while let Some(ch) = self.advance() {
            if ch == '*' && self.peek() == Some('/') {
                self.advance(); // consume '/'
                return Ok(TokenKind::Comment(text.trim().to_string()));
            }
            text.push(ch);
        }
        Err(LexerError {
            message: "Unterminated block comment".to_string(),
            span: Span::new(start, self.pos),
            suggestion: Some("Did you forget to close the comment with `*/`?".to_string()),
        })
    }

    fn read_string(&mut self, start: usize) -> Result<TokenKind, LexerError> {
        let mut value = String::new();
        loop {
            match self.advance() {
                Some('\'') => {
                    // Check for escaped quote ''
                    if self.peek() == Some('\'') {
                        value.push('\'');
                        self.advance();
                    } else {
                        return Ok(TokenKind::String(value));
                    }
                    continue;
                }
                Some(ch) => value.push(ch),
                None => {
                    return Err(LexerError {
                        message: "Unterminated string literal".to_string(),
                        span: Span::new(start, self.pos),
                        suggestion: Some(
                            "Did you forget to close the string with a `'`?".to_string(),
                        ),
                    })
                }
            }
        }
    }

    fn read_number(&mut self, _start: usize, first: char) -> TokenKind {
        // Accumulate the digit string first, then parse it exactly once. This
        // avoids the previous overflow path, which corrupted big integers
        // (e.g. a 20-digit literal parsed as ~1/100th of its value).
        let mut digits = String::new();
        digits.push(first);
        let mut is_float = false;

        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() {
                digits.push(ch);
                self.advance();
            } else if ch == '.' && !is_float {
                is_float = true;
                digits.push('.');
                self.advance();
            } else {
                break;
            }
        }

        if is_float {
            TokenKind::Float(digits.parse().unwrap_or(0.0))
        } else if let Ok(v) = digits.parse::<i64>() {
            TokenKind::Integer(v)
        } else {
            // Integer overflow — fall back to float rather than corrupting the
            // value (matches the previous intent, minus the bug).
            TokenKind::Float(digits.parse().unwrap_or(0.0))
        }
    }

    fn read_identifier(&mut self, first: char) -> String {
        let mut s = String::new();
        s.push(first);
        while let Some(ch) = self.peek() {
            if ch.is_alphanumeric() || ch == '_' {
                s.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        s
    }

    fn keyword_or_ident(s: &str) -> TokenKind {
        if s.eq_ignore_ascii_case("from") { return TokenKind::From; }
        if s.eq_ignore_ascii_case("filter") { return TokenKind::Filter; }
        if s.eq_ignore_ascii_case("select") { return TokenKind::Select; }
        if s.eq_ignore_ascii_case("derive") { return TokenKind::Derive; }
        if s.eq_ignore_ascii_case("join") { return TokenKind::Join; }
        if s.eq_ignore_ascii_case("group") { return TokenKind::Group; }
        if s.eq_ignore_ascii_case("sort") { return TokenKind::Sort; }
        if s.eq_ignore_ascii_case("take") { return TokenKind::Take; }
        if s.eq_ignore_ascii_case("skip") { return TokenKind::Skip; }
        if s.eq_ignore_ascii_case("into") { return TokenKind::Into; }
        if s.eq_ignore_ascii_case("insert") { return TokenKind::Insert; }
        if s.eq_ignore_ascii_case("update") { return TokenKind::Update; }
        if s.eq_ignore_ascii_case("delete") { return TokenKind::Delete; }
        if s.eq_ignore_ascii_case("table") { return TokenKind::Table; }
        if s.eq_ignore_ascii_case("as") { return TokenKind::As; }
        if s.eq_ignore_ascii_case("on") { return TokenKind::On; }
        if s.eq_ignore_ascii_case("and") { return TokenKind::And; }
        if s.eq_ignore_ascii_case("or") { return TokenKind::Or; }
        if s.eq_ignore_ascii_case("not") { return TokenKind::Not; }
        if s.eq_ignore_ascii_case("in") { return TokenKind::In; }
        if s.eq_ignore_ascii_case("is") { return TokenKind::Is; }
        if s.eq_ignore_ascii_case("null") { return TokenKind::Null; }
        if s.eq_ignore_ascii_case("true") { return TokenKind::True; }
        if s.eq_ignore_ascii_case("false") { return TokenKind::False; }
        if s.eq_ignore_ascii_case("left") { return TokenKind::Left; }
        if s.eq_ignore_ascii_case("right") { return TokenKind::Right; }
        if s.eq_ignore_ascii_case("full") { return TokenKind::Full; }
        if s.eq_ignore_ascii_case("inner") { return TokenKind::Inner; }
        if s.eq_ignore_ascii_case("asc") { return TokenKind::Asc; }
        if s.eq_ignore_ascii_case("desc") { return TokenKind::Desc; }
        if s.eq_ignore_ascii_case("upsert") { return TokenKind::Upsert; }
        if s.eq_ignore_ascii_case("conflict") { return TokenKind::Conflict; }
        if s.eq_ignore_ascii_case("do") { return TokenKind::Do; }
        if s.eq_ignore_ascii_case("union") { return TokenKind::Union; }
        if s.eq_ignore_ascii_case("all") { return TokenKind::All; }
        TokenKind::Ident(s.to_string())
    }

    fn read_param(&mut self, start: usize) -> Result<TokenKind, LexerError> {
        match self.peek() {
            Some('{') => {
                self.advance(); // skip {
                let mut name = String::new();
                loop {
                    match self.advance() {
                        Some('}') => {
                            return Ok(TokenKind::ParamBraced(name));
                        }
                        Some(ch) if ch.is_alphanumeric() || ch == '_' => {
                            name.push(ch);
                        }
                        Some(ch) => {
                            return Err(LexerError {
                                message: format!("Invalid character '{ch}' in parameter name"),
                                span: Span::new(start, self.pos),
                                suggestion: Some(
                                    "Parameter names may contain letters, digits, and underscores"
                                        .to_string(),
                                ),
                            });
                        }
                        None => {
                            return Err(LexerError {
                                message: "Unterminated parameter".to_string(),
                                span: Span::new(start, self.pos),
                                suggestion: Some("Did you forget to close the parameter with `}`? e.g. `${name}`".to_string()),
                            });
                        }
                    }
                }
            }
            Some(ch) if ch.is_alphanumeric() || ch == '_' => {
                self.advance(); // consume the first character
                let name = self.read_identifier(ch);
                Ok(TokenKind::Param(name))
            }
            Some(ch) => Err(LexerError {
                message: format!("Expected parameter name after '$', found '{ch}'"),
                span: Span::new(start, self.pos),
                suggestion: Some("Parameters use the form `$name` or `${name}`".to_string()),
            }),
            None => Err(LexerError {
                message: "Unexpected end of input after '$'".to_string(),
                span: Span::new(start, self.pos),
                suggestion: Some("Parameters use the form `$name` or `${name}`".to_string()),
            }),
        }
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>, Vec<LexerError>> {
        let mut tokens = Vec::new();
        let mut errors = Vec::new();

        loop {
            self.skip_whitespace();

            let start = self.pos;

            if self.pos >= self.input.len() {
                tokens.push(Token::new(TokenKind::Eof, Span::new(start, start)));
                break;
            }

            let ch = match self.peek() {
                Some(ch) => ch,
                None => {
                    tokens.push(Token::new(TokenKind::Eof, Span::new(start, start)));
                    break;
                }
            };

            let kind = match ch {
                '\n' => {
                    self.advance();
                    // Collapse consecutive newlines
                    if tokens
                        .last()
                        .is_none_or(|t: &Token| t.kind != TokenKind::Newline)
                    {
                        TokenKind::Newline
                    } else {
                        continue;
                    }
                }
                '|' => {
                    self.advance();
                    TokenKind::Pipe
                }
                '(' => {
                    self.advance();
                    TokenKind::LParen
                }
                ')' => {
                    self.advance();
                    TokenKind::RParen
                }
                '[' => {
                    self.advance();
                    TokenKind::LBracket
                }
                ']' => {
                    self.advance();
                    TokenKind::RBracket
                }
                ',' => {
                    self.advance();
                    TokenKind::Comma
                }
                '.' => {
                    self.advance();
                    TokenKind::Dot
                }
                '+' => {
                    self.advance();
                    TokenKind::Plus
                }
                '-' => {
                    self.advance();
                    // Check for line comment --
                    if self.peek() == Some('-') {
                        self.advance();
                        self.read_line_comment()
                    } else {
                        TokenKind::Minus
                    }
                }
                '*' => {
                    self.advance();
                    TokenKind::Star
                }
                '/' => {
                    self.advance();
                    if self.peek() == Some('*') {
                        self.advance(); // consume '*'
                        match self.read_block_comment(start) {
                            Ok(kind) => kind,
                            Err(e) => {
                                errors.push(e);
                                continue;
                            }
                        }
                    } else {
                        TokenKind::Slash
                    }
                }
                '=' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        TokenKind::Eq
                    } else {
                        TokenKind::Assign
                    }
                }
                '!' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        TokenKind::NotEq
                    } else {
                        errors.push(LexerError {
                            message: "Unexpected character '!'".to_string(),
                            span: Span::new(start, self.pos),
                            suggestion: Some("Use `!=` for not-equal".to_string()),
                        });
                        continue;
                    }
                }
                '<' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        TokenKind::LtEq
                    } else if self.peek() == Some('>') {
                        self.advance();
                        TokenKind::NotEq
                    } else {
                        TokenKind::Lt
                    }
                }
                '>' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        TokenKind::GtEq
                    } else {
                        TokenKind::Gt
                    }
                }
                '\'' => {
                    self.advance();
                    match self.read_string(start) {
                        Ok(kind) => kind,
                        Err(e) => {
                            errors.push(e);
                            continue;
                        }
                    }
                }
                '$' => {
                    self.advance();
                    match self.read_param(start) {
                        Ok(kind) => kind,
                        Err(e) => {
                            errors.push(e);
                            continue;
                        }
                    }
                }
                ch if ch.is_ascii_digit() => {
                    self.advance(); // consume the first digit
                    self.read_number(start, ch)
                }
                ch if ch.is_alphanumeric() || ch == '_' => {
                    self.advance(); // consume the first character
                    let ident = self.read_identifier(ch);
                    Self::keyword_or_ident(&ident)
                }
                ch => {
                    self.advance();
                    errors.push(LexerError {
                        message: format!("Unexpected character '{ch}'"),
                        span: Span::new(start, self.pos),
                        suggestion: Some("Check the PipeQL grammar; only `|`, `==`, `!=`, `<>`, `<=`, `>=`, `<`, `>`, `+`, `-`, `*`, `/`, `=`, `(`, `)`, `[`, `]`, `,`, `.`, `$`, and `'` are operators".to_string()),
                    });
                    continue;
                }
            };

            tokens.push(Token::new(kind, Span::new(start, self.pos)));
        }

        if errors.is_empty() {
            Ok(tokens)
        } else {
            Err(errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_tokens() {
        let mut lexer = Lexer::new("from users | filter age > 18");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::From);
        assert_eq!(tokens[1].kind, TokenKind::Ident("users".into()));
        assert_eq!(tokens[2].kind, TokenKind::Pipe);
        assert_eq!(tokens[3].kind, TokenKind::Filter);
        assert_eq!(tokens[4].kind, TokenKind::Ident("age".into()));
        assert_eq!(tokens[5].kind, TokenKind::Gt);
        assert_eq!(tokens[6].kind, TokenKind::Integer(18));
    }

    #[test]
    fn test_string_literal() {
        let mut lexer = Lexer::new("from users | filter name == 'Alice'");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[6].kind, TokenKind::String("Alice".into()));
    }

    #[test]
    fn test_escaped_string() {
        let mut lexer = Lexer::new("from t | filter x == '''hello'''");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[6].kind, TokenKind::String("'hello'".into()));
    }

    #[test]
    fn test_parameters() {
        let mut lexer = Lexer::new("from users | filter id == $user_id and name == ${full_name}");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[6].kind, TokenKind::Param("user_id".into()));
        assert_eq!(tokens[10].kind, TokenKind::ParamBraced("full_name".into()));
    }

    #[test]
    fn test_keywords_case_insensitive() {
        let mut lexer = Lexer::new("FROM USERS | FILTER age > 18");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::From);
        assert_eq!(tokens[2].kind, TokenKind::Pipe);
        assert_eq!(tokens[3].kind, TokenKind::Filter);
    }

    #[test]
    fn test_mutation_keywords() {
        let mut lexer = Lexer::new(
            "into notes | insert [title = $t] | update [x = 1] | delete | table t [id int]",
        );
        let tokens = lexer.tokenize().unwrap();
        let kinds: Vec<_> = tokens.iter().map(|t| t.kind.clone()).collect();
        assert!(kinds.contains(&TokenKind::Into));
        assert!(kinds.contains(&TokenKind::Insert));
        assert!(kinds.contains(&TokenKind::Update));
        assert!(kinds.contains(&TokenKind::Delete));
        assert!(kinds.contains(&TokenKind::Table));
    }

    #[test]
    fn test_newlines_as_pipe() {
        let mut lexer = Lexer::new("from users\nfilter age > 18\nselect [id]");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::From);
        assert_eq!(tokens[2].kind, TokenKind::Newline);
        assert_eq!(tokens[3].kind, TokenKind::Filter);
    }

    #[test]
    fn test_line_comment() {
        let mut lexer = Lexer::new("from users -- this is a comment\n| filter age > 18");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::From);
        assert_eq!(tokens[1].kind, TokenKind::Ident("users".into()));
        // The comment is preserved as a Comment token with its text.
        assert_eq!(
            tokens[2].kind,
            TokenKind::Comment("this is a comment".into())
        );
        // After the comment, the newline becomes a Newline token (pipe separator)
        assert_eq!(tokens[3].kind, TokenKind::Newline);
        assert_eq!(tokens[4].kind, TokenKind::Pipe);
    }

    #[test]
    fn test_float_literal() {
        let mut lexer = Lexer::new("from t | filter x == 3.25");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[6].kind, TokenKind::Float(3.25));
    }

    #[test]
    fn test_big_integer_literal_keeps_its_value() {
        // Regression: 20+ digit integers used to corrupt their value when
        // overflowing i64 (the overflow fallback divided by 10^extra digits).
        // i64::MAX is 9223372036854775807; a 20-digit literal must still
        // carry the full magnitude (as a float fallback, not ~1/100th).
        let mut lexer = Lexer::new("from t | filter x == 99999999999999999999");
        let tokens = lexer.tokenize().unwrap();
        match &tokens[6].kind {
            TokenKind::Float(v) => {
                assert!(
                    (*v / 1e20 - 1.0).abs() < 1e-10,
                    "20-digit literal parsed as {v}, expected ~1e20"
                );
            }
            other => panic!("expected Float fallback for big literal, got {other:?}"),
        }
    }

    #[test]
    fn test_huge_integer_within_i64_stays_integer() {
        let mut lexer = Lexer::new("from t | filter x == 9223372036854775807");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[6].kind, TokenKind::Integer(9223372036854775807));
    }

    #[test]
    fn test_span_tracking() {
        let mut lexer = Lexer::new("from users");
        let tokens = lexer.tokenize().unwrap();
        assert_eq!(tokens[0].span, Span::new(0, 4));
        assert_eq!(tokens[1].span, Span::new(5, 10));
    }
}
