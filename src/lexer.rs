use logos::Logos;

#[derive(Logos, Debug, PartialEq, Clone)]
#[logos(skip(r"[ \t\f]+|#[^\n]*", allow_greedy = true))]
pub enum RawToken {
    #[token("def")]
    Def,
    #[token("return")]
    Return,
    #[token("if")]
    If,
    #[token("else")]
    Else,
    #[token("elif")]
    Elif,
    #[token("while")]
    While,
    #[token("True")]
    True,
    #[token("False")]
    False,
    #[token("None")]
    None,
    #[token("and")]
    And,
    #[token("or")]
    Or,
    #[token("not")]
    Not,
    #[token("for")]
    For,
    #[token("in")]
    In,
    #[token("class")]
    Class,
    #[token("lambda")]
    Lambda,
    #[token("try")]
    Try,
    #[token("except")]
    Except,
    #[token("as")]
    As,
    #[token("import")]
    Import,
    #[token("raise")]
    Raise,
    #[token("yield")]
    Yield,

    #[regex("[a-zA-Z_][a-zA-Z0-9_]*", |lex| lex.slice().to_string())]
    Identifier(String),

    #[regex("[0-9]+", |lex| lex.slice().parse::<i64>().unwrap())]
    Integer(i64),

    #[regex(r#""([^"\\]|\\.)*""#, |lex| {
        let s = lex.slice();
        s[1..s.len()-1].to_string()
    })]
    String(String),

    #[regex(r#"f"([^"\\]|\\.)*""#, |lex| {
        let s = lex.slice();
        s[2..s.len()-1].to_string()
    })]
    FString(String),

    #[token("=")]
    Assign,
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Multiply,
    #[token("/")]
    Divide,
    #[token("%")]
    Percent,
    #[token("==")]
    Equal,
    #[token(":")]
    Colon,
    #[token("!=")]
    NotEqual,
    #[token("<")]
    Less,
    #[token(">")]
    Greater,
    #[token("<=")]
    LessEqual,
    #[token(">=")]
    GreaterEqual,

    #[token("|")]
    BitwiseOr,
    #[token("&")]
    BitwiseAnd,
    #[token("^")]
    BitwiseXor,

    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token(",")]
    Comma,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token(".")]
    Dot,
}

#[derive(Debug, PartialEq, Clone)]
pub enum Token {
    Raw(RawToken),
    Indent,
    Dedent,
    Newline,
    Eof,
}

pub struct Lexer {
    tokens: Vec<Token>,
    cursor: usize,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        let mut tokens = Vec::new();
        let mut indent_stack = vec![0];

        for line in input.lines() {
            let trimmed = line.trim_start();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let indent = line.len() - trimmed.len();
            let current_indent = *indent_stack.last().unwrap();

            if indent > current_indent {
                indent_stack.push(indent);
                tokens.push(Token::Indent);
            } else if indent < current_indent {
                while indent < *indent_stack.last().unwrap() {
                    indent_stack.pop();
                    tokens.push(Token::Dedent);
                }
                // Check for inconsistent indentation (optional)
            }

            let lex = RawToken::lexer(trimmed);
            for raw in lex.flatten() {
                tokens.push(Token::Raw(raw));
            }
            tokens.push(Token::Newline);
        }

        while indent_stack.len() > 1 {
            indent_stack.pop();
            tokens.push(Token::Dedent);
        }
        tokens.push(Token::Eof);

        Self { tokens, cursor: 0 }
    }

    pub fn next(&mut self) -> Token {
        if self.cursor < self.tokens.len() {
            let t = self.tokens[self.cursor].clone();
            self.cursor += 1;
            t
        } else {
            Token::Eof
        }
    }

    pub fn peek(&self) -> Token {
        if self.cursor < self.tokens.len() {
            self.tokens[self.cursor].clone()
        } else {
            Token::Eof
        }
    }
}
