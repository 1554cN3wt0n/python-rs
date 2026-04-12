use crate::ast::Stmt;
use enum_as_inner::EnumAsInner;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq, EnumAsInner)]
#[allow(unpredictable_function_pointer_comparisons)]
pub enum PyObject {
    Int(i64),
    String(String),
    Bool(bool),
    List(Rc<RefCell<Vec<PyObject>>>),
    Dict(Rc<RefCell<HashMap<String, PyObject>>>),
    Function {
        name: String,
        params: Vec<String>,
        body: Vec<Stmt>,
    },
    BuiltinFunction(fn(Vec<PyObject>) -> PyObject),
    Class {
        name: String,
        methods: HashMap<String, PyObject>,
    },
    Instance {
        class: Rc<RefCell<PyObject>>,
        attributes: Rc<RefCell<HashMap<String, PyObject>>>,
    },
    None,
}

impl fmt::Display for PyObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PyObject::Int(n) => write!(f, "{}", n),
            PyObject::String(s) => write!(f, "{}", s),
            PyObject::Bool(b) => write!(f, "{}", if *b { "True" } else { "False" }),
            PyObject::List(l) => {
                write!(f, "[")?;
                let items = l.borrow();
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, "]")
            }
            PyObject::Dict(d) => {
                write!(f, "{{")?;
                let items = d.borrow();
                for (i, (k, v)) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "\"{}\": {}", k, v)?;
                }
                write!(f, "}}")
            }
            PyObject::Function { name, .. } => write!(f, "<function {}>", name),
            PyObject::BuiltinFunction(_) => write!(f, "<built-in function>"),
            PyObject::Class { name, .. } => write!(f, "<class {}>", name),
            PyObject::Instance { class, .. } => {
                let class_borrow = class.borrow();
                if let PyObject::Class { name, .. } = &*class_borrow {
                    write!(f, "<{} instance>", name)
                } else {
                    write!(f, "<instance>")
                }
            }
            PyObject::None => write!(f, "None"),
        }
    }
}
