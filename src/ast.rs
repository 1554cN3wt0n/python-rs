#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal(Literal),
    Variable(String),
    Binary(Box<Expr>, BinaryOp, Box<Expr>),
    Logical(Box<Expr>, LogicalOp, Box<Expr>),
    Unary(LogicalOp, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    List(Vec<Expr>),
    Dict(Vec<(Expr, Expr)>),
    Subscript(Box<Expr>, Box<Expr>),
    Attribute(Box<Expr>, String),
}

#[derive(Debug, Clone, PartialEq, Copy)]
pub enum LogicalOp {
    And,
    Or,
    Not,
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
        methods: Vec<Stmt>,
    },
    Return(Option<Expr>),
    Expression(Expr),
}
