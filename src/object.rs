use crate::ast::Stmt;
use enum_as_inner::EnumAsInner;
use std::fmt;

#[derive(Debug, Clone, PartialEq, EnumAsInner)]
#[allow(unpredictable_function_pointer_comparisons)]
pub enum PyObject {
    Int(i64),
    String(String),
    Bool(bool),
    Function {
        name: String,
        params: Vec<String>,
        body: Vec<Stmt>,
    },
    BuiltinFunction(fn(Vec<PyObject>) -> PyObject),
    None,
}

impl fmt::Display for PyObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PyObject::Int(n) => write!(f, "{}", n),
            PyObject::String(s) => write!(f, "{}", s),
            PyObject::Bool(b) => write!(f, "{}", if *b { "True" } else { "False" }),
            PyObject::Function { name, .. } => write!(f, "<function {}>", name),
            PyObject::BuiltinFunction(_) => write!(f, "<built-in function>"),
            PyObject::None => write!(f, "None"),
        }
    }
}
