use crate::object::PyObject;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub struct Environment {
    values: HashMap<String, PyObject>,
    parent: Option<Rc<RefCell<Environment>>>,
}

impl Environment {
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
            parent: None,
        }
    }

    pub fn with_parent(parent: Rc<RefCell<Environment>>) -> Self {
        Self {
            values: HashMap::new(),
            parent: Some(parent),
        }
    }

    pub fn define(&mut self, name: String, value: PyObject) {
        self.values.insert(name, value);
    }

    pub fn get(&self, name: &str) -> Option<PyObject> {
        if let Some(value) = self.values.get(name) {
            return Some(value.clone());
        }

        if let Some(parent) = &self.parent {
            return parent.borrow().get(name);
        }

        None
    }

    pub fn values(&self) -> HashMap<String, PyObject> {
        self.values.clone()
    }

    #[allow(dead_code)]
    pub fn assign(&mut self, name: String, value: PyObject) -> bool {
        if let Some(v) = self.values.get_mut(&name) {
            *v = value;
            return true;
        }

        if let Some(parent) = &self.parent {
            return parent.borrow_mut().assign(name, value);
        }

        false
    }
}
