use crate::ast::*;
use crate::lexer::{Lexer, RawToken, Token};
use anyhow::{Result, anyhow};

pub struct Parser {
    lexer: Lexer,
}

impl Parser {
    pub fn new(lexer: Lexer) -> Self {
        Self { lexer }
    }

    pub fn parse(&mut self) -> Result<Vec<Stmt>> {
        let mut statements = Vec::new();
        while self.lexer.peek() != Token::Eof {
            if let Some(stmt) = self.parse_statement()? {
                statements.push(stmt);
            }
        }
        Ok(statements)
    }

    fn parse_statement(&mut self) -> Result<Option<Stmt>> {
        match self.lexer.peek() {
            Token::Newline => {
                self.lexer.next();
                Ok(None)
            }
            Token::Raw(RawToken::Def) => Ok(Some(self.parse_function_def()?)),
            Token::Raw(RawToken::If) => Ok(Some(self.parse_if_statement()?)),
            Token::Raw(RawToken::While) => Ok(Some(self.parse_while_statement()?)),
            Token::Raw(RawToken::For) => Ok(Some(self.parse_for_statement()?)),
            Token::Raw(RawToken::Class) => Ok(Some(self.parse_class_def()?)),
            Token::Raw(RawToken::Return) => Ok(Some(self.parse_return_statement()?)),
            _ => {
                let expr = self.parse_expression()?;
                if let Token::Raw(RawToken::Assign) = self.lexer.peek() {
                    self.lexer.next();
                    let value = self.parse_expression()?;
                    self.consume_newline()?;
                    Ok(Some(Stmt::Assignment(expr, value)))
                } else {
                    self.consume_newline()?;
                    Ok(Some(Stmt::Expression(expr)))
                }
            }
        }
    }

    fn parse_function_def(&mut self) -> Result<Stmt> {
        self.lexer.next(); // consume 'def'
        let name = match self.lexer.next() {
            Token::Raw(RawToken::Identifier(id)) => id,
            _ => return Err(anyhow!("Expected function name")),
        };

        self.expect(Token::Raw(RawToken::LParen))?;
        let mut params = Vec::new();
        if self.lexer.peek() != Token::Raw(RawToken::RParen) {
            loop {
                match self.lexer.next() {
                    Token::Raw(RawToken::Identifier(id)) => params.push(id),
                    _ => return Err(anyhow!("Expected parameter name")),
                }
                if self.lexer.peek() == Token::Raw(RawToken::Comma) {
                    self.lexer.next();
                } else {
                    break;
                }
            }
        }
        self.expect(Token::Raw(RawToken::RParen))?;
        self.expect(Token::Raw(RawToken::Colon))?;
        self.consume_newline()?;

        let body = self.parse_block()?;
        Ok(Stmt::FunctionDef { name, params, body })
    }

    fn parse_class_def(&mut self) -> Result<Stmt> {
        self.lexer.next(); // consume 'class'
        let name = match self.lexer.next() {
            Token::Raw(RawToken::Identifier(id)) => id,
            _ => return Err(anyhow!("Expected class name")),
        };

        let mut bases = Vec::new();
        if self.lexer.peek() == Token::Raw(RawToken::LParen) {
            self.lexer.next(); // consume '('
            while self.lexer.peek() != Token::Raw(RawToken::RParen)
                && self.lexer.peek() != Token::Eof
            {
                bases.push(self.parse_expression()?);
                if self.lexer.peek() == Token::Raw(RawToken::Comma) {
                    self.lexer.next();
                } else {
                    break;
                }
            }
            self.expect(Token::Raw(RawToken::RParen))?;
        }

        self.expect(Token::Raw(RawToken::Colon))?;
        self.consume_newline()?;
        let body = self.parse_block()?;
        Ok(Stmt::ClassDef {
            name,
            bases,
            methods: body,
        })
    }

    fn parse_if_statement(&mut self) -> Result<Stmt> {
        self.lexer.next(); // consume 'if'
        let condition = self.parse_expression()?;
        self.expect(Token::Raw(RawToken::Colon))?;
        self.consume_newline()?;
        let then_branch = self.parse_block()?;

        let mut else_branch = None;
        if self.lexer.peek() == Token::Raw(RawToken::Else) {
            self.lexer.next();
            self.expect(Token::Raw(RawToken::Colon))?;
            self.consume_newline()?;
            else_branch = Some(self.parse_block()?);
        } else if self.lexer.peek() == Token::Raw(RawToken::Elif) {
            // Treat elif as another if in the else branch
            else_branch = Some(vec![self.parse_if_statement()?]);
        }

        Ok(Stmt::If {
            condition,
            then_branch,
            else_branch,
        })
    }

    fn parse_while_statement(&mut self) -> Result<Stmt> {
        self.lexer.next(); // consume 'while'
        let condition = self.parse_expression()?;
        self.expect(Token::Raw(RawToken::Colon))?;
        self.consume_newline()?;
        let body = self.parse_block()?;
        Ok(Stmt::While { condition, body })
    }

    fn parse_for_statement(&mut self) -> Result<Stmt> {
        self.lexer.next(); // consume 'for'
        let target = match self.lexer.next() {
            Token::Raw(RawToken::Identifier(id)) => id,
            _ => return Err(anyhow!("Expected identifier after 'for'")),
        };
        self.expect(Token::Raw(RawToken::In))?;
        let iterable = self.parse_expression()?;
        self.expect(Token::Raw(RawToken::Colon))?;
        self.consume_newline()?;
        let body = self.parse_block()?;
        Ok(Stmt::For {
            target,
            iterable,
            body,
        })
    }

    fn parse_return_statement(&mut self) -> Result<Stmt> {
        self.lexer.next(); // consume 'return'
        if self.lexer.peek() == Token::Newline || self.lexer.peek() == Token::Eof {
            self.consume_newline()?;
            Ok(Stmt::Return(None))
        } else {
            let expr = self.parse_expression()?;
            self.consume_newline()?;
            Ok(Stmt::Return(Some(expr)))
        }
    }

    fn parse_block(&mut self) -> Result<Vec<Stmt>> {
        self.expect(Token::Indent)?;
        let mut body = Vec::new();
        while self.lexer.peek() != Token::Dedent && self.lexer.peek() != Token::Eof {
            if let Some(stmt) = self.parse_statement()? {
                body.push(stmt);
            }
        }
        self.expect(Token::Dedent)?;
        Ok(body)
    }

    fn parse_expression(&mut self) -> Result<Expr> {
        self.parse_logical_or()
    }

    fn parse_logical_or(&mut self) -> Result<Expr> {
        let mut expr = self.parse_logical_and()?;
        while self.lexer.peek() == Token::Raw(RawToken::Or) {
            self.lexer.next();
            let right = self.parse_logical_and()?;
            expr = Expr::Logical(Box::new(expr), LogicalOp::Or, Box::new(right));
        }
        Ok(expr)
    }

    fn parse_logical_and(&mut self) -> Result<Expr> {
        let mut expr = self.parse_logical_not()?;
        while self.lexer.peek() == Token::Raw(RawToken::And) {
            self.lexer.next();
            let right = self.parse_logical_not()?;
            expr = Expr::Logical(Box::new(expr), LogicalOp::And, Box::new(right));
        }
        Ok(expr)
    }

    fn parse_logical_not(&mut self) -> Result<Expr> {
        if self.lexer.peek() == Token::Raw(RawToken::Not) {
            self.lexer.next();
            let expr = self.parse_logical_not()?;
            Ok(Expr::Unary(LogicalOp::Not, Box::new(expr)))
        } else {
            self.parse_equality()
        }
    }

    fn parse_equality(&mut self) -> Result<Expr> {
        let mut expr = self.parse_comparison()?;
        while matches!(
            self.lexer.peek(),
            Token::Raw(RawToken::Equal) | Token::Raw(RawToken::NotEqual)
        ) {
            let op = match self.lexer.next() {
                Token::Raw(RawToken::Equal) => BinaryOp::Equal,
                Token::Raw(RawToken::NotEqual) => BinaryOp::NotEqual,
                _ => unreachable!(),
            };
            let right = self.parse_comparison()?;
            expr = Expr::Binary(Box::new(expr), op, Box::new(right));
        }
        Ok(expr)
    }

    fn parse_comparison(&mut self) -> Result<Expr> {
        let mut expr = self.parse_term()?;
        while matches!(
            self.lexer.peek(),
            Token::Raw(RawToken::Less)
                | Token::Raw(RawToken::LessEqual)
                | Token::Raw(RawToken::Greater)
                | Token::Raw(RawToken::GreaterEqual)
                | Token::Raw(RawToken::In)
        ) {
            let op = match self.lexer.next() {
                Token::Raw(RawToken::Less) => BinaryOp::Less,
                Token::Raw(RawToken::LessEqual) => BinaryOp::LessEqual,
                Token::Raw(RawToken::Greater) => BinaryOp::Greater,
                Token::Raw(RawToken::GreaterEqual) => BinaryOp::GreaterEqual,
                Token::Raw(RawToken::In) => BinaryOp::In,
                _ => unreachable!(),
            };
            let right = self.parse_term()?;
            expr = Expr::Binary(Box::new(expr), op, Box::new(right));
        }
        Ok(expr)
    }

    fn parse_term(&mut self) -> Result<Expr> {
        let mut expr = self.parse_factor()?;
        while matches!(
            self.lexer.peek(),
            Token::Raw(RawToken::Plus) | Token::Raw(RawToken::Minus)
        ) {
            let op = match self.lexer.next() {
                Token::Raw(RawToken::Plus) => BinaryOp::Add,
                Token::Raw(RawToken::Minus) => BinaryOp::Sub,
                _ => unreachable!(),
            };
            let right = self.parse_factor()?;
            expr = Expr::Binary(Box::new(expr), op, Box::new(right));
        }
        Ok(expr)
    }

    fn parse_factor(&mut self) -> Result<Expr> {
        let mut expr = self.parse_postfix()?;
        while matches!(
            self.lexer.peek(),
            Token::Raw(RawToken::Multiply) | Token::Raw(RawToken::Divide)
        ) {
            let op = match self.lexer.next() {
                Token::Raw(RawToken::Multiply) => BinaryOp::Mul,
                Token::Raw(RawToken::Divide) => BinaryOp::Div,
                _ => unreachable!(),
            };
            let right = self.parse_postfix()?;
            expr = Expr::Binary(Box::new(expr), op, Box::new(right));
        }
        Ok(expr)
    }

    fn parse_postfix(&mut self) -> Result<Expr> {
        let mut expr = self.parse_primary()?;
        loop {
            match self.lexer.peek() {
                Token::Raw(RawToken::LParen) => {
                    self.lexer.next();
                    let mut args = Vec::new();
                    if self.lexer.peek() != Token::Raw(RawToken::RParen) {
                        loop {
                            args.push(self.parse_expression()?);
                            if self.lexer.peek() == Token::Raw(RawToken::Comma) {
                                self.lexer.next();
                            } else {
                                break;
                            }
                        }
                    }
                    self.expect(Token::Raw(RawToken::RParen))?;
                    expr = Expr::Call(Box::new(expr), args);
                }
                Token::Raw(RawToken::LBracket) => {
                    self.lexer.next();
                    let index = self.parse_expression()?;
                    self.expect(Token::Raw(RawToken::RBracket))?;
                    expr = Expr::Subscript(Box::new(expr), Box::new(index));
                }
                Token::Raw(RawToken::Dot) => {
                    self.lexer.next();
                    match self.lexer.next() {
                        Token::Raw(RawToken::Identifier(id)) => {
                            expr = Expr::Attribute(Box::new(expr), id);
                        }
                        _ => return Err(anyhow!("Expected identifier after '.'")),
                    }
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr> {
        match self.lexer.next() {
            Token::Raw(RawToken::True) => Ok(Expr::Literal(Literal::Bool(true))),
            Token::Raw(RawToken::False) => Ok(Expr::Literal(Literal::Bool(false))),
            Token::Raw(RawToken::None) => Ok(Expr::Literal(Literal::None)),
            Token::Raw(RawToken::Integer(n)) => Ok(Expr::Literal(Literal::Int(n))),
            Token::Raw(RawToken::String(s)) => Ok(Expr::Literal(Literal::String(s))),
            Token::Raw(RawToken::LBracket) => {
                let mut items = Vec::new();
                if self.lexer.peek() != Token::Raw(RawToken::RBracket) {
                    loop {
                        items.push(self.parse_expression()?);
                        if self.lexer.peek() == Token::Raw(RawToken::Comma) {
                            self.lexer.next();
                        } else {
                            break;
                        }
                    }
                }
                self.expect(Token::Raw(RawToken::RBracket))?;
                Ok(Expr::List(items))
            }
            Token::Raw(RawToken::LBrace) => {
                let mut items = Vec::new();
                if self.lexer.peek() != Token::Raw(RawToken::RBrace) {
                    loop {
                        let key = self.parse_expression()?;
                        self.expect(Token::Raw(RawToken::Colon))?;
                        let val = self.parse_expression()?;
                        items.push((key, val));
                        if self.lexer.peek() == Token::Raw(RawToken::Comma) {
                            self.lexer.next();
                        } else {
                            break;
                        }
                    }
                }
                self.expect(Token::Raw(RawToken::RBrace))?;
                Ok(Expr::Dict(items))
            }
            Token::Raw(RawToken::Identifier(id)) => Ok(Expr::Variable(id)),
            Token::Raw(RawToken::LParen) => {
                let expr = self.parse_expression()?;
                self.expect(Token::Raw(RawToken::RParen))?;
                Ok(expr)
            }
            t => Err(anyhow!("Unexpected token: {:?}", t)),
        }
    }

    fn expect(&mut self, expected: Token) -> Result<()> {
        let token = self.lexer.next();
        if token == expected {
            Ok(())
        } else {
            Err(anyhow!("Expected {:?}, found {:?}", expected, token))
        }
    }

    fn consume_newline(&mut self) -> Result<()> {
        match self.lexer.peek() {
            Token::Newline => {
                self.lexer.next();
                Ok(())
            }
            Token::Eof => Ok(()),
            Token::Dedent => Ok(()), // Dedent often implies end of line
            t => Err(anyhow!("Expected newline or Eof, found {:?}", t)),
        }
    }
}
