use std::collections::HashMap;

use crate::{
    analyzer::TypeTableId,
    diagnostic::{codes, DiagnosticPhase, SolDiagnostic, SourceSpan},
    lexer::{Token, TokenKind},
};

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Type {
    Void,
    Integer,
    Float,
    String,
    Char,
    Bool,

    Tuple(Vec<Type>),
    Array {
        size: Option<i128>,
        inner: Box<Type>,
    },
    Ident(String),
    Function {
        params: Vec<Type>,
        ret: Box<Type>,
    },
}

pub type Program = Vec<Ast>;
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Ast {
    DeclFunc {
        name: String,
        params: Vec<(String, Type)>,
        ret: Type,
        body: Box<Ast>,
        scope: TypeTableId,
    },
    DeclExtFunc {
        name: String,
        params: Vec<(String, Type)>,
        ret: Type,
    },
    DeclVar {
        name: String,
        kind: Type,
        value: Option<Box<Ast>>,
    },
    DeclStruct {
        name: String,
        fields: HashMap<String, Type>,
    },
    DeclEnum {
        name: String,
        variants: HashMap<String, isize>,
    },

    Block {
        block: Vec<Ast>,
        scope: TypeTableId,
    },
    StmtImport {
        #[allow(dead_code)]
        path: Vec<String>,
        alias: Option<String>,
    },
    StmtIf {
        condition: Box<Ast>,
        body: Box<Ast>,
        alt: Option<Box<Ast>>,
    },
    StmtWhile {
        condition: Box<Ast>,
        body: Box<Ast>,
    },
    StmtFor {
        elem_name: String,
        array: Box<Ast>,
        body: Box<Ast>,
    },

    #[allow(dead_code)]
    ExprAssign {
        var_name: String,
        value: Box<Ast>,
    },
    ExprBinary {
        lhs: Box<Ast>,
        rhs: Box<Ast>,
        op: Token,
    },
    ExprUnary {
        child: Box<Ast>,
        op: Token,
    },
    ExprFuncCall {
        name: String,
        args: Vec<Ast>,
    },
    ExprMemAcc {
        lhs: Box<Ast>,
        member: String,
    },
    ExprEnumVar {
        name: String,
        var: String,
    },
    ExprArrAcc {
        lhs: Box<Ast>,
        index: Box<Ast>,
    },
    ExprReturn {
        val: Option<Box<Ast>>,
    },
    ExprInteger(i128),
    ExprFloat(f64),
    ExprString(String),
    ExprChar(char),
    ExprBool(bool),
    ExprUndefined,
    ExprVar(String),
    ExprStructInit {
        name: String,
        fields: Vec<(String, Ast)>,
    },
    ExprArrayInit {
        values: Vec<Ast>,
    },
}

pub struct Parser {
    tokens: Vec<Token>,
    /// Source span of each token in `tokens`, populated by the
    /// lexer. Indexed in lockstep with `tokens`. Empty when the
    /// parser was constructed via the legacy `Parser::from(tokens)`
    /// path (which silently uses default spans). The current
    /// `parse_source` API always provides real spans.
    spans: Vec<SourceSpan>,
    index: usize,
    can_struct: bool,
    /// Collected diagnostics. The api layer drains this after
    /// `run()` completes.
    pub diagnostics: Vec<SolDiagnostic>,
    /// Set when a fatal-style error fires (something the upstream
    /// pre-B.2 code would have `process::exit(1)`ed for). The
    /// `run()` recovery loop synchronizes to the next top-level
    /// keyword and clears the flag, then keeps parsing so the user
    /// sees every error at once.
    had_fatal_error: bool,
}

macro_rules! noob {
    ($self: expr) => {
        $self.index < $self.tokens.len()
    };
}

impl Parser {
    /// Backwards-compatible constructor without spans. Diagnostics
    /// emitted by this parser instance will lack source spans.
    /// Prefer `Parser::from_with_spans` (and the `parse_source`
    /// API which uses it) for editor / IDE consumers.
    pub fn from(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            spans: Vec::new(),
            index: 0,
            can_struct: true,
            diagnostics: Vec::new(),
            had_fatal_error: false,
        }
    }

    /// Construct a parser with per-token source spans. Diagnostics
    /// emitted via `emit_error()` will carry the span of the
    /// current token (or a synthesized end-of-input span when past
    /// the last token).
    pub fn from_with_spans(tokens: Vec<Token>, spans: Vec<SourceSpan>) -> Self {
        debug_assert!(
            spans.is_empty() || spans.len() == tokens.len(),
            "spans must be parallel to tokens",
        );
        Self {
            tokens,
            spans,
            index: 0,
            can_struct: true,
            diagnostics: Vec::new(),
            had_fatal_error: false,
        }
    }

    /// Span of the token currently at `self.index`, or a one-byte
    /// synthesized span just past the last token's end if `index`
    /// is at / past end of input. Returns `None` when spans aren't
    /// available (legacy `Parser::from` path).
    fn current_span(&self) -> Option<SourceSpan> {
        if self.spans.is_empty() { return None; }
        if self.index < self.spans.len() {
            return Some(self.spans[self.index]);
        }
        // Past end of input — point at the very last byte the lexer
        // consumed so the editor can still highlight something
        // meaningful (typically the last `}` or `;`).
        let last = self.spans.last()?;
        Some(SourceSpan::new(last.end, last.end))
    }

    /// Push a structured diagnostic + set the fatal-error flag.
    /// Replaces the upstream `eprintln! + process::exit(1)` pattern.
    /// The `run()` recovery loop reads the flag, syncs forward, and
    /// resets it before trying the next declaration.
    ///
    /// Diagnostic span = span of the current token (or end-of-input
    /// synthesized span). All parser emit sites flow through this
    /// single helper so adding the span here covers every site.
    fn emit_error(&mut self, code: &'static str, message: impl Into<String>) {
        let mut d = SolDiagnostic::error(DiagnosticPhase::Parser, code, message);
        if let Some(span) = self.current_span() {
            d = d.with_span(span);
        }
        self.diagnostics.push(d);
        self.had_fatal_error = true;
    }

    /// Best-effort token-consume. On mismatch: push diagnostic,
    /// set fatal flag, DO NOT advance. The caller's downstream
    /// code may produce garbage AST; recovery happens in `run()`.
    fn eat(&mut self, tk: TokenKind, msg: &str) {
        if self.index >= self.tokens.len() {
            self.emit_error(codes::PARSE_MISSING_DELIMITER,
                format!("{msg} (reached end of input)"));
            return;
        }
        let tkcurr = self.tokens[self.index].get_kind();
        if tkcurr != tk {
            self.emit_error(codes::PARSE_MISSING_DELIMITER, msg.to_string());
            return;
        }
        self.index += 1;
    }
    fn current(&self) -> Token {
        self.tokens[self.index].clone()
    }
    fn advance(&mut self) -> Token {
        self.index += 1;
        self.tokens[self.index-1].clone()
    }

    /// Skip tokens until we hit a top-level keyword or end of
    /// input. Used by `run()` after a fatal parse error so the
    /// next declaration can be tried.
    fn sync_to_next_top_level(&mut self) {
        while self.index < self.tokens.len() {
            match self.tokens[self.index].get_kind() {
                TokenKind::Func
                | TokenKind::Ext
                | TokenKind::Let
                | TokenKind::Struct
                | TokenKind::Enum
                | TokenKind::Import => return,
                _ => self.index += 1,
            }
        }
    }

    pub fn run(&mut self) -> Program {
        let mut program: Program = Vec::new();
        while self.index < self.tokens.len() {
            // Snapshot the index before each attempt so we can
            // detect "no progress was made" cases — without this
            // a stuck recovery state could spin forever.
            let before = self.index;
            if let Some(decl) = self.declaration() {
                program.push(decl);
            }
            if self.had_fatal_error {
                self.sync_to_next_top_level();
                self.had_fatal_error = false;
                // If sync didn't move past the bad token, force a
                // single-token skip so we always make progress.
                if self.index == before && self.index < self.tokens.len() {
                    self.index += 1;
                }
            } else if self.index == before {
                // Defensive: if declaration() returned None without
                // advancing and no error fired, bail out so we don't
                // loop forever on an unrecognized state.
                break;
            }
        }
        program
    }
    fn declaration(&mut self) -> Option<Ast> {
        if self.index >= self.tokens.len() { return None; }
        match self.current().clone() {
            Token::Ext => self.ext_func_decl(),
            Token::Func => self.func_decl(),
            Token::Let => self.var_decl(),
            Token::Struct => self.struct_decl(),
            Token::Enum => self.enum_decl(),
            Token::Import => self.import_stmt(),
            x => {
                self.emit_error(
                    codes::PARSE_UNKNOWN_DECLARATION,
                    format!("unknown top-level construct: {x:?}"),
                );
                None
            }
        }
    }

    fn parse_type(&mut self) -> Option<Type> {
        match self.tokens[self.index].clone() {
            Token::Ident(ptype) => {
                let ty = match ptype.as_str() {
                    "int" => Some(Type::Integer),
                    "float" => Some(Type::Float),
                    "str" => Some(Type::String),
                    "char" => Some(Type::Char),
                    "bool" => Some(Type::Bool),
                    _ => Some(Type::Ident(ptype))
                };
                self.index += 1;
                ty
            }
            Token::LSquare => {
                self.index += 1;

                let size = if self.tokens[self.index].get_kind() != TokenKind::RSquare {
                    let Token::Integer(s) = self.tokens[self.index].clone() else {
                        self.emit_error(
                            codes::PARSE_BAD_ARRAY_SIZE,
                            "only integers can be used to specify an array size",
                        );
                        return None;
                    };
                    self.index += 1;
                    Some(s)
                } else { None };
                self.eat(TokenKind::RSquare, "expected `]` after array size");

                let inner = Box::new(self.parse_type()?);

                Some(Type::Array { size, inner })
            }
            Token::LParen => {
                self.index += 1;

                let mut types = Vec::new();
                while noob!(self) && !matches!(self.current(), Token::RParen) {
                    types.push(self.parse_type()?);
                    if matches!(self.current(), Token::Comma) {
                        self.index += 1;
                    } else {
                        break;
                    }
                }
                self.eat(TokenKind::RParen, "expected an `)` after tuple type");

                Some(Type::Tuple(types))
            }
            x => {
                self.emit_error(
                    codes::PARSE_INVALID_TYPE,
                    format!("`{x:?}` is not valid in a type specifier"),
                );
                None
            }
        }
    }

    fn ext_func_decl(&mut self) -> Option<Ast> {
        self.index += 1;
        self.eat(TokenKind::Func, "expected `function` keyword after `ext`");
        let Token::Ident(name) = self.tokens[self.index].clone() else {
            self.emit_error(
                codes::PARSE_BAD_NAME,
                "name expected after ext function keyword",
            );
            return None;
        };
        self.index += 1;
        self.eat(TokenKind::LParen, "expected left parenthesis after function name");

        let mut params = Vec::new();
        while noob!(self) && !matches!(self.tokens[self.index], Token::RParen) {
            let Token::Ident(pname) = self.tokens[self.index].clone() else {
                self.emit_error(codes::PARSE_BAD_PARAM, "expected parameter name");
                return None;
            };
            self.index += 1;
            self.eat(TokenKind::Colon, "expected colon after parameter name");
            let ptype = self.parse_type()?;
            params.push((pname, ptype));
            if self.tokens[self.index].get_kind() == TokenKind::Comma {
                self.index += 1;
            } else { break; }
        }
        self.eat(TokenKind::RParen, "expected right parenthesis after parameter list");

        let ret = if self.tokens[self.index].get_kind() == TokenKind::Arrow {
            self.index += 1;
            self.parse_type()?
        } else {
            Type::Void
        };

        self.eat(TokenKind::Semi, "expected semicolon after ext function declaration");

        Some(Ast::DeclExtFunc { name, params, ret })
    }

    fn func_decl(&mut self) -> Option<Ast> {
        self.index += 1;
        let Token::Ident(name) = self.tokens[self.index].clone() else {
            self.emit_error(codes::PARSE_BAD_NAME, "name expected after `function` keyword");
            return None;
        };
        self.index += 1;
        self.eat(TokenKind::LParen, "expected left parenthesis after function name");
        
        let mut params = Vec::new();
        while noob!(self) && !matches!(self.tokens[self.index], Token::RParen) {
            let Token::Ident(pname) = self.tokens[self.index].clone() else {
                self.emit_error(codes::PARSE_BAD_PARAM, "expected parameter name");
                return None;
            };
            self.index += 1;
            self.eat(TokenKind::Colon, "expected colon after parameter name");
            let ptype = self.parse_type()?;
            params.push((pname, ptype));
            if self.tokens[self.index].get_kind() == TokenKind::Comma {
                self.index += 1;
            } else { break; }
        }
        self.eat(TokenKind::RParen, "expected right parenthesis after parameter list");

        let ret = if self.tokens[self.index].get_kind() == TokenKind::Arrow {
            self.index += 1;
            self.parse_type()?
        } else {
            Type::Void
        };

        let body = Box::new(self.block()?);

        Some(Ast::DeclFunc { name, params, ret, body, scope: usize::MAX })
    }
    fn var_decl(&mut self) -> Option<Ast> {
        self.index += 1;

        let Token::Ident(name) = self.advance().clone() else {
            self.emit_error(codes::PARSE_BAD_NAME, "name expected in variable declaration");
            return None;
        };

        self.eat(TokenKind::Colon, "expected colon after variable name in a declaration");
        let kind = self.parse_type()?;

        let value = if matches!(self.current(), Token::Eq) {
            self.advance();
            Some(Box::new(self.expression()?))
        } else { None };

        self.eat(TokenKind::Semi, "expected semicolon at the end of a variable declaration");

        Some(Ast::DeclVar { name, kind, value })
    }

    fn block(&mut self) -> Option<Ast> {
        match self.tokens[self.index] {
            Token::LCurly => {
                self.index += 1;
                let mut stmts = Vec::new();
                while noob!(self) && !matches!(self.tokens[self.index], Token::RCurly) {
                    stmts.push(self.statement()?);
                }
                self.eat(TokenKind::RCurly, "left curly brace is never closed");
                Some(Ast::Block { block: stmts, scope: usize::MAX }) // scope isn't filled in until analysis
            }
            _ => self.statement(),
        }
    }
    fn statement(&mut self) -> Option<Ast> {
        match self.tokens[self.index].clone() {
            Token::For => self.for_stmt(),
            Token::If => self.if_stmt(),
            Token::Import => self.import_stmt(),
            Token::While => self.while_stmt(),
            Token::Let => self.var_decl(),
            Token::Return => self.return_stmt(),
            Token::LCurly => self.block(),
            x => {
                let expr = self.expression();
                if expr.is_some() {
                    self.eat(TokenKind::Semi, "expected semicolon to follow exprstmt");
                    expr
                } else {
                    self.emit_error(
                        codes::PARSE_BAD_STATEMENT,
                        format!("`{x:?}` is not the start of any known statement"),
                    );
                    None
                }
            }
        }
    }
    fn for_stmt(&mut self) -> Option<Ast> {
        self.index += 1;

        let Token::Ident(elem_name) = self.tokens[self.index].clone() else {
            self.emit_error(codes::PARSE_BAD_NAME, "variable name expected after `for` keyword");
            return None;
        };
        self.index += 1;

        self.eat(TokenKind::In, "expected `in` keyword to follow in a for declaration");

        let old = self.can_struct;
        self.can_struct = false;
        let array = Box::new(self.expression()?);
        self.can_struct = old;

        self.eat(TokenKind::LCurly, "expected `{` after for loop declaration");
        self.index -= 1;
        let body = Box::new(self.block()?);

        Some(Ast::StmtFor { elem_name, array, body })
    }
    fn if_stmt(&mut self) -> Option<Ast> {
        self.index += 1;

        let old = self.can_struct;
        self.can_struct = false;
        let condition = Box::new(self.expression()?);
        self.can_struct = old;

        // eprintln!("{condition:#?}");
        self.eat(TokenKind::LCurly, "expected `{` after if statement declaration");
        self.index -= 1;
        let body = Box::new(self.block()?);

        let alt = if matches!(self.tokens[self.index], Token::Else) {
            self.index += 1;
            Some(Box::new(self.block()?))
        } else { None };

        Some(Ast::StmtIf { condition, body, alt })
    }
    fn while_stmt(&mut self) -> Option<Ast> {
        self.index += 1;

        let old = self.can_struct;
        self.can_struct = false;
        let condition = Box::new(self.expression()?);
        self.can_struct = old;

        self.eat(TokenKind::LCurly, "expected `{` after while loop declaration");
        self.index -= 1;
        let body = Box::new(self.block()?);

        Some(Ast::StmtWhile { condition, body })
    }

    fn import_stmt(&mut self) -> Option<Ast> {
        self.index += 1;

        let mut path = Vec::new();
        {
            let Token::Ident(root) = self.tokens[self.index].clone() else {
                self.emit_error(codes::PARSE_BAD_IMPORT, "expected an identifier in an import path");
                return None;
            };
            self.index += 1;
            path.push(root);
        }
        while noob!(self) && self.tokens[self.index].get_kind() == TokenKind::Dot {
            self.index += 1;
            let Token::Ident(section) = self.tokens[self.index].clone() else {
                self.emit_error(codes::PARSE_BAD_IMPORT, "expected an identifier in an import path");
                return None;
            };
            self.index += 1;
            path.push(section);
        }

        let alias = if self.tokens[self.index].get_kind() == TokenKind::As {
            self.index += 1;
            let Token::Ident(section) = self.tokens[self.index].clone() else {
                self.emit_error(codes::PARSE_BAD_IMPORT, "expected an identifier for import to alias as");
                return None;
            };
            self.index += 1;
            Some(section)
        } else { None };

        self.eat(TokenKind::Semi, "expected semicolon at the end of an import statement");
        Some(Ast::StmtImport { path, alias })
    }
    fn return_stmt(&mut self) -> Option<Ast> {
        self.index += 1;

        let val = if matches!(self.current(), Token::Semi) {
            None
        } else {
            Some(Box::new(self.expression()?))
        };
        self.eat(TokenKind::Semi, "expected semicolon at the end of a return statement");

        Some(Ast::ExprReturn { val })
    }
    fn struct_decl(&mut self) -> Option<Ast> {
        self.index += 1;

        let Token::Ident(name) = self.tokens[self.index].clone() else {
            self.emit_error(codes::PARSE_BAD_NAME, "expected a name after keyword `struct`");
            return None;
        };
        self.index += 1;

        self.eat(TokenKind::LCurly, "expected `{` after struct declaration");

        let mut fields = HashMap::new();
        while noob!(self) && self.tokens[self.index].get_kind() != TokenKind::RCurly {
            let Token::Ident(fname) = self.tokens[self.index].clone() else {
                self.emit_error(codes::PARSE_BAD_FIELD, "expected identifier for a field name in struct declaration");
                return None;
            };
            self.index += 1;

            self.eat(TokenKind::Colon, "expected colon after field name");
            let fkind = self.parse_type()?;

            fields.insert(fname, fkind);
            if self.tokens[self.index].get_kind() == TokenKind::Comma {
                self.index += 1;
            } else { break; }
        }
        self.eat(TokenKind::RCurly, "expected `}` to close struct declaration");

        Some(Ast::DeclStruct { name, fields })
    }
    fn enum_decl(&mut self) -> Option<Ast> {
        self.index += 1;

        let Token::Ident(name) = self.tokens[self.index].clone() else {
            self.emit_error(codes::PARSE_BAD_NAME, "expected a name after keyword `enum`");
            return None;
        };
        self.index += 1;

        self.eat(TokenKind::LCurly, "expected `{` after enum declaration");

        let mut variants = HashMap::new();
        let mut iota = 0;
        while noob!(self) && self.tokens[self.index].get_kind() != TokenKind::RCurly {
            let Token::Ident(vname) = self.tokens[self.index].clone() else {
                self.emit_error(codes::PARSE_BAD_FIELD, "expected identifier for a member name in enum declaration");
                return None;
            };
            self.index += 1;

            if self.tokens[self.index].get_kind() == TokenKind::Eq {
                self.index += 1;
                let Token::Integer(viota) = self.tokens[self.index].clone() else {
                    self.emit_error(codes::PARSE_BAD_ENUM_VALUE, "expected an integer after equals sign in enum declaration");
                    return None;
                };
                self.index += 1;

                iota = viota as isize
            }

            variants.insert(vname, iota);
            iota += 1;
            if self.tokens[self.index].get_kind() == TokenKind::Comma {
                self.index += 1;
            } else { break; }
        }
        self.eat(TokenKind::RCurly, "expected `}` to close enum declaration");

        Some(Ast::DeclEnum { name, variants })
    }

    fn left_rec(&mut self, symbols: &[TokenKind], child: fn(&mut Parser) -> Option<Ast>) -> Option<Ast>{
        let mut lhs = child(self)?;

        while symbols.contains(&self.current().get_kind()) {
            let op = self.advance();
            let rhs = child(self)?;
            lhs = Ast::ExprBinary { lhs: Box::new(lhs), rhs: Box::new(rhs), op }
        }

        Some(lhs)
    }
    fn right_rec(&mut self,
        symbols: &[TokenKind],
        parent: fn(&mut Parser) -> Option<Ast>,
        child: fn(&mut Parser) -> Option<Ast>,
    ) -> Option<Ast> {
        let lhs = parent(self)?;

        Some(if symbols.contains(&self.current().get_kind()) {
            let op = self.advance();
            let rhs = child(self)?;
            Ast::ExprBinary { lhs: Box::new(lhs), rhs: Box::new(rhs), op: op }
        } else { lhs })
    }
    fn expression(&mut self) -> Option<Ast> { self.assignment() }
    fn assignment(&mut self) -> Option<Ast> { self.right_rec(&[TokenKind::Eq], Self::logic_or, Self::assignment) }
    fn logic_or(&mut self) -> Option<Ast> { self.left_rec(&[TokenKind::PipePipe], Self::logic_and) }
    fn logic_and(&mut self) -> Option<Ast> { self.left_rec(&[TokenKind::AmpAmp], Self::bitwise_or) }
    fn bitwise_or(&mut self) -> Option<Ast> { self.left_rec(&[TokenKind::Pipe], Self::bitwise_xor) }
    fn bitwise_xor(&mut self) -> Option<Ast> { self.left_rec(&[TokenKind::Caret], Self::bitwise_and) }
    fn bitwise_and(&mut self) -> Option<Ast> { self.left_rec(&[TokenKind::Ampersand], Self::equality) }
    fn equality(&mut self) -> Option<Ast> { self.left_rec(&[TokenKind::EqEq, TokenKind::BangEq], Self::relational) }
    fn relational(&mut self) -> Option<Ast> { self.left_rec(&[TokenKind::LessThan, TokenKind::LessEq, TokenKind::MoreThan, TokenKind::MoreEq], Self::shift) }
    fn shift(&mut self) -> Option<Ast> { self.left_rec(&[TokenKind::LShift, TokenKind::RShift], Self::additive) }
    fn additive(&mut self) -> Option<Ast> { self.left_rec(&[TokenKind::Plus, TokenKind::Dash], Self::multiplicative) }
    fn multiplicative(&mut self) -> Option<Ast> { self.left_rec(&[TokenKind::Star, TokenKind::Slash], Self::unary) }
    fn unary(&mut self) -> Option<Ast> {
        if [TokenKind::Bang,
            TokenKind::Dash,
            TokenKind::Tilde
        ].contains(&self.current().get_kind()) {
            let op = self.advance();
            Some(Ast::ExprUnary { child: Box::new(self.unary()?), op })
        } else { self.postfix() }
    }
    fn postfix(&mut self) -> Option<Ast> {
        let mut lhs = self.primary()?;

        while let ck = self.current().get_kind() && (ck == TokenKind::Dot || ck == TokenKind::LSquare) {
            match ck {
                TokenKind::Dot => {
                    self.advance();
                    let tok = self.advance();
                    let Token::Ident(rhs) = tok.clone() else {
                        self.emit_error(
                            codes::PARSE_BAD_MEMBER,
                            format!("`{tok:?}` is not a valid member"),
                        );
                        return None;
                    };
                    lhs = Ast::ExprMemAcc { lhs: Box::new(lhs), member: rhs };
                }
                TokenKind::LSquare => {
                    self.advance();
                    let index = self.expression()?;
                    self.eat(TokenKind::RSquare, "expected ']' to close array index");
                    lhs = Ast::ExprArrAcc { lhs: Box::new(lhs), index: Box::new(index) };
                }
                _ => unreachable!(),
            }
        }

        Some(lhs)
    }
    fn primary(&mut self) -> Option<Ast> {
        let kind = self.current().get_kind();

        let res = match kind {
            TokenKind::Integer => {
                if let Token::Integer(v) = self.advance() {
                    Some(Ast::ExprInteger(v))
                } else { None }
            }
            TokenKind::Float => {
                if let Token::Float(v) = self.advance() {
                    Some(Ast::ExprFloat(v))
                } else { None }
            }
            TokenKind::String => {
                if let Token::String(v) = self.advance() {
                    Some(Ast::ExprString(v))
                } else { None }
            }
            TokenKind::Char => {
                if let Token::Char(v) = self.advance() {
                    Some(Ast::ExprChar(v))
                } else { None }
            }
            TokenKind::True => {
                self.advance();
                Some(Ast::ExprBool(true))
            }
            TokenKind::False => {
                self.advance();
                Some(Ast::ExprBool(false))
            }
            TokenKind::Ident => {
                // Extract the name from the Token::Ident
                let name = if let Token::Ident(n) = self.advance() { n } else { unreachable!() };

                let next_kind = self.current().get_kind();

                if next_kind == TokenKind::LParen {
                    self.eat(TokenKind::LParen, "Expected '(' for function call");
                    let mut args = Vec::new();
                    if self.current().get_kind() != TokenKind::RParen {
                        loop {
                            args.push(self.expression()?);
                            if self.current().get_kind() == TokenKind::Comma {
                                self.advance();
                            } else {
                                break;
                            }
                        }
                    }
                    self.eat(TokenKind::RParen, "Expected ')' after arguments");
                    Some(Ast::ExprFuncCall { name, args })
                } else if self.can_struct && next_kind == TokenKind::LCurly {
                    self.eat(TokenKind::LCurly, "Expected '{' for struct initialization");
                    let mut fields = Vec::new();
                    while self.current().get_kind() != TokenKind::RCurly {
                        let field_token = self.advance();
                        if let Token::Ident(field_name) = field_token {
                            self.eat(TokenKind::Colon, "Expected ':' after field name");
                            let value = self.expression()?;
                            fields.push((field_name, value));
                            if self.current().get_kind() == TokenKind::Comma {
                                self.advance();
                            }
                        }
                    }
                    self.eat(TokenKind::RCurly, "Expected '}' after struct fields");
                    Some(Ast::ExprStructInit { name, fields })
                } else if next_kind == TokenKind::ColonColon {
                    self.advance();
                    let t = self.advance();
                    let var = if let Token::Ident(n) = t { n } else {
                        self.emit_error(
                            codes::PARSE_BAD_FIELD,
                            format!("{t:?} is not a valid enum variant"),
                        );
                        return None;
                    };
                    Some(Ast::ExprEnumVar { name, var })
                } else {
                    Some(Ast::ExprVar(name))
                }
            }
            TokenKind::LParen => {
                self.eat(TokenKind::LParen, "Expected '('");

                // Re-enable struct parsing inside parentheses
                let old_can_struct = self.can_struct;
                self.can_struct = true;

                let expr = self.expression();

                // Restore previous state (e.g., if we were in an 'if' condition)
                self.can_struct = old_can_struct;

                self.eat(TokenKind::RParen, "Expected ')' after expression");
                expr
            }
            TokenKind::LSquare => {
                self.advance();

                let mut exprs = Vec::new();
                while !matches!(self.current(), Token::RSquare) {
                    exprs.push(self.expression()?);
                    if self.tokens[self.index].get_kind() == TokenKind::Comma {
                        self.index += 1;
                    } else { break; }
                }
                self.eat(TokenKind::RSquare, "expected ']' to close an array initializer");

                Some(Ast::ExprArrayInit { values: exprs })
            }
            x => {
                self.emit_error(
                    codes::PARSE_NOT_EXPRESSION,
                    format!("`{x:?}` is not a valid expression start"),
                );
                None
            }
        };
        // Don't double-emit if the inner arm already pushed a
        // diagnostic; `had_fatal_error` is the marker.
        if res.is_none() && !self.had_fatal_error {
            self.emit_error(codes::PARSE_NOT_EXPRESSION, "could not parse expression");
        }
        res
    }
}
