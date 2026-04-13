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
        target: Box<Expr>,
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
    FString(Vec<FStringPart>),
    Tuple(Vec<Expr>),
    Set(Vec<Expr>),
    Yield(Option<Box<Expr>>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum FStringPart {
    Literal(String),
    Expression(Expr),
}

#[derive(Debug, Clone, PartialEq, Copy)]
pub enum LogicalOp {
    And,
    Or,
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
    BitwiseAnd,
    BitwiseOr,
    BitwiseXor,
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
        target: Expr,
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
    Raise(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExceptHandler {
    pub type_: Option<Expr>,
    pub name: Option<String>,
    pub body: Vec<Stmt>,
}

impl Expr {
    pub fn has_yield(&self) -> bool {
        match self {
            Expr::Yield(_) => true,
            Expr::Binary(left, _, right) => left.has_yield() || right.has_yield(),
            Expr::Logical(left, _, right) => left.has_yield() || right.has_yield(),
            Expr::Unary(_, expr) => expr.has_yield(),
            Expr::Call(callee, args) => {
                callee.has_yield() || args.iter().any(|arg| arg.has_yield())
            }
            Expr::List(exprs) | Expr::Tuple(exprs) | Expr::Set(exprs) => {
                exprs.iter().any(|e| e.has_yield())
            }
            Expr::Dict(items) => items.iter().any(|(k, v)| k.has_yield() || v.has_yield()),
            Expr::Subscript(obj, key) => obj.has_yield() || key.has_yield(),
            Expr::Attribute(obj, _) => obj.has_yield(),
            Expr::ListComprehension {
                expression,
                iterable,
                condition,
                ..
            } => {
                expression.has_yield()
                    || iterable.has_yield()
                    || condition.as_ref().is_some_and(|c| c.has_yield())
            }
            Expr::Slice { start, stop, step } => {
                start.as_ref().is_some_and(|e| e.has_yield())
                    || stop.as_ref().is_some_and(|e| e.has_yield())
                    || step.as_ref().is_some_and(|e| e.has_yield())
            }
            Expr::FString(parts) => parts.iter().any(|p| match p {
                FStringPart::Expression(e) => e.has_yield(),
                _ => false,
            }),
            _ => false,
        }
    }
}

impl Stmt {
    pub fn has_yield(&self) -> bool {
        match self {
            Stmt::Expression(expr) | Stmt::Return(Some(expr)) | Stmt::Raise(expr) => {
                expr.has_yield()
            }
            Stmt::Assignment(target, value) => target.has_yield() || value.has_yield(),
            Stmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                condition.has_yield()
                    || then_branch.iter().any(|s| s.has_yield())
                    || else_branch
                        .as_ref()
                        .is_some_and(|b| b.iter().any(|s| s.has_yield()))
            }
            Stmt::While { condition, body } => {
                condition.has_yield() || body.iter().any(|s| s.has_yield())
            }
            Stmt::For { iterable, body, .. } => {
                iterable.has_yield() || body.iter().any(|s| s.has_yield())
            }
            Stmt::Try { body, handlers } => {
                body.iter().any(|s| s.has_yield())
                    || handlers
                        .iter()
                        .any(|h| h.body.iter().any(|s| s.has_yield()))
            }
            _ => false,
        }
    }
}
