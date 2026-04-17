use crate::object::PyObject;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

pub struct Environment {
    values: HashMap<String, PyObject>,
    parent: Option<Arc<RwLock<Environment>>>,
}

impl Environment {
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
            parent: None,
        }
    }

    pub fn with_parent(parent: Arc<RwLock<Environment>>) -> Self {
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
            return parent.read().get(name);
        }

        None
    }

    #[allow(dead_code)]
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
            return parent.write().assign(name, value);
        }

        false
    }
}
