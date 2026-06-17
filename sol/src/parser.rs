//! Recursive-descent parser for the Sol language.
//!
//! The [`Parser`] consumes tokens from a [`Lexer`] and
//! produces a [`crate::ast::Program`]. It implements a classic expression parser
//! with precedence climbing for operators.

use crate::ast::*;
use crate::lexer::{Lexer, Token};

/// A recursive-descent parser for Sol source code.
///
/// # Example
///
/// ```ignore
/// let mut parser = Parser::new("let x: int = 42;");
/// let program = parser.parse().expect("parse failed");
/// ```
pub struct Parser {
    lexer: Lexer,
    lookahead: Option<Token>,
}

impl Parser {
    /// Create a new parser for the given source string.
    ///
    /// The first token is read immediately (lookahead of one).
    pub fn new(source: &str) -> Self {
        let mut lexer = Lexer::new(source);
        let lookahead = lexer.next_token();
        Self { lexer, lookahead }
    }

    /// Consume and return the current lookahead token, advancing the lexer.
    fn next_token(&mut self) -> Option<Token> {
        let tok = self.lookahead.clone();
        self.lookahead = self.lexer.next_token();
        tok
    }

    /// Return a reference to the current lookahead token without consuming it.
    fn peek(&self) -> Option<&Token> {
        self.lookahead.as_ref()
    }

    /// Convert an expression to an assignment target.
    fn expr_to_target(expr: &Expr) -> Result<Target, String> {
        match expr {
            Expr::Ident(name) => Ok(Target::Ident(name.clone())),
            Expr::MemberAccess(obj, field) => {
                let target = Self::expr_to_target(obj)?;
                Ok(Target::MemberAccess(Box::new(target), field.clone()))
            }
            Expr::Index(obj, index) => {
                let target = Self::expr_to_target(obj)?;
                Ok(Target::Index(Box::new(target), index.clone()))
            }
            _ => Err(format!("invalid assignment target: {:?}", expr)),
        }
    }

    /// Expect the next token to match `expected`, returning an error otherwise.
    fn expect(&mut self, expected: &Token) -> Result<(), String> {
        let tok = self.next_token();
        if tok.as_ref() == Some(expected) {
            Ok(())
        } else {
            Err(format!("expected {:?}, got {:?}", expected, tok))
        }
    }

    /// Parse a complete Sol program.
    ///
    /// Repeatedly parses top-level items (functions, structs, enums, workflows,
    /// imports) until EOF.
    pub fn parse(&mut self) -> Result<Program, String> {
        let mut items = Vec::new();
        while !matches!(self.peek(), Some(Token::EOF) | None) {
            items.push(self.parse_top_level()?);
        }
        Ok(Program { items })
    }

    /// Parse a single top-level declaration based on the lookahead token.
    fn parse_top_level(&mut self) -> Result<TopLevel, String> {
        match self.peek() {
            Some(Token::Fn) => Ok(TopLevel::Function(self.parse_function()?)),
            Some(Token::Struct) => Ok(TopLevel::Struct(self.parse_struct()?)),
            Some(Token::Enum) => Ok(TopLevel::Enum(self.parse_enum()?)),
            Some(Token::Workflow) => Ok(TopLevel::Workflow(self.parse_workflow()?)),
            Some(Token::Import) => Ok(TopLevel::Import(self.parse_import()?)),
            Some(t) => Err(format!("unexpected top-level token {:?}", t)),
            None => Err("unexpected EOF".into()),
        }
    }

    /// Parse an import declaration:
    /// - `import module;`
    /// - `import "name" from module;`
    fn parse_import(&mut self) -> Result<ImportDecl, String> {
        self.expect(&Token::Import)?;
        let spec = if matches!(self.peek(), Some(Token::StrLit(_))) {
            let name = self.expect_str_lit()?;
            self.expect(&Token::From)?;
            let module = self.expect_ident()?;
            self.expect(&Token::Semicolon)?;
            ImportSpec::Named { name, module }
        } else {
            let module = self.expect_ident()?;
            self.expect(&Token::Semicolon)?;
            ImportSpec::Module(module)
        };
        Ok(ImportDecl { spec })
    }

    /// Parse a function declaration: `fn name(params) -> ReturnType { body }`
    fn parse_function(&mut self) -> Result<FunctionDecl, String> {
        self.expect(&Token::Fn)?;
        let name = self.expect_ident()?;
        self.expect(&Token::LParen)?;
        let params = self.parse_params()?;
        self.expect(&Token::RParen)?;
        let return_type = if matches!(self.peek(), Some(Token::Arrow)) {
            self.next_token();
            Some(self.parse_type()?)
        } else {
            None
        };
        let body = self.parse_block()?;
        Ok(FunctionDecl { name, params, return_type, body })
    }

    /// Parse a struct declaration: `struct Name { field: Type; … }`
    fn parse_struct(&mut self) -> Result<StructDecl, String> {
        self.expect(&Token::Struct)?;
        let name = self.expect_ident()?;
        self.expect(&Token::LBrace)?;
        let mut fields = Vec::new();
        while !matches!(self.peek(), Some(Token::RBrace) | None) {
            let field_name = self.expect_ident()?;
            self.expect(&Token::Colon)?;
            let type_ = self.parse_type()?;
            self.expect(&Token::Semicolon)?;
            fields.push(Field { name: field_name, type_ });
        }
        self.expect(&Token::RBrace)?;
        Ok(StructDecl { name, fields })
    }

    /// Parse an enum declaration: `enum Name { Variant1; Variant2; … }`
    fn parse_enum(&mut self) -> Result<EnumDecl, String> {
        self.expect(&Token::Enum)?;
        let name = self.expect_ident()?;
        self.expect(&Token::LBrace)?;
        let mut variants = Vec::new();
        while !matches!(self.peek(), Some(Token::RBrace) | None) {
            let v = self.expect_ident()?;
            self.expect(&Token::Semicolon)?;
            variants.push(v);
        }
        self.expect(&Token::RBrace)?;
        Ok(EnumDecl { name, variants })
    }

    /// Parse a workflow declaration: `workflow "name" { body }`
    fn parse_workflow(&mut self) -> Result<WorkflowDecl, String> {
        self.expect(&Token::Workflow)?;
        let name = self.expect_str_lit()?;
        let body = self.parse_block()?;
        Ok(WorkflowDecl { name, body })
    }

    /// Parse a block: `{ stmt; stmt; … }`
    fn parse_block(&mut self) -> Result<Block, String> {
        self.expect(&Token::LBrace)?;
        let mut stmts = Vec::new();
        while !matches!(self.peek(), Some(Token::RBrace) | None) {
            stmts.push(self.parse_stmt()?);
        }
        self.expect(&Token::RBrace)?;
        Ok(Block { stmts })
    }

    /// Parse a comma-separated parameter list.
    fn parse_params(&mut self) -> Result<Vec<Param>, String> {
        let mut params = Vec::new();
        while !matches!(self.peek(), Some(Token::RParen) | None | Some(Token::EOF)) {
            let name = self.expect_ident()?;
            self.expect(&Token::Colon)?;
            let type_ = self.parse_type()?;
            params.push(Param { name, type_ });
            if matches!(self.peek(), Some(Token::Comma)) {
                self.next_token();
            }
        }
        Ok(params)
    }

    /// Expect and return an identifier token.
    fn expect_ident(&mut self) -> Result<String, String> {
        match self.next_token() {
            Some(Token::Ident(name)) => Ok(name),
            Some(t) => Err(format!("expected identifier, got {:?}", t)),
            None => Err("expected identifier, got EOF".into()),
        }
    }

    /// Expect and return a string literal token.
    fn expect_str_lit(&mut self) -> Result<String, String> {
        match self.next_token() {
            Some(Token::StrLit(s)) => Ok(s),
            Some(t) => Err(format!("expected string literal, got {:?}", t)),
            None => Err("expected string literal, got EOF".into()),
        }
    }

    /// Parse a type annotation.
    fn parse_type(&mut self) -> Result<Type, String> {
        match self.next_token() {
            Some(Token::TypeBool) => Ok(Type::Bool),
            Some(Token::TypeInt) => Ok(Type::Int),
            Some(Token::TypeFloat) => Ok(Type::Float),
            Some(Token::TypeChar) => Ok(Type::Char),
            Some(Token::TypeStr) => Ok(Type::Str),
            Some(Token::LBracket) => {
                self.expect(&Token::RBracket)?;
                let inner = self.parse_type()?;
                Ok(Type::Array(Box::new(inner)))
            }
            Some(Token::Ident(name)) => Ok(Type::Named(name)),
            Some(t) => Err(format!("expected type, got {:?}", t)),
            None => Err("expected type, got EOF".into()),
        }
    }

    /// Parse a single statement.
    fn parse_stmt(&mut self) -> Result<Stmt, String> {
        match self.peek() {
            Some(Token::Let) => {
                self.next_token();
                let name = self.expect_ident()?;
                let type_ = if matches!(self.peek(), Some(Token::Colon)) {
                    self.next_token();
                    self.parse_type()?
                } else {
                    Type::Bool
                };
                self.expect(&Token::Assign)?;
                let value = self.parse_expr()?;
                if matches!(self.peek(), Some(Token::Semicolon)) {
                    self.next_token();
                }
                Ok(Stmt::Let { name, type_, value })
            }
            Some(Token::If) => {
                self.next_token();
                self.expect(&Token::LParen)?;
                let condition = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                let then = self.parse_block()?;
                let else_ = if matches!(self.peek(), Some(Token::Else)) {
                    self.next_token();
                    Some(self.parse_block()?)
                } else { None };
                Ok(Stmt::If { condition, then, else_ })
            }
            Some(Token::While) => {
                self.next_token();
                self.expect(&Token::LParen)?;
                let condition = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                let body = self.parse_block()?;
                Ok(Stmt::While { condition, body })
            }
            Some(Token::For) => {
                self.next_token();
                let item = self.expect_ident()?;
                self.expect(&Token::In)?;
                let iter = self.parse_expr()?;
                let body = self.parse_block()?;
                Ok(Stmt::For { item, iter, body })
            }
            Some(Token::Return) => {
                self.next_token();
                if matches!(self.peek(), Some(Token::Semicolon)) {
                    self.next_token();
                    Ok(Stmt::Return(None))
                } else {
                    let value = self.parse_expr()?;
                    if matches!(self.peek(), Some(Token::Semicolon)) {
                        self.next_token();
                    }
                    Ok(Stmt::Return(Some(value)))
                }
            }
            Some(Token::Emit) => {
                self.next_token();
                let event = self.expect_str_lit()?;
                if matches!(self.peek(), Some(Token::Semicolon)) {
                    self.next_token();
                }
                Ok(Stmt::Emit(event))
            }
            Some(Token::LBrace) => {
                Err("unexpected block as statement".into())
            }
            _ => {
                let expr = self.parse_expr()?;
                if matches!(self.peek(), Some(Token::Assign)) {
                    // Assignment: `target = value;`
                    let target = Self::expr_to_target(&expr)?;
                    self.next_token();
                    let value = self.parse_expr()?;
                    if matches!(self.peek(), Some(Token::Semicolon)) {
                        self.next_token();
                    }
                    Ok(Stmt::Assign { target, value })
                } else {
                    if matches!(self.peek(), Some(Token::Semicolon)) {
                        self.next_token();
                    }
                    Ok(Stmt::Expr(expr))
                }
            }
        }
    }

    /// Entry point for expression parsing (lowest precedence).
    fn parse_expr(&mut self) -> Result<Expr, String> {
        self.parse_or()
    }

    /// Parse `||` (logical or).
    fn parse_or(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_and()?;
        while matches!(self.peek(), Some(Token::Or)) {
            self.next_token();
            let right = self.parse_and()?;
            left = Expr::BinOp(Box::new(left), BinOp::Or, Box::new(right));
        }
        Ok(left)
    }

    /// Parse `&&` (logical and).
    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_comparison()?;
        while matches!(self.peek(), Some(Token::And)) {
            self.next_token();
            let right = self.parse_comparison()?;
            left = Expr::BinOp(Box::new(left), BinOp::And, Box::new(right));
        }
        Ok(left)
    }

    /// Parse comparison operators: `==`, `!=`, `<`, `>`, `<=`, `>=`.
    fn parse_comparison(&mut self) -> Result<Expr, String> {
        let left = self.parse_term()?;
        match self.peek() {
            Some(Token::Eq) => { self.next_token(); let r = self.parse_term()?; Ok(Expr::BinOp(Box::new(left), BinOp::Eq, Box::new(r))) }
            Some(Token::Ne) => { self.next_token(); let r = self.parse_term()?; Ok(Expr::BinOp(Box::new(left), BinOp::Ne, Box::new(r))) }
            Some(Token::Lt) => { self.next_token(); let r = self.parse_term()?; Ok(Expr::BinOp(Box::new(left), BinOp::Lt, Box::new(r))) }
            Some(Token::Gt) => { self.next_token(); let r = self.parse_term()?; Ok(Expr::BinOp(Box::new(left), BinOp::Gt, Box::new(r))) }
            Some(Token::Le) => { self.next_token(); let r = self.parse_term()?; Ok(Expr::BinOp(Box::new(left), BinOp::Le, Box::new(r))) }
            Some(Token::Ge) => { self.next_token(); let r = self.parse_term()?; Ok(Expr::BinOp(Box::new(left), BinOp::Ge, Box::new(r))) }
            _ => Ok(left),
        }
    }

    /// Parse additive operators: `+`, `-`.
    fn parse_term(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_factor()?;
        loop {
            match self.peek() {
                Some(Token::Plus) => { self.next_token(); let r = self.parse_factor()?; left = Expr::BinOp(Box::new(left), BinOp::Add, Box::new(r)); }
                Some(Token::Minus) => { self.next_token(); let r = self.parse_factor()?; left = Expr::BinOp(Box::new(left), BinOp::Sub, Box::new(r)); }
                _ => break,
            }
        }
        Ok(left)
    }

    /// Parse multiplicative operators: `*`, `/`.
    fn parse_factor(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_unary()?;
        loop {
            match self.peek() {
                Some(Token::Star) => { self.next_token(); let r = self.parse_unary()?; left = Expr::BinOp(Box::new(left), BinOp::Mul, Box::new(r)); }
                Some(Token::Slash) => { self.next_token(); let r = self.parse_unary()?; left = Expr::BinOp(Box::new(left), BinOp::Div, Box::new(r)); }
                _ => break,
            }
        }
        Ok(left)
    }

    /// Parse unary operators: `-`, `!`.
    fn parse_unary(&mut self) -> Result<Expr, String> {
        match self.peek() {
            Some(Token::Minus) => { self.next_token(); let e = self.parse_unary()?; Ok(Expr::UnaryOp(Box::new(e), UnaryOp::Neg)) }
            Some(Token::Not) => { self.next_token(); let e = self.parse_unary()?; Ok(Expr::UnaryOp(Box::new(e), UnaryOp::Not)) }
            _ => self.parse_postfix(),
        }
    }

    /// Parse postfix operators: `.field`, `[index]`, `(args)`.
    fn parse_postfix(&mut self) -> Result<Expr, String> {
        let mut left = self.parse_primary()?;
        loop {
            match self.peek() {
                Some(Token::Dot) => {
                    self.next_token();
                    let field = self.expect_ident()?;
                    left = Expr::MemberAccess(Box::new(left), field);
                }
                Some(Token::LBracket) => {
                    self.next_token();
                    let index = self.parse_expr()?;
                    self.expect(&Token::RBracket)?;
                    left = Expr::Index(Box::new(left), Box::new(index));
                }
                Some(Token::DoubleColon) => {
                    self.next_token();
                    let name = self.expect_ident()?;
                    if matches!(self.peek(), Some(Token::LParen)) {
                        // `expr::name(args)` — namespace call
                        self.next_token();
                        let mut args = Vec::new();
                        while !matches!(self.peek(), Some(Token::RParen) | None) {
                            args.push(self.parse_expr()?);
                            if matches!(self.peek(), Some(Token::Comma)) {
                                self.next_token();
                            }
                        }
                        self.expect(&Token::RParen)?;
                        left = Expr::NamespaceCall {
                            namespace: Box::new(left),
                            name,
                            args,
                        };
                    } else {
                        // `EnumName::Variant` — enum variant
                        let enum_name = match left {
                            Expr::Ident(s) => s,
                            _ => return Err("expected enum name before `::`".into()),
                        };
                        left = Expr::EnumVariant { enum_name, variant: name };
                    }
                }
                Some(Token::LParen) => {
                    self.next_token();
                    let mut args = Vec::new();
                    while !matches!(self.peek(), Some(Token::RParen) | None) {
                        args.push(self.parse_expr()?);
                        if matches!(self.peek(), Some(Token::Comma)) {
                            self.next_token();
                        }
                    }
                    self.expect(&Token::RParen)?;
                    left = Expr::Call(Box::new(left), args);
                }
                _ => break,
            }
        }
        Ok(left)
    }

    /// Parse a primary expression (literals, identifiers, parenthesised, etc.).
    fn parse_primary(&mut self) -> Result<Expr, String> {
        match self.next_token() {
            Some(Token::IntLit(n)) => Ok(Expr::Int(n)),
            Some(Token::FloatLit(n)) => Ok(Expr::Float(n)),
            Some(Token::BoolLit(b)) => Ok(Expr::Bool(b)),
            Some(Token::CharLit(c)) => Ok(Expr::Char(c)),
            Some(Token::StrLit(s)) => Ok(Expr::Str(s)),
            Some(Token::LParen) => {
                let expr = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(expr)
            }
            Some(Token::LBracket) => {
                let mut elements = Vec::new();
                while !matches!(self.peek(), Some(Token::RBracket) | None) {
                    elements.push(self.parse_expr()?);
                    if matches!(self.peek(), Some(Token::Comma)) {
                        self.next_token();
                    }
                }
                self.expect(&Token::RBracket)?;
                Ok(Expr::Array(elements))
            }
            Some(Token::Call) => {
                self.expect(&Token::LParen)?;
                let capability_expr = self.parse_expr()?;
                self.expect(&Token::Comma)?;
                let params = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(Expr::WorkflowCall {
                    capability_expr: Box::new(capability_expr),
                    params: Box::new(params),
                })
            }
            Some(Token::Ident(name)) => {
                if matches!(self.peek(), Some(Token::LBrace)) {
                    // Peek ahead to distinguish `Ident { field: ... }` (struct instance)
                    // from `Ident { stmt; ... }` (block, e.g. for-loop body).
                    let is_struct = {
                        let mut lex = self.lexer.clone();
                        match lex.next_token() {
                            Some(Token::RBrace) => true,
                            Some(Token::Ident(_)) | Some(Token::StrLit(_)) => {
                                matches!(lex.next_token(), Some(Token::Colon))
                            }
                            _ => false,
                        }
                    };
                    if is_struct {
                        self.next_token(); // consume {
                        let mut fields = Vec::new();
                        while !matches!(self.peek(), Some(Token::RBrace) | None) {
                            let field_name = match self.next_token() {
                                Some(Token::Ident(n)) => n,
                                Some(Token::StrLit(s)) => s,
                                Some(t) => return Err(format!("expected field name, got {:?}", t)),
                                None => return Err("expected field name".into()),
                            };
                            self.expect(&Token::Colon)?;
                            let value = self.parse_expr()?;
                            fields.push((field_name, value));
                            if matches!(self.peek(), Some(Token::Comma)) {
                                self.next_token();
                            }
                        }
                        self.expect(&Token::RBrace)?;
                        Ok(Expr::StructInstance { name, fields })
                    } else {
                        Ok(Expr::Ident(name))
                    }
                } else {
                    Ok(Expr::Ident(name))
                }
            }
            Some(Token::LBrace) => {
                let name = String::new();
                let mut fields = Vec::new();
                while !matches!(self.peek(), Some(Token::RBrace) | None) {
                    let field_name = match self.next_token() {
                        Some(Token::Ident(n)) => n,
                        Some(Token::StrLit(s)) => s,
                        Some(t) => return Err(format!("expected field name, got {:?}", t)),
                        None => return Err("expected field name".into()),
                    };
                    self.expect(&Token::Colon)?;
                    let value = self.parse_expr()?;
                    fields.push((field_name, value));
                    if matches!(self.peek(), Some(Token::Comma)) {
                        self.next_token();
                    }
                }
                self.expect(&Token::RBrace)?;
                Ok(Expr::StructInstance { name, fields })
            }
            Some(t) => Err(format!("unexpected token {:?}", t)),
            None => Err("unexpected EOF".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(source: &str) -> Result<Program, String> {
        let mut parser = Parser::new(source);
        parser.parse()
    }

    fn parse_expr(source: &str) -> Result<Expr, String> {
        // Wrap in a workflow so we can test expression parsing via parse_stmt
        let wrapped = format!("workflow \"test\" {{ let _: bool = {}; }}", source);
        let mut parser = Parser::new(&wrapped);
        let program = parser.parse()?;
        if let Some(TopLevel::Workflow(wf)) = program.items.into_iter().next() {
            if let Some(Stmt::Let { value, .. }) = wf.body.stmts.into_iter().next() {
                return Ok(value);
            }
        }
        Err("could not extract expression".into())
    }

    #[test]
    fn test_empty_program() {
        let program = parse("").unwrap();
        assert!(program.items.is_empty());
    }

    #[test]
    fn test_workflow_declaration() {
        let source = r#"workflow "hello" { print("hi"); }"#;
        let program = parse(source).unwrap();
        assert_eq!(program.items.len(), 1);
        match &program.items[0] {
            TopLevel::Workflow(wf) => {
                assert_eq!(wf.name, "hello");
                assert_eq!(wf.body.stmts.len(), 1);
            }
            _ => panic!("expected workflow"),
        }
    }

    #[test]
    fn test_function_declaration() {
        let source = r#"fn add(a: int, b: int) <- int { return a + b; }"#;
        let program = parse(source).unwrap();
        assert_eq!(program.items.len(), 1);
        match &program.items[0] {
            TopLevel::Function(f) => {
                assert_eq!(f.name, "add");
                assert_eq!(f.params.len(), 2);
                assert!(f.return_type.is_some());
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_function_no_return_type() {
        let source = r#"fn greet(name: str) { print(name); }"#;
        let program = parse(source).unwrap();
        match &program.items[0] {
            TopLevel::Function(f) => {
                assert!(f.return_type.is_none());
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn test_struct_declaration() {
        let source = r#"struct Point { x: int; y: int; }"#;
        let program = parse(source).unwrap();
        match &program.items[0] {
            TopLevel::Struct(s) => {
                assert_eq!(s.name, "Point");
                assert_eq!(s.fields.len(), 2);
            }
            _ => panic!("expected struct"),
        }
    }

    #[test]
    fn test_empty_struct() {
        let source = r#"struct Empty {}"#;
        let program = parse(source).unwrap();
        match &program.items[0] {
            TopLevel::Struct(s) => {
                assert!(s.fields.is_empty());
            }
            _ => panic!("expected struct"),
        }
    }

    #[test]
    fn test_enum_declaration() {
        let source = r#"enum Color { Red; Green; Blue; }"#;
        let program = parse(source).unwrap();
        match &program.items[0] {
            TopLevel::Enum(e) => {
                assert_eq!(e.name, "Color");
                assert_eq!(e.variants, vec!["Red", "Green", "Blue"]);
            }
            _ => panic!("expected enum"),
        }
    }

    #[test]
    fn test_empty_enum() {
        let source = r#"enum Nothing {}"#;
        let program = parse(source).unwrap();
        match &program.items[0] {
            TopLevel::Enum(e) => {
                assert!(e.variants.is_empty());
            }
            _ => panic!("expected enum"),
        }
    }

    #[test]
    fn test_import_module() {
        let source = r#"import alpha;"#;
        let program = parse(source).unwrap();
        match &program.items[0] {
            TopLevel::Import(i) => {
                match &i.spec {
                    ImportSpec::Module(name) => assert_eq!(name, "alpha"),
                    _ => panic!("expected module import"),
                }
            }
            _ => panic!("expected import"),
        }
    }

    #[test]
    fn test_import_named() {
        let source = r#"import "send" from discord;"#;
        let program = parse(source).unwrap();
        match &program.items[0] {
            TopLevel::Import(i) => {
                match &i.spec {
                    ImportSpec::Named { name, module } => {
                        assert_eq!(name, "send");
                        assert_eq!(module, "discord");
                    }
                    _ => panic!("expected named import"),
                }
            }
            _ => panic!("expected import"),
        }
    }

    #[test]
    fn test_let_statement_with_type() {
        let source = r#"workflow "test" { let x: int = 42; }"#;
        let program = parse(source).unwrap();
        let wf = extract_workflow(&program);
        match &wf.body.stmts[0] {
            Stmt::Let { name, type_, value } => {
                assert_eq!(name, "x");
                assert_eq!(*type_, Type::Int);
                assert!(matches!(value, Expr::Int(42)));
            }
            _ => panic!("expected let"),
        }
    }

    #[test]
    fn test_let_statement_no_type_defaults_to_bool() {
        let source = r#"workflow "test" { let x = true; }"#;
        let program = parse(source).unwrap();
        let wf = extract_workflow(&program);
        match &wf.body.stmts[0] {
            Stmt::Let { name, type_, .. } => {
                assert_eq!(name, "x");
                assert_eq!(*type_, Type::Bool);
            }
            _ => panic!("expected let"),
        }
    }

    #[test]
    fn test_if_else() {
        let source = r#"workflow "test" { if (x > 0) { let a: int = 1; } else { let b: int = 2; } }"#;
        let program = parse(source).unwrap();
        let wf = extract_workflow(&program);
        match &wf.body.stmts[0] {
            Stmt::If { condition, then, else_ } => {
                assert!(else_.is_some());
                assert_eq!(then.stmts.len(), 1);
                assert_eq!(else_.as_ref().unwrap().stmts.len(), 1);
            }
            _ => panic!("expected if"),
        }
    }

    #[test]
    fn test_if_no_else() {
        let source = r#"workflow "test" { if (x > 0) { let a: int = 1; } }"#;
        let program = parse(source).unwrap();
        let wf = extract_workflow(&program);
        match &wf.body.stmts[0] {
            Stmt::If { condition: _, then: _, else_ } => {
                assert!(else_.is_none());
            }
            _ => panic!("expected if"),
        }
    }

    #[test]
    fn test_while_loop() {
        let source = r#"workflow "test" { while (count > 0) { let count: int = count - 1; } }"#;
        let program = parse(source).unwrap();
        let wf = extract_workflow(&program);
        match &wf.body.stmts[0] {
            Stmt::While { condition: _, body } => {
                assert_eq!(body.stmts.len(), 1);
            }
            _ => panic!("expected while"),
        }
    }

    #[test]
    fn test_for_loop() {
        let source = r#"workflow "test" { for i in [1, 2, 3] { print(i); } }"#;
        let program = parse(source).unwrap();
        let wf = extract_workflow(&program);
        match &wf.body.stmts[0] {
            Stmt::For { item, iter, body } => {
                assert_eq!(item, "i");
                assert_eq!(body.stmts.len(), 1);
            }
            _ => panic!("expected for"),
        }
    }

    #[test]
    fn test_return_with_value() {
        let source = r#"workflow "test" { return 42; }"#;
        let program = parse(source).unwrap();
        let wf = extract_workflow(&program);
        match &wf.body.stmts[0] {
            Stmt::Return(Some(_)) => {}
            _ => panic!("expected return with value"),
        }
    }

    #[test]
    fn test_return_void() {
        let source = r#"workflow "test" { return; }"#;
        let program = parse(source).unwrap();
        let wf = extract_workflow(&program);
        match &wf.body.stmts[0] {
            Stmt::Return(None) => {}
            _ => panic!("expected return without value"),
        }
    }

    #[test]
    fn test_emit_statement() {
        let source = r#"workflow "test" { emit "event.occured"; }"#;
        let program = parse(source).unwrap();
        let wf = extract_workflow(&program);
        match &wf.body.stmts[0] {
            Stmt::Emit(event) => {
                assert_eq!(event, "event.occured");
            }
            _ => panic!("expected emit"),
        }
    }

    #[test]
    fn test_call_expression() {
        let source = r#"workflow "test" { let x: str = call("discord.send", {msg: "hi"}); }"#;
        let program = parse(source).unwrap();
        let wf = extract_workflow(&program);
        match &wf.body.stmts[0] {
            Stmt::Let { value, .. } => {
                match value {
                    Expr::WorkflowCall { capability_expr, params } => {
                        assert_eq!(**capability_expr, Expr::Str("discord.send".into()));
                    }
                    _ => panic!("expected workflow call"),
                }
            }
            _ => panic!("expected let"),
        }
    }

    #[test]
    fn test_expression_precedence() {
        // a + b * c should parse as a + (b * c)
        let source = r#"workflow "test" { let _: bool = a + b * c; }"#;
        let program = parse(source).unwrap();
        let wf = extract_workflow(&program);
        match &wf.body.stmts[0] {
            Stmt::Let { value, .. } => {
                match value {
                    Expr::BinOp(left, BinOp::Add, right) => {
                        match &**right {
                            Expr::BinOp(_, BinOp::Mul, _) => {} // correct
                            _ => panic!("expected multiplication as right operand of addition"),
                        }
                    }
                    _ => panic!("expected binary operation"),
                }
            }
            _ => panic!("expected let"),
        }
    }

    #[test]
    fn test_comparison_chaining() {
        let source = r#"workflow "test" { let _: bool = a == b && c < d; }"#;
        let program = parse(source).unwrap();
        let wf = extract_workflow(&program);
        match &wf.body.stmts[0] {
            Stmt::Let { value, .. } => {
                match value {
                    Expr::BinOp(_, BinOp::And, _) => {} // correct
                    _ => panic!("expected AND as top-level operator"),
                }
            }
            _ => panic!("expected let"),
        }
    }

    #[test]
    fn test_unary_negation() {
        let source = r#"workflow "test" { let _: int = -42; }"#;
        let program = parse(source).unwrap();
        let wf = extract_workflow(&program);
        match &wf.body.stmts[0] {
            Stmt::Let { value, .. } => {
                match value {
                    Expr::UnaryOp(operand, UnaryOp::Neg) => {
                        assert_eq!(**operand, Expr::Int(42));
                    }
                    _ => panic!("expected unary negation"),
                }
            }
            _ => panic!("expected let"),
        }
    }

    #[test]
    fn test_logical_not() {
        let source = r#"workflow "test" { let _: bool = !flag; }"#;
        let program = parse(source).unwrap();
        let wf = extract_workflow(&program);
        match &wf.body.stmts[0] {
            Stmt::Let { value, .. } => {
                match value {
                    Expr::UnaryOp(_, UnaryOp::Not) => {} // correct
                    _ => panic!("expected logical not"),
                }
            }
            _ => panic!("expected let"),
        }
    }

    #[test]
    fn test_member_access() {
        let source = r#"workflow "test" { let _: str = obj.field; }"#;
        let program = parse(source).unwrap();
        let wf = extract_workflow(&program);
        match &wf.body.stmts[0] {
            Stmt::Let { value, .. } => {
                match value {
                    Expr::MemberAccess(obj, field) => {
                        assert_eq!(field, "field");
                    }
                    _ => panic!("expected member access"),
                }
            }
            _ => panic!("expected let"),
        }
    }

    #[test]
    fn test_array_index() {
        let source = r#"workflow "test" { let _: int = arr[0]; }"#;
        let program = parse(source).unwrap();
        let wf = extract_workflow(&program);
        match &wf.body.stmts[0] {
            Stmt::Let { value, .. } => {
                match value {
                    Expr::Index(arr, index) => {
                        assert_eq!(**index, Expr::Int(0));
                    }
                    _ => panic!("expected index expression"),
                }
            }
            _ => panic!("expected let"),
        }
    }

    #[test]
    fn test_struct_instance() {
        let source = r#"workflow "test" { let p: Point = Point { x: 10, y: 20 }; }"#;
        let program = parse(source).unwrap();
        let wf = extract_workflow(&program);
        match &wf.body.stmts[0] {
            Stmt::Let { value, .. } => {
                match value {
                    Expr::StructInstance { name, fields } => {
                        assert_eq!(name, "Point");
                        assert_eq!(fields.len(), 2);
                    }
                    _ => panic!("expected struct instance"),
                }
            }
            _ => panic!("expected let"),
        }
    }

    #[test]
    fn test_anonymous_struct_instance() {
        let source = r#"workflow "test" { let _ = { x: 10, y: 20 }; }"#;
        let program = parse(source).unwrap();
        let wf = extract_workflow(&program);
        match &wf.body.stmts[0] {
            Stmt::Let { value, .. } => {
                match value {
                    Expr::StructInstance { name, fields } => {
                        assert!(name.is_empty(), "anonymous struct should have empty name");
                        assert_eq!(fields.len(), 2);
                    }
                    _ => panic!("expected anonymous struct instance"),
                }
            }
            _ => panic!("expected let"),
        }
    }

    #[test]
    fn test_function_call_expression() {
        let source = r#"workflow "test" { let result: int = double(21); }"#;
        let program = parse(source).unwrap();
        let wf = extract_workflow(&program);
        match &wf.body.stmts[0] {
            Stmt::Let { value, .. } => {
                match value {
                    Expr::Call(callee, args) => {
                        assert_eq!(args.len(), 1);
                        match &**callee {
                            Expr::Ident(name) => assert_eq!(name, "double"),
                            _ => panic!("expected identifier callee"),
                        }
                    }
                    _ => panic!("expected function call"),
                }
            }
            _ => panic!("expected let"),
        }
    }

    #[test]
    fn test_array_literal() {
        let source = r#"workflow "test" { let arr: []int = [1, 2, 3]; }"#;
        let program = parse(source).unwrap();
        let wf = extract_workflow(&program);
        match &wf.body.stmts[0] {
            Stmt::Let { value, .. } => {
                match value {
                    Expr::Array(elements) => {
                        assert_eq!(elements.len(), 3);
                    }
                    _ => panic!("expected array"),
                }
            }
            _ => panic!("expected let"),
        }
    }

    #[test]
    fn test_empty_array() {
        let source = r#"workflow "test" { let arr: []int = []; }"#;
        let program = parse(source).unwrap();
        let wf = extract_workflow(&program);
        match &wf.body.stmts[0] {
            Stmt::Let { value, .. } => {
                match value {
                    Expr::Array(elements) => {
                        assert!(elements.is_empty());
                    }
                    _ => panic!("expected empty array"),
                }
            }
            _ => panic!("expected let"),
        }
    }

    #[test]
    fn test_parenthesized_expression() {
        let source = r#"workflow "test" { let _: int = (2 + 3) * 4; }"#;
        let program = parse(source).unwrap();
        let wf = extract_workflow(&program);
        match &wf.body.stmts[0] {
            Stmt::Let { value, .. } => {
                match value {
                    Expr::BinOp(left, BinOp::Mul, _) => {
                        match &**left {
                            Expr::BinOp(_, BinOp::Add, _) => {} // correct: (2 + 3) is left of *
                            _ => panic!("expected addition as left operand of multiplication"),
                        }
                    }
                    _ => panic!("expected multiplication"),
                }
            }
            _ => panic!("expected let"),
        }
    }

    #[test]
    fn test_enum_variant_expression() {
        let source = r#"workflow "test" { let c: Color = Color::Red; }"#;
        let program = parse(source).unwrap();
        let wf = extract_workflow(&program);
        match &wf.body.stmts[0] {
            Stmt::Let { value, .. } => {
                match value {
                    Expr::EnumVariant { enum_name, variant } => {
                        assert_eq!(enum_name, "Color");
                        assert_eq!(variant, "Red");
                    }
                    _ => panic!("expected EnumVariant, got {:?}", value),
                }
            }
            _ => panic!("expected let"),
        }
    }

    #[test]
    fn test_namespace_call_still_works_with_parens() {
        let source = r#"workflow "test" { let _ = module::func(42); }"#;
        let program = parse(source).unwrap();
        let wf = extract_workflow(&program);
        match &wf.body.stmts[0] {
            Stmt::Let { value, .. } => {
                match value {
                    Expr::NamespaceCall { namespace, name, args } => {
                        assert_eq!(name, "func");
                        assert_eq!(args.len(), 1);
                        match &**namespace {
                            Expr::Ident(s) => assert_eq!(s, "module"),
                            _ => panic!("expected Ident namespace"),
                        }
                    }
                    other => panic!("expected NamespaceCall, got {:?}", other),
                }
            }
            _ => panic!("expected let"),
        }
    }

    #[test]
    fn test_assignment_statement() {
        let source = r#"workflow "test" { x = 42; }"#;
        let program = parse(source).unwrap();
        let wf = extract_workflow(&program);
        match &wf.body.stmts[0] {
            Stmt::Assign { target, value } => {
                assert_eq!(*target, Target::Ident("x".into()));
                assert!(matches!(value, Expr::Int(42)));
            }
            other => panic!("expected Assign, got {:?}", other),
        }
    }

    #[test]
    fn test_error_unexpected_token() {
        let result = parse(r#"workflow "test" { let x: int = ; }"#);
        assert!(result.is_err());
    }

    #[test]
    fn test_error_unclosed_block() {
        let result = parse(r#"workflow "test" { let x: int = 42;"#);
        assert!(result.is_err());
    }

    #[test]
    fn test_error_unexpected_eof() {
        let result = parse("workflow \"test\" {");
        assert!(result.is_err());
    }

    fn extract_workflow(program: &Program) -> &WorkflowDecl {
        match &program.items[0] {
            TopLevel::Workflow(wf) => wf,
            _ => panic!("expected workflow"),
        }
    }
}
