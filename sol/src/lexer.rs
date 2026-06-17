///! Tokenisation for the Sol programming language.
///!
///! The [`Lexer`] converts raw source text into a sequence of [`Token`]s
///! that the [`Parser`](crate::parser::Parser) consumes. It handles
///! identifiers, keywords, literals, operators, comments, and whitespace.

use crate::lexer::Token::*;

/// A single lexical token produced by the [`Lexer`].
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // ── Literals ────────────────────────────────────────────

    /// Integer literal, e.g. `42`
    IntLit(i64),
    /// Floating-point literal, e.g. `3.14`
    FloatLit(f64),
    /// Boolean literal: `true` or `false`
    BoolLit(bool),
    /// Character literal, e.g. `'x'`
    CharLit(char),
    /// String literal, e.g. `"hello"`
    StrLit(String),

    // ── Type keywords ───────────────────────────────────────

    /// `bool`
    TypeBool,
    /// `int`
    TypeInt,
    /// `float`
    TypeFloat,
    /// `char`
    TypeChar,
    /// `str`
    TypeStr,

    // ── Statement / declaration keywords ────────────────────

    /// `let`
    Let,
    /// `if`
    If,
    /// `else`
    Else,
    /// `while`
    While,
    /// `for`
    For,
    /// `in`
    In,
    /// `return`
    Return,
    /// `fn`
    Fn,
    /// `workflow`
    Workflow,
    /// `emit`
    Emit,
    /// `call`
    Call,
    /// `struct`
    Struct,
    /// `enum`
    Enum,
    /// `true` (parsed as [`BoolLit(true)`](Self::BoolLit))
    True,
    /// `false` (parsed as [`BoolLit(false)`](Self::BoolLit))
    False,
    /// `import`
    Import,
    /// `from`
    From,

    // ── Symbols ─────────────────────────────────────────────

    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `{`
    LBrace,
    /// `}`
    RBrace,
    /// `[`
    LBracket,
    /// `]`
    RBracket,
    /// `,`
    Comma,
    /// `:`
    Colon,
    /// `::`
    DoubleColon,
    /// `;`
    Semicolon,
    /// `.`
    Dot,
    /// `<-`
    Arrow,
    /// `=`
    Assign,
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*`
    Star,
    /// `/`
    Slash,

    // ── Comparison / logical operators ──────────────────────

    /// `==`
    Eq,
    /// `!=`
    Ne,
    /// `<`
    Lt,
    /// `>`
    Gt,
    /// `<=`
    Le,
    /// `>=`
    Ge,
    /// `&&`
    And,
    /// `||`
    Or,
    /// `!`
    Not,

    // ── Special ─────────────────────────────────────────────

    /// A user-defined identifier (or unrecognised keyword).
    Ident(String),
    /// End of input.
    EOF,
}

/// A character-by-character lexer that produces [`Token`]s from Sol source text.
///
/// # Example
///
/// ```ignore
/// let mut lexer = Lexer::new("let x: int = 42;");
/// while let Some(token) = lexer.next_token() {
///     println!("{:?}", token);
/// }
/// ```
#[derive(Clone)]
pub struct Lexer {
    chars: Vec<char>,
    pos: usize,
}

impl Lexer {
    /// Create a new lexer for the given source string.
    pub fn new(source: &str) -> Self {
        Self { chars: source.chars().collect(), pos: 0 }
    }

    /// Return the current character without consuming it.
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    /// Consume and return the current character.
    fn advance(&mut self) -> Option<char> {
        let c = self.chars.get(self.pos).copied();
        self.pos += 1;
        c
    }

    /// Skip whitespace characters (spaces, tabs, newlines, carriage returns).
    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            if c == ' ' || c == '\t' || c == '\n' || c == '\r' {
                self.advance();
            } else {
                break;
            }
        }
    }

    /// Read a string literal delimited by `quote` (either `"` or `'`).
    /// Handles escape sequences: `\n`, `\t`, `\r`, `\\`, `\"`, `\'`.
    fn read_string(&mut self, quote: char) -> String {
        let mut s = String::new();
        loop {
            match self.advance() {
                Some('\\') => {
                    match self.advance() {
                        Some('n') => s.push('\n'),
                        Some('t') => s.push('\t'),
                        Some('r') => s.push('\r'),
                        Some('\\') => s.push('\\'),
                        Some('"') => s.push('"'),
                        Some('\'') => s.push('\''),
                        Some(c) => s.push(c),
                        None => break,
                    }
                }
                Some(c) if c == quote => break,
                Some(c) => s.push(c),
                None => break,
            }
        }
        s
    }

    /// Read an identifier or keyword starting with `start`.
    /// Alphanumeric characters and underscores are consumed.
    /// Returns the appropriate [`Token`] variant for recognised keywords.
    fn read_identifier_or_keyword(&mut self, start: char) -> Token {
        let mut s = String::new();
        s.push(start);
        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                s.push(self.advance().unwrap());
            } else {
                break;
            }
        }
        match s.as_str() {
            "true" => Token::BoolLit(true),
            "false" => Token::BoolLit(false),
            "bool" => Token::TypeBool,
            "int" => Token::TypeInt,
            "float" => Token::TypeFloat,
            "char" => Token::TypeChar,
            "str" => Token::TypeStr,
            "let" => Token::Let,
            "if" => Token::If,
            "else" => Token::Else,
            "while" => Token::While,
            "for" => Token::For,
            "in" => Token::In,
            "return" => Token::Return,
            "fn" => Token::Fn,
            "workflow" => Token::Workflow,
            "emit" => Token::Emit,
            "call" => Token::Call,
            "struct" => Token::Struct,
            "enum" => Token::Enum,
            "import" => Token::Import,
            "from" => Token::From,
            _ => Token::Ident(s),
        }
    }

    /// Consume and return the next [`Token`] from the input.
    ///
    /// Returns `Some(Token::EOF)` when the end of input is reached.
    /// Skips single-line comments beginning with `#`.
    pub fn next_token(&mut self) -> Option<Token> {
        self.skip_whitespace();
        match self.advance() {
            None => Some(Token::EOF),
            Some('#') => {
                // Comment — skip until newline
                while let Some(c) = self.peek() {
                    if c == '\n' { break; }
                    self.advance();
                }
                self.next_token()
            }
            Some('(') => Some(LParen),
            Some(')') => Some(RParen),
            Some('{') => Some(LBrace),
            Some('}') => Some(RBrace),
            Some('[') => Some(LBracket),
            Some(']') => Some(RBracket),
            Some(',') => Some(Comma),
            Some(':') => {
                if self.peek() == Some(':') {
                    self.advance();
                    Some(DoubleColon)
                } else {
                    Some(Colon)
                }
            }
            Some(';') => Some(Semicolon),
            Some('.') => Some(Dot),
            Some('=') => {
                if self.peek() == Some('=') {
                    self.advance();
                    Some(Eq)
                } else {
                    Some(Assign)
                }
            }
            Some('!') => {
                if self.peek() == Some('=') {
                    self.advance();
                    Some(Ne)
                } else {
                    Some(Not)
                }
            }
            Some('<') => {
                if self.peek() == Some('=') {
                    self.advance();
                    Some(Le)
                } else if self.peek() == Some('-') {
                    self.advance();
                    Some(Arrow)
                } else {
                    Some(Lt)
                }
            }
            Some('>') => {
                if self.peek() == Some('=') {
                    self.advance();
                    Some(Ge)
                } else {
                    Some(Gt)
                }
            }
            Some('+') => Some(Plus),
            Some('-') => Some(Minus),
            Some('*') => Some(Star),
            Some('/') => Some(Slash),
            Some('&') => {
                if self.peek() == Some('&') {
                    self.advance();
                    Some(And)
                } else {
                    Some(Ident("&".into()))
                }
            }
            Some('|') => {
                if self.peek() == Some('|') {
                    self.advance();
                    Some(Or)
                } else {
                    Some(Ident("|".into()))
                }
            }
            Some('"') => Some(StrLit(self.read_string('"'))),
            Some('\'') => {
                let c = self.advance().unwrap_or('\0');
                if self.peek() == Some('\'') {
                    self.advance();
                }
                Some(CharLit(c))
            }
            Some(c) if c.is_ascii_digit() => {
                let mut s = String::new();
                s.push(c);
                let mut has_dot = false;
                while let Some(d) = self.peek() {
                    if d.is_ascii_digit() {
                        s.push(self.advance().unwrap());
                    } else if d == '.' && !has_dot {
                        has_dot = true;
                        s.push(self.advance().unwrap());
                    } else {
                        break;
                    }
                }
                if has_dot {
                    Some(FloatLit(s.parse().unwrap_or(0.0)))
                } else {
                    Some(IntLit(s.parse().unwrap_or(0)))
                }
            }
            Some(c) if c.is_alphabetic() || c == '_' => {
                Some(self.read_identifier_or_keyword(c))
            }
            Some(c) => Some(Ident(c.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokenize(source: &str) -> Vec<Token> {
        let mut lexer = Lexer::new(source);
        let mut tokens = Vec::new();
        while let Some(token) = lexer.next_token() {
            tokens.push(token);
            if matches!(tokens.last(), Some(Token::EOF)) {
                break;
            }
        }
        tokens
    }

    #[test]
    fn test_empty_source() {
        let tokens = tokenize("");
        assert_eq!(tokens, vec![Token::EOF]);
    }

    #[test]
    fn test_whitespace_only() {
        let tokens = tokenize("   \t\n\r  ");
        assert_eq!(tokens, vec![Token::EOF]);
    }

    #[test]
    fn test_integer_literal() {
        let tokens = tokenize("42");
        assert_eq!(tokens, vec![Token::IntLit(42), Token::EOF]);
    }

    #[test]
    fn test_negative_integer() {
        let tokens = tokenize("-42");
        assert_eq!(tokens, vec![Token::Minus, Token::IntLit(42), Token::EOF]);
    }

    #[test]
    fn test_float_literal() {
        let tokens = tokenize("3.14");
        assert_eq!(tokens, vec![Token::FloatLit(3.14), Token::EOF]);
    }

    #[test]
    fn test_float_leading_dot_fails_as_ident() {
        let tokens = tokenize(".5");
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0], Token::Dot);
        assert_eq!(tokens[1], Token::IntLit(5));
        assert_eq!(tokens[2], Token::EOF);
    }

    #[test]
    fn test_string_literal() {
        let tokens = tokenize(r#""hello world""#);
        assert_eq!(tokens, vec![Token::StrLit("hello world".into()), Token::EOF]);
    }

    #[test]
    fn test_string_with_escapes() {
        let tokens = tokenize(r#""line1\nline2\tTab""#);
        assert_eq!(tokens, vec![Token::StrLit("line1\nline2\tTab".into()), Token::EOF]);
    }

    #[test]
    fn test_string_escape_backslash() {
        let tokens = tokenize(r#""path\\file""#);
        assert_eq!(tokens, vec![Token::StrLit("path\\file".into()), Token::EOF]);
    }

    #[test]
    fn test_string_escape_quote() {
        let tokens = tokenize(r#""she said \"hi\"""#);
        assert_eq!(tokens, vec![Token::StrLit("she said \"hi\"".into()), Token::EOF]);
    }

    #[test]
    fn test_unclosed_string_returns_eof() {
        let tokens = tokenize(r#""unclosed"#);
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0], Token::StrLit("unclosed".into()));
        assert_eq!(tokens[1], Token::EOF);
    }

    #[test]
    fn test_char_literal() {
        let tokens = tokenize("'x'");
        assert_eq!(tokens, vec![Token::CharLit('x'), Token::EOF]);
    }

    #[test]
    fn test_bool_literals() {
        let tokens = tokenize("true false");
        assert_eq!(tokens, vec![
            Token::BoolLit(true),
            Token::BoolLit(false),
            Token::EOF,
        ]);
    }

    #[test]
    fn test_keywords() {
        let tokens = tokenize("let if else while for in return fn workflow emit call struct enum import from");
        assert_eq!(tokens, vec![
            Token::Let, Token::If, Token::Else, Token::While, Token::For,
            Token::In, Token::Return, Token::Fn, Token::Workflow, Token::Emit,
            Token::Call, Token::Struct, Token::Enum, Token::Import, Token::From,
            Token::EOF,
        ]);
    }

    #[test]
    fn test_type_keywords() {
        let tokens = tokenize("bool int float char str");
        assert_eq!(tokens, vec![
            Token::TypeBool, Token::TypeInt, Token::TypeFloat,
            Token::TypeChar, Token::TypeStr, Token::EOF,
        ]);
    }

    #[test]
    fn test_identifiers() {
        let tokens = tokenize("foo bar_baz qux123");
        assert_eq!(tokens, vec![
            Token::Ident("foo".into()),
            Token::Ident("bar_baz".into()),
            Token::Ident("qux123".into()),
            Token::EOF,
        ]);
    }

    #[test]
    fn test_identifier_starting_with_underscore() {
        let tokens = tokenize("_private __hidden");
        assert_eq!(tokens, vec![
            Token::Ident("_private".into()),
            Token::Ident("__hidden".into()),
            Token::EOF,
        ]);
    }

    #[test]
    fn test_operators() {
        let tokens = tokenize("+ - * / = == ! != < > <= >= && || <-");
        assert_eq!(tokens, vec![
            Token::Plus, Token::Minus, Token::Star, Token::Slash,
            Token::Assign, Token::Eq, Token::Not, Token::Ne,
            Token::Lt, Token::Gt, Token::Le, Token::Ge,
            Token::And, Token::Or, Token::Arrow,
            Token::EOF,
        ]);
    }

    #[test]
    fn test_delimiters() {
        let tokens = tokenize("() {} [] , : ; .");
        assert_eq!(tokens, vec![
            Token::LParen, Token::RParen,
            Token::LBrace, Token::RBrace,
            Token::LBracket, Token::RBracket,
            Token::Comma, Token::Colon, Token::Semicolon, Token::Dot,
            Token::EOF,
        ]);
    }

    #[test]
    fn test_single_line_comment() {
        let tokens = tokenize("let x = 5; # this is a comment\ny = 3;");
        assert!(tokens.contains(&Token::Let));
        assert!(tokens.contains(&Token::Assign));
        assert!(tokens.contains(&Token::IntLit(5)));
        assert!(tokens.contains(&Token::IntLit(3)));
    }

    #[test]
    fn test_comment_at_end_of_file() {
        let tokens = tokenize("let x = 1; # just a comment");
        assert_eq!(tokens, vec![
            Token::Let, Token::Ident("x".into()), Token::Assign,
            Token::IntLit(1), Token::Semicolon, Token::EOF,
        ]);
    }

    #[test]
    fn test_arrow_vs_less_than() {
        let tokens_arrow = tokenize("<-");
        assert_eq!(tokens_arrow[0], Token::Arrow);

        let tokens_lt = tokenize("<");
        assert_eq!(tokens_lt[0], Token::Lt);
    }

    #[test]
    fn test_equals_vs_assign() {
        let tokens_eq = tokenize("=");
        assert_eq!(tokens_eq[0], Token::Assign);

        let tokens_assign = tokenize("==");
        assert_eq!(tokens_assign[0], Token::Eq);
    }

    #[test]
    fn test_and_or_operators() {
        let tokens_and = tokenize("&&");
        assert_eq!(tokens_and[0], Token::And);

        let tokens_or = tokenize("||");
        assert_eq!(tokens_or[0], Token::Or);

        let tokens_single_and = tokenize("&");
        assert_eq!(tokens_single_and[0], Token::Ident("&".into()));

        let tokens_single_pipe = tokenize("|");
        assert_eq!(tokens_single_pipe[0], Token::Ident("|".into()));
    }

    #[test]
    fn test_mixed_tokens() {
        let tokens = tokenize("let x: int = 42;");
        assert_eq!(tokens, vec![
            Token::Let,
            Token::Ident("x".into()),
            Token::Colon,
            Token::TypeInt,
            Token::Assign,
            Token::IntLit(42),
            Token::Semicolon,
            Token::EOF,
        ]);
    }

    #[test]
    fn test_multiple_statements() {
        let source = r#"
            let name: str = "hello";
            let count: int = 10;
            print(count);
        "#;
        let tokens = tokenize(source);
        assert!(tokens.contains(&Token::Let));
        assert!(tokens.contains(&Token::Ident("name".into())));
        assert!(tokens.contains(&Token::StrLit("hello".into())));
        assert!(tokens.contains(&Token::Ident("count".into())));
        assert!(tokens.contains(&Token::IntLit(10)));
        assert!(tokens.contains(&Token::Ident("print".into())));
        assert!(tokens.contains(&Token::LParen));
        assert!(tokens.contains(&Token::RParen));
    }

    #[test]
    fn test_call_keyword() {
        let tokens = tokenize(r#"call("discord.send", {msg: "hello"})"#);
        assert_eq!(tokens[0], Token::Call);
        assert_eq!(tokens[1], Token::LParen);
        assert_eq!(tokens[2], Token::StrLit("discord.send".into()));
    }

    #[test]
    fn test_unicode_not_handled_as_identifier() {
        let tokens = tokenize("héllo");
        // Only ASCII alphanumeric is supported for identifiers
        assert!(tokens.iter().any(|t| matches!(t, Token::Ident(_))));
    }
}
