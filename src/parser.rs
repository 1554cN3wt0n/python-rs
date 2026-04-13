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
            Token::Raw(RawToken::Import) => self.parse_import_statement().map(Some),
            Token::Raw(RawToken::Raise) => self.parse_raise_statement().map(Some),
            Token::Raw(RawToken::Def) => Ok(Some(self.parse_function_def()?)),
            Token::Raw(RawToken::Try) => self.parse_try_statement().map(Some),
            Token::Raw(RawToken::If) => self.parse_if_statement().map(Some),
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

    fn parse_import_statement(&mut self) -> Result<Stmt> {
        self.lexer.next(); // consume 'import'
        let mut name = match self.lexer.next() {
            Token::Raw(RawToken::Identifier(id)) => id,
            _ => return Err(anyhow!("Expected identifier after 'import'")),
        };

        while self.lexer.peek() == Token::Raw(RawToken::Dot) {
            self.lexer.next();
            match self.lexer.next() {
                Token::Raw(RawToken::Identifier(id)) => {
                    name.push('.');
                    name.push_str(&id);
                }
                _ => return Err(anyhow!("Expected identifier after '.'")),
            }
        }

        self.consume_newline()?;
        Ok(Stmt::Import(name))
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
                bases.push(self.parse_single_expression()?);
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
        let condition = self.parse_single_expression()?;
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
        let condition = self.parse_single_expression()?;
        self.expect(Token::Raw(RawToken::Colon))?;
        self.consume_newline()?;
        let body = self.parse_block()?;
        Ok(Stmt::While { condition, body })
    }

    fn parse_try_statement(&mut self) -> Result<Stmt> {
        self.lexer.next(); // consume 'try'
        self.expect(Token::Raw(RawToken::Colon))?;
        self.consume_newline()?;
        let body = self.parse_block()?;

        let mut handlers = Vec::new();
        while self.lexer.peek() == Token::Raw(RawToken::Except) {
            self.lexer.next(); // consume 'except'
            let mut type_ = None;
            let mut name = None;

            if self.lexer.peek() != Token::Raw(RawToken::Colon) {
                type_ = Some(self.parse_expression()?);
                if self.lexer.peek() == Token::Raw(RawToken::As) {
                    self.lexer.next();
                    match self.lexer.next() {
                        Token::Raw(RawToken::Identifier(id)) => name = Some(id),
                        _ => return Err(anyhow!("Expected identifier after 'as'")),
                    }
                }
            }

            self.expect(Token::Raw(RawToken::Colon))?;
            self.consume_newline()?;
            let handler_body = self.parse_block()?;
            handlers.push(ExceptHandler {
                type_,
                name,
                body: handler_body,
            });
        }

        if handlers.is_empty() {
            return Err(anyhow!("Expected at least one 'except' block after 'try'"));
        }

        Ok(Stmt::Try { body, handlers })
    }

    fn parse_for_statement(&mut self) -> Result<Stmt> {
        self.lexer.next(); // consume 'for'
        let target = self.parse_for_target()?;
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
        let mut exprs = Vec::new();
        exprs.push(self.parse_single_expression()?);

        let mut saw_comma = false;
        while self.lexer.peek() == Token::Raw(RawToken::Comma) {
            self.lexer.next();
            saw_comma = true;
            match self.lexer.peek() {
                Token::Raw(RawToken::RParen)
                | Token::Raw(RawToken::RBrace)
                | Token::Raw(RawToken::RBracket)
                | Token::Newline
                | Token::Eof => break,
                _ => {}
            }
            exprs.push(self.parse_single_expression()?);
        }

        if saw_comma {
            Ok(Expr::Tuple(exprs))
        } else {
            Ok(exprs.pop().unwrap())
        }
    }

    fn parse_single_expression(&mut self) -> Result<Expr> {
        if self.lexer.peek() == Token::Raw(RawToken::Lambda) {
            self.parse_lambda()
        } else {
            self.parse_logical_or()
        }
    }

    fn parse_lambda(&mut self) -> Result<Expr> {
        self.lexer.next(); // consume 'lambda'
        let mut params = Vec::new();
        if self.lexer.peek() != Token::Raw(RawToken::Colon) {
            loop {
                match self.lexer.next() {
                    Token::Raw(RawToken::Identifier(id)) => params.push(id),
                    _ => return Err(anyhow!("Expected identifier in lambda params")),
                }
                if self.lexer.peek() == Token::Raw(RawToken::Comma) {
                    self.lexer.next();
                } else {
                    break;
                }
            }
        }
        self.expect(Token::Raw(RawToken::Colon))?;
        let body = self.parse_single_expression()?;
        Ok(Expr::Lambda {
            params,
            body: Box::new(body),
        })
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
        let mut expr = self.parse_unary()?;
        while self.lexer.peek() == Token::Raw(RawToken::And) {
            self.lexer.next();
            let right = self.parse_unary()?;
            expr = Expr::Logical(Box::new(expr), LogicalOp::And, Box::new(right));
        }
        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<Expr> {
        match self.lexer.peek() {
            Token::Raw(RawToken::Not) => {
                self.lexer.next();
                let expr = self.parse_unary()?;
                Ok(Expr::Unary(UnaryOp::Not, Box::new(expr)))
            }
            Token::Raw(RawToken::Plus) => {
                self.lexer.next();
                let expr = self.parse_unary()?;
                Ok(Expr::Unary(UnaryOp::Pos, Box::new(expr)))
            }
            Token::Raw(RawToken::Minus) => {
                self.lexer.next();
                let expr = self.parse_unary()?;
                Ok(Expr::Unary(UnaryOp::Neg, Box::new(expr)))
            }
            _ => self.parse_equality(),
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
        let mut expr = self.parse_bitwise_or()?;
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
            let right = self.parse_bitwise_or()?;
            expr = Expr::Binary(Box::new(expr), op, Box::new(right));
        }
        Ok(expr)
    }

    fn parse_bitwise_or(&mut self) -> Result<Expr> {
        let mut expr = self.parse_bitwise_xor()?;
        while self.lexer.peek() == Token::Raw(RawToken::BitwiseOr) {
            self.lexer.next();
            let right = self.parse_bitwise_xor()?;
            expr = Expr::Binary(Box::new(expr), BinaryOp::BitwiseOr, Box::new(right));
        }
        Ok(expr)
    }

    fn parse_bitwise_xor(&mut self) -> Result<Expr> {
        let mut expr = self.parse_bitwise_and()?;
        while self.lexer.peek() == Token::Raw(RawToken::BitwiseXor) {
            self.lexer.next();
            let right = self.parse_bitwise_and()?;
            expr = Expr::Binary(Box::new(expr), BinaryOp::BitwiseXor, Box::new(right));
        }
        Ok(expr)
    }

    fn parse_bitwise_and(&mut self) -> Result<Expr> {
        let mut expr = self.parse_term()?;
        while self.lexer.peek() == Token::Raw(RawToken::BitwiseAnd) {
            self.lexer.next();
            let right = self.parse_term()?;
            expr = Expr::Binary(Box::new(expr), BinaryOp::BitwiseAnd, Box::new(right));
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
            Token::Raw(RawToken::Multiply)
                | Token::Raw(RawToken::Divide)
                | Token::Raw(RawToken::Percent)
        ) {
            let op = match self.lexer.next() {
                Token::Raw(RawToken::Multiply) => BinaryOp::Mul,
                Token::Raw(RawToken::Divide) => BinaryOp::Div,
                Token::Raw(RawToken::Percent) => BinaryOp::Mod,
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
                            args.push(self.parse_single_expression()?);
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
                    let index = if self.lexer.peek() == Token::Raw(RawToken::Colon) {
                        // Empty start slice like [:5]
                        self.parse_slice(None)?
                    } else {
                        let first = self.parse_expression()?;
                        if self.lexer.peek() == Token::Raw(RawToken::Colon) {
                            self.parse_slice(Some(first))?
                        } else {
                            first
                        }
                    };
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
            Token::Raw(RawToken::FString(s)) => self.parse_fstring(s),
            Token::Raw(RawToken::LBracket) => {
                if self.lexer.peek() == Token::Raw(RawToken::RBracket) {
                    self.lexer.next();
                    return Ok(Expr::List(Vec::new()));
                }

                let expression = self.parse_single_expression()?;
                if self.lexer.peek() == Token::Raw(RawToken::For) {
                    self.lexer.next(); // consume 'for'
                    let target = self.parse_for_target()?;
                    self.expect(Token::Raw(RawToken::In))?;
                    let iterable = self.parse_expression()?;
                    let mut condition = None;
                    if self.lexer.peek() == Token::Raw(RawToken::If) {
                        self.lexer.next();
                        condition = Some(Box::new(self.parse_expression()?));
                    }
                    self.expect(Token::Raw(RawToken::RBracket))?;
                    return Ok(Expr::ListComprehension {
                        expression: Box::new(expression),
                        target: Box::new(target),
                        iterable: Box::new(iterable),
                        condition,
                    });
                }

                let mut items = vec![expression];
                while self.lexer.peek() == Token::Raw(RawToken::Comma) {
                    self.lexer.next();
                    if self.lexer.peek() == Token::Raw(RawToken::RBracket) {
                        break;
                    }
                    items.push(self.parse_single_expression()?);
                }
                self.expect(Token::Raw(RawToken::RBracket))?;
                Ok(Expr::List(items))
            }
            Token::Raw(RawToken::LBrace) => {
                if self.lexer.peek() == Token::Raw(RawToken::RBrace) {
                    self.lexer.next();
                    return Ok(Expr::Dict(Vec::new()));
                }

                let first = self.parse_single_expression()?;
                if self.lexer.peek() == Token::Raw(RawToken::Colon) {
                    // It's a dictionary
                    self.lexer.next();
                    let val = self.parse_single_expression()?;
                    let mut items = vec![(first, val)];
                    while self.lexer.peek() == Token::Raw(RawToken::Comma) {
                        self.lexer.next();
                        if self.lexer.peek() == Token::Raw(RawToken::RBrace) {
                            break;
                        }
                        let k = self.parse_single_expression()?;
                        self.expect(Token::Raw(RawToken::Colon))?;
                        let v = self.parse_single_expression()?;
                        items.push((k, v));
                    }
                    self.expect(Token::Raw(RawToken::RBrace))?;
                    Ok(Expr::Dict(items))
                } else {
                    // It's a set
                    let mut items = vec![first];
                    while self.lexer.peek() == Token::Raw(RawToken::Comma) {
                        self.lexer.next();
                        if self.lexer.peek() == Token::Raw(RawToken::RBrace) {
                            break;
                        }
                        items.push(self.parse_single_expression()?);
                    }
                    self.expect(Token::Raw(RawToken::RBrace))?;
                    Ok(Expr::Set(items))
                }
            }
            Token::Raw(RawToken::Identifier(id)) => Ok(Expr::Variable(id)),
            Token::Raw(RawToken::LParen) => {
                if self.lexer.peek() == Token::Raw(RawToken::RParen) {
                    self.lexer.next();
                    return Ok(Expr::Tuple(Vec::new()));
                }
                let expr = self.parse_expression()?;
                self.expect(Token::Raw(RawToken::RParen))?;
                Ok(expr)
            }
            t => Err(anyhow!("Unexpected token: {:?}", t)),
        }
    }

    fn parse_slice(&mut self, start: Option<Expr>) -> Result<Expr> {
        self.expect(Token::Raw(RawToken::Colon))?;
        let mut stop = None;
        if self.lexer.peek() != Token::Raw(RawToken::Colon)
            && self.lexer.peek() != Token::Raw(RawToken::RBracket)
        {
            stop = Some(Box::new(self.parse_expression()?));
        }

        let mut step = None;
        if self.lexer.peek() == Token::Raw(RawToken::Colon) {
            self.lexer.next();
            if self.lexer.peek() != Token::Raw(RawToken::RBracket) {
                step = Some(Box::new(self.parse_expression()?));
            }
        }

        Ok(Expr::Slice {
            start: start.map(Box::new),
            stop,
            step,
        })
    }

    fn expect(&mut self, expected: Token) -> Result<()> {
        let token = self.lexer.next();
        if token == expected {
            Ok(())
        } else {
            Err(anyhow!("Expected {:?}, found {:?}", expected, token))
        }
    }

    fn parse_raise_statement(&mut self) -> Result<Stmt> {
        self.lexer.next(); // consume 'raise'
        let expr = self.parse_expression()?;
        self.consume_newline()?;
        Ok(Stmt::Raise(expr))
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

    fn parse_fstring(&mut self, content: String) -> Result<Expr> {
        let mut parts = Vec::new();
        let mut start = 0;

        while start < content.len() {
            if let Some(brace_start) = content[start..].find('{') {
                let brace_start = start + brace_start;
                if brace_start > start {
                    parts.push(FStringPart::Literal(
                        content[start..brace_start].to_string(),
                    ));
                }

                if let Some(brace_end) = content[brace_start..].find('}') {
                    let brace_end = brace_start + brace_end;
                    let expr_str = &content[brace_start + 1..brace_end];

                    let inner_lexer = crate::lexer::Lexer::new(expr_str);
                    let mut inner_parser = Parser::new(inner_lexer);
                    let expr = inner_parser.parse_expression()?;

                    parts.push(FStringPart::Expression(expr));
                    start = brace_end + 1;
                } else {
                    return Err(anyhow!("Unclosed '{{' in f-string"));
                }
            } else {
                parts.push(FStringPart::Literal(content[start..].to_string()));
                break;
            }
        }
        Ok(Expr::FString(parts))
    }

    fn parse_for_target(&mut self) -> Result<Expr> {
        let mut targets = Vec::new();
        targets.push(self.parse_postfix()?);

        let mut saw_comma = false;
        while self.lexer.peek() == Token::Raw(RawToken::Comma) {
            self.lexer.next();
            saw_comma = true;
            if self.lexer.peek() == Token::Raw(RawToken::In) {
                break;
            }
            targets.push(self.parse_postfix()?);
        }

        if saw_comma {
            Ok(Expr::Tuple(targets))
        } else {
            Ok(targets.pop().unwrap())
        }
    }
}
