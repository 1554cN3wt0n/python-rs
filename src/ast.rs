#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal(Literal),
    Variable(String),
    Binary(Box<Expr>, BinaryOp, Box<Expr>),
    Logical(Box<Expr>, LogicalOp, Box<Expr>),
    Unary(UnaryOp, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    List(Vec<Expr>),
    Dict(Vec<(Expr, Expr)>),
    Subscript(Box<Expr>, Box<Expr>),
    Attribute(Box<Expr>, String),
    ListComprehension {
        expression: Box<Expr>,
        target: String,
        iterable: Box<Expr>,
        condition: Option<Box<Expr>>,
    },
    Lambda {
        params: Vec<String>,
        body: Box<Expr>,
    },
    Slice {
        start: Option<Box<Expr>>,
        stop: Option<Box<Expr>>,
        step: Option<Box<Expr>>,
    },
}

#[derive(Debug, Clone, PartialEq, Copy)]
pub enum LogicalOp {
    And,
    Or,
    Not,
}

#[derive(Debug, Clone, PartialEq, Copy)]
pub enum UnaryOp {
    Not,
    Neg,
    Pos,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Int(i64),
    String(String),
    Bool(bool),
    None,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Equal,
    NotEqual,
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
    In,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Assignment(Expr, Expr),
    If {
        condition: Expr,
        then_branch: Vec<Stmt>,
        else_branch: Option<Vec<Stmt>>,
    },
    While {
        condition: Expr,
        body: Vec<Stmt>,
    },
    For {
        target: String,
        iterable: Expr,
        body: Vec<Stmt>,
    },
    FunctionDef {
        name: String,
        params: Vec<String>,
        body: Vec<Stmt>,
    },
    ClassDef {
        name: String,
        bases: Vec<Expr>,
        methods: Vec<Stmt>,
    },
    Return(Option<Expr>),
    Expression(Expr),
    Try {
        body: Vec<Stmt>,
        handlers: Vec<ExceptHandler>,
    },
    Import(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExceptHandler {
    pub type_: Option<Expr>,
    pub name: Option<String>,
    pub body: Vec<Stmt>,
}
