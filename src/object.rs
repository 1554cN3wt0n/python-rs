use crate::ast::Stmt;
use enum_as_inner::EnumAsInner;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

pub trait PyCallableContext {
    fn call_method(
        &mut self,
        obj: &PyObject,
        name: &str,
        args: Vec<PyObject>,
    ) -> anyhow::Result<PyObject>;
    fn call_func(&mut self, func: &PyObject, args: Vec<PyObject>) -> anyhow::Result<PyObject>;
    fn is_subclass(&self, child: &PyObject, parent: &PyObject) -> bool;
    fn is_truthy(&self, obj: &PyObject) -> bool;
}

pub type BuiltinFunc =
    Rc<dyn Fn(&mut dyn PyCallableContext, Vec<PyObject>) -> anyhow::Result<PyObject>>;

#[derive(Clone, EnumAsInner)]
pub enum PyObject {
    Int(i64),
    String(String),
    Bool(bool),
    List(Rc<RefCell<Vec<PyObject>>>),
    Tuple(Vec<PyObject>),
    Dict(Rc<RefCell<HashMap<String, PyObject>>>),
    Set(Rc<RefCell<HashSet<PyObject>>>),
    Function {
        name: String,
        params: Vec<String>,
        body: Vec<Stmt>,
    },
    BuiltinFunction(BuiltinFunc),
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
    Enumerate(Rc<RefCell<PyIterator>>, i64),
    Zip(Vec<Rc<RefCell<PyIterator>>>),
    Map(PyObject, Vec<Rc<RefCell<PyIterator>>>),
    Filter(PyObject, Rc<RefCell<PyIterator>>),
    Custom(PyObject),
}

impl PyIterator {
    pub fn next(&mut self, ctx: &mut dyn PyCallableContext) -> anyhow::Result<Option<PyObject>> {
        match self {
            PyIterator::List(l, idx) => {
                let items = l.borrow();
                if *idx < items.len() {
                    let val = items[*idx].clone();
                    *idx += 1;
                    Ok(Some(val))
                } else {
                    Ok(None)
                }
            }
            PyIterator::String(s, idx) => {
                let chars: Vec<char> = s.chars().collect();
                if *idx < chars.len() {
                    let val = PyObject::String(chars[*idx].to_string());
                    *idx += 1;
                    Ok(Some(val))
                } else {
                    Ok(None)
                }
            }
            PyIterator::Range(curr, stop, step) => {
                if (*step > 0 && *curr < *stop) || (*step < 0 && *curr > *stop) {
                    let val = PyObject::Int(*curr);
                    *curr += *step;
                    Ok(Some(val))
                } else {
                    Ok(None)
                }
            }
            PyIterator::Enumerate(it, count) => {
                let mut it_borrow = it.borrow_mut();
                if let Some(val) = it_borrow.next(ctx)? {
                    let res = PyObject::Tuple(vec![PyObject::Int(*count), val]);
                    *count += 1;
                    Ok(Some(res))
                } else {
                    Ok(None)
                }
            }
            PyIterator::Zip(its) => {
                let mut results = Vec::new();
                for it in its {
                    if let Some(val) = it.borrow_mut().next(ctx)? {
                        results.push(val);
                    } else {
                        return Ok(None);
                    }
                }
                Ok(Some(PyObject::Tuple(results)))
            }
            PyIterator::Map(func, its) => {
                let mut args = Vec::new();
                for it in its {
                    if let Some(val) = it.borrow_mut().next(ctx)? {
                        args.push(val);
                    } else {
                        return Ok(None);
                    }
                }
                Ok(Some(ctx.call_func(func, args)?))
            }
            PyIterator::Filter(func, it) => {
                let mut it_borrow = it.borrow_mut();
                while let Some(val) = it_borrow.next(ctx)? {
                    let truthy = match func {
                        PyObject::None => val.clone(),
                        _ => ctx.call_func(func, vec![val.clone()])?,
                    };
                    if ctx.is_truthy(&truthy) {
                        return Ok(Some(val));
                    }
                }
                Ok(None)
            }
            PyIterator::Custom(obj) => match ctx.call_method(obj, "__next__", vec![obj.clone()]) {
                Ok(res) => Ok(Some(res)),
                Err(e) => {
                    if e.to_string().contains("StopIteration") {
                        Ok(None)
                    } else {
                        Err(e)
                    }
                }
            },
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
            PyObject::Tuple(t) => f.debug_tuple("Tuple").field(t).finish(),
            PyObject::Dict(d) => f.debug_tuple("Dict").field(d).finish(),
            PyObject::Set(s) => f.debug_tuple("Set").field(s).finish(),
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
            (PyObject::Tuple(a), PyObject::Tuple(b)) => a == b,
            (PyObject::Dict(a), PyObject::Dict(b)) => {
                Rc::ptr_eq(a, b) || *a.borrow() == *b.borrow()
            }
            (PyObject::Set(a), PyObject::Set(b)) => Rc::ptr_eq(a, b) || *a.borrow() == *b.borrow(),
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

impl Eq for PyObject {}

impl Hash for PyObject {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            PyObject::Int(n) => {
                0u8.hash(state);
                n.hash(state);
            }
            PyObject::String(s) => {
                1u8.hash(state);
                s.hash(state);
            }
            PyObject::Bool(b) => {
                2u8.hash(state);
                b.hash(state);
            }
            PyObject::None => {
                3u8.hash(state);
            }
            PyObject::Tuple(items) => {
                4u8.hash(state);
                for item in items {
                    item.hash(state);
                }
            }
            PyObject::Type(s) => {
                5u8.hash(state);
                s.hash(state);
            }
            PyObject::Class { name, .. } => {
                6u8.hash(state);
                name.hash(state);
            }
            PyObject::Function { name, .. } => {
                7u8.hash(state);
                name.hash(state);
            }
            PyObject::BuiltinFunction(_) => {
                8u8.hash(state);
                // Hash by address if we could, but let's just use a placeholder
            }
            _ => {
                // Unhashable types should be checked before calling hash()
                // In Python, this raises TypeError. Here we might want to panic if reached,
                // but we should ideally prevent this in the evaluator.
            }
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
            PyObject::Tuple(items) => {
                write!(f, "(")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                if items.len() == 1 {
                    write!(f, ",")?;
                }
                write!(f, ")")
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
            PyObject::Set(s) => {
                write!(f, "{{")?;
                let items = s.borrow();
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                if items.is_empty() {
                    write!(f, "set()")?;
                } else {
                    write!(f, "}}")?;
                }
                Ok(())
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
    pub fn to_iterator(&self, ctx: &mut dyn PyCallableContext) -> Option<Rc<RefCell<PyIterator>>> {
        match self {
            PyObject::List(l) => Some(Rc::new(RefCell::new(PyIterator::List(l.clone(), 0)))),
            PyObject::Tuple(t) => Some(Rc::new(RefCell::new(PyIterator::List(
                Rc::new(RefCell::new(t.clone())),
                0,
            )))),
            PyObject::String(s) => Some(Rc::new(RefCell::new(PyIterator::String(s.clone(), 0)))),
            PyObject::Set(s) => Some(Rc::new(RefCell::new(PyIterator::List(
                Rc::new(RefCell::new(s.borrow().iter().cloned().collect())),
                0,
            )))),
            PyObject::Iterator(it) => Some(it.clone()),
            _ => {
                // Check for __iter__
                if let Ok(res) = ctx.call_method(self, "__iter__", vec![self.clone()]) {
                    if let PyObject::Iterator(it) = res {
                        return Some(it);
                    }
                    // Handle case where __iter__ returns another object that is an iterator
                    return Some(Rc::new(RefCell::new(PyIterator::Custom(res))));
                }
                None
            }
        }
    }

    pub fn is_hashable(&self) -> bool {
        match self {
            PyObject::Int(_)
            | PyObject::String(_)
            | PyObject::Bool(_)
            | PyObject::None
            | PyObject::Type(_)
            | PyObject::Class { .. }
            | PyObject::Function { .. }
            | PyObject::BuiltinFunction(_) => true,
            PyObject::Tuple(items) => items.iter().all(|item| item.is_hashable()),
            _ => false,
        }
    }
}
