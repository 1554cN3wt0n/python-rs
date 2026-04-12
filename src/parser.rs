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
            Token::Raw(RawToken::Return) => Ok(Some(self.parse_return_statement()?)),
            _ => {
                let expr = self.parse_expression()?;
                if let Token::Raw(RawToken::Assign) = self.lexer.peek() {
                    self.lexer.next();
                    if let Expr::Variable(name) = expr {
                        let value = self.parse_expression()?;
                        self.consume_newline()?;
                        Ok(Some(Stmt::Assignment(name, value)))
                    } else {
                        Err(anyhow!("Invalid assignment target"))
                    }
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
        self.parse_equality()
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
        ) {
            let op = match self.lexer.next() {
                Token::Raw(RawToken::Less) => BinaryOp::Less,
                Token::Raw(RawToken::LessEqual) => BinaryOp::LessEqual,
                Token::Raw(RawToken::Greater) => BinaryOp::Greater,
                Token::Raw(RawToken::GreaterEqual) => BinaryOp::GreaterEqual,
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
        let mut expr = self.parse_primary()?;
        while matches!(
            self.lexer.peek(),
            Token::Raw(RawToken::Multiply) | Token::Raw(RawToken::Divide)
        ) {
            let op = match self.lexer.next() {
                Token::Raw(RawToken::Multiply) => BinaryOp::Mul,
                Token::Raw(RawToken::Divide) => BinaryOp::Div,
                _ => unreachable!(),
            };
            let right = self.parse_primary()?;
            expr = Expr::Binary(Box::new(expr), op, Box::new(right));
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
            Token::Raw(RawToken::Identifier(id)) => {
                if self.lexer.peek() == Token::Raw(RawToken::LParen) {
                    self.lexer.next(); // consume (
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
                    Ok(Expr::Call(id, args))
                } else {
                    Ok(Expr::Variable(id))
                }
            }
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
