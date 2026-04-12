use crate::ast::Stmt;
use enum_as_inner::EnumAsInner;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

#[derive(Clone, EnumAsInner)]
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
    BuiltinFunction(Rc<dyn Fn(Vec<PyObject>) -> PyObject>),
    Class {
        name: String,
        bases: Vec<PyObject>,
        methods: HashMap<String, PyObject>,
    },
    Type(String),
    Module {
        name: String,
        env: Rc<RefCell<crate::env::Environment>>,
    },
    Slice {
        start: Option<Box<PyObject>>,
        stop: Option<Box<PyObject>>,
        step: Option<Box<PyObject>>,
    },
    Instance {
        class: Rc<RefCell<PyObject>>,
        attributes: Rc<RefCell<HashMap<String, PyObject>>>,
    },
    Iterator(Rc<RefCell<PyIterator>>),
    None,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PyIterator {
    List(Rc<RefCell<Vec<PyObject>>>, usize),
    String(String, usize),
    Range(i64, i64, i64), // current, stop, step
}

impl PyIterator {
    pub fn next(&mut self) -> Option<PyObject> {
        match self {
            PyIterator::List(l, idx) => {
                let items = l.borrow();
                if *idx < items.len() {
                    let val = items[*idx].clone();
                    *idx += 1;
                    Some(val)
                } else {
                    None
                }
            }
            PyIterator::String(s, idx) => {
                let chars: Vec<char> = s.chars().collect();
                if *idx < chars.len() {
                    let val = PyObject::String(chars[*idx].to_string());
                    *idx += 1;
                    Some(val)
                } else {
                    None
                }
            }
            PyIterator::Range(curr, stop, step) => {
                if (*step > 0 && *curr < *stop) || (*step < 0 && *curr > *stop) {
                    let val = PyObject::Int(*curr);
                    *curr += *step;
                    Some(val)
                } else {
                    None
                }
            }
        }
    }
}

impl fmt::Debug for PyObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PyObject::Int(n) => f.debug_tuple("Int").field(n).finish(),
            PyObject::String(s) => f.debug_tuple("String").field(s).finish(),
            PyObject::Bool(b) => f.debug_tuple("Bool").field(b).finish(),
            PyObject::List(l) => f.debug_tuple("List").field(l).finish(),
            PyObject::Dict(d) => f.debug_tuple("Dict").field(d).finish(),
            PyObject::Function { name, .. } => f
                .debug_struct("Function")
                .field("name", name)
                .finish_non_exhaustive(),
            PyObject::BuiltinFunction(_) => f.debug_tuple("BuiltinFunction").finish(),
            PyObject::Class { name, .. } => f
                .debug_struct("Class")
                .field("name", name)
                .finish_non_exhaustive(),
            PyObject::Type(name) => f.debug_tuple("Type").field(name).finish(),
            PyObject::Module { name, .. } => f.debug_tuple("Module").field(name).finish(),
            PyObject::Iterator(it) => f.debug_tuple("Iterator").field(it).finish(),
            PyObject::Slice { start, stop, step } => f
                .debug_struct("Slice")
                .field("start", start)
                .field("stop", stop)
                .field("step", step)
                .finish(),
            PyObject::Instance { .. } => f.debug_struct("Instance").finish_non_exhaustive(),
            PyObject::None => write!(f, "None"),
        }
    }
}

impl PartialEq for PyObject {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (PyObject::Int(a), PyObject::Int(b)) => a == b,
            (PyObject::String(a), PyObject::String(b)) => a == b,
            (PyObject::Bool(a), PyObject::Bool(b)) => a == b,
            (PyObject::List(a), PyObject::List(b)) => {
                Rc::ptr_eq(a, b) || *a.borrow() == *b.borrow()
            }
            (PyObject::Dict(a), PyObject::Dict(b)) => {
                Rc::ptr_eq(a, b) || *a.borrow() == *b.borrow()
            }
            (PyObject::Function { name: a, .. }, PyObject::Function { name: b, .. }) => a == b,
            (PyObject::BuiltinFunction(a), PyObject::BuiltinFunction(b)) => Rc::ptr_eq(a, b),
            (PyObject::Class { name: a, .. }, PyObject::Class { name: b, .. }) => a == b,
            (PyObject::Type(a), PyObject::Type(b)) => a == b,
            (
                PyObject::Slice {
                    start: a_start,
                    stop: a_stop,
                    step: a_step,
                },
                PyObject::Slice {
                    start: b_start,
                    stop: b_stop,
                    step: b_step,
                },
            ) => a_start == b_start && a_stop == b_stop && a_step == b_step,
            (PyObject::Iterator(a), PyObject::Iterator(b)) => Rc::ptr_eq(a, b),
            (PyObject::None, PyObject::None) => true,
            _ => false,
        }
    }
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
            PyObject::Type(name) => write!(f, "<type {}>", name),
            PyObject::Module { name, .. } => write!(f, "<module {}>", name),
            PyObject::Iterator(_) => write!(f, "<iterator object>"),
            PyObject::Slice { start, stop, step } => {
                write!(f, "slice(")?;
                if let Some(s) = start {
                    write!(f, "{}, ", s)?;
                } else {
                    write!(f, "None, ")?;
                }
                if let Some(s) = stop {
                    write!(f, "{}, ", s)?;
                } else {
                    write!(f, "None, ")?;
                }
                if let Some(s) = step {
                    write!(f, "{}", s)?;
                } else {
                    write!(f, "None")?;
                }
                write!(f, ")")
            }
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

impl PyObject {
    pub fn to_iterator(&self) -> Option<Rc<RefCell<PyIterator>>> {
        match self {
            PyObject::List(l) => Some(Rc::new(RefCell::new(PyIterator::List(l.clone(), 0)))),
            PyObject::String(s) => Some(Rc::new(RefCell::new(PyIterator::String(s.clone(), 0)))),
            PyObject::Iterator(it) => Some(it.clone()),
            _ => None,
        }
    }
}
