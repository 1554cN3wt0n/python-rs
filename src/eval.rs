use crate::ast::{BinaryOp, Expr, Literal, LogicalOp, Stmt};
use crate::env::Environment;
use crate::object::PyObject;
use anyhow::{Result, anyhow};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub struct Evaluator {
    global_env: Rc<RefCell<Environment>>,
}

impl Evaluator {
    pub fn new() -> Self {
        let global_env = Rc::new(RefCell::new(Environment::new()));

        // Register built-ins
        global_env.borrow_mut().define(
            "print".to_string(),
            PyObject::BuiltinFunction(Rc::new(|args| {
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        print!(" ");
                    }
                    print!("{}", arg);
                }
                println!();
                PyObject::None
            })),
        );

        global_env.borrow_mut().define(
            "len".to_string(),
            PyObject::BuiltinFunction(Rc::new(|args| {
                if args.len() != 1 {
                    return PyObject::None; // Should ideally be an error
                }
                match &args[0] {
                    PyObject::String(s) => PyObject::Int(s.len() as i64),
                    PyObject::List(l) => PyObject::Int(l.borrow().len() as i64),
                    PyObject::Dict(d) => PyObject::Int(d.borrow().len() as i64),
                    _ => PyObject::None,
                }
            })),
        );

        global_env.borrow_mut().define(
            "range".to_string(),
            PyObject::BuiltinFunction(Rc::new(|args| {
                let (start, stop) = match args.len() {
                    1 => (0, args[0].as_int().cloned().unwrap_or(0)),
                    2 => (
                        args[0].as_int().cloned().unwrap_or(0),
                        args[1].as_int().cloned().unwrap_or(0),
                    ),
                    _ => (0, 0),
                };
                let mut items = Vec::new();
                for i in start..stop {
                    items.push(PyObject::Int(i));
                }
                PyObject::List(Rc::new(RefCell::new(items)))
            })),
        );

        global_env.borrow_mut().define(
            "str".to_string(),
            PyObject::BuiltinFunction(Rc::new(|args| {
                if args.len() != 1 {
                    return PyObject::None;
                }
                PyObject::String(args[0].to_string())
            })),
        );

        Self { global_env }
    }

    pub fn eval(&mut self, statements: &[Stmt]) -> Result<PyObject> {
        self.eval_statements(statements, self.global_env.clone())
    }

    fn eval_statements(
        &mut self,
        statements: &[Stmt],
        env: Rc<RefCell<Environment>>,
    ) -> Result<PyObject> {
        let last_value = PyObject::None;
        for stmt in statements {
            if let Some(val) = self.eval_statement(stmt, env.clone())? {
                return Ok(val); // Early return (e.g., from 'return' statement)
            }
        }
        Ok(last_value)
    }

    fn eval_statement(
        &mut self,
        stmt: &Stmt,
        env: Rc<RefCell<Environment>>,
    ) -> Result<Option<PyObject>> {
        match stmt {
            Stmt::Expression(expr) => {
                self.eval_expression(expr, env)?;
                Ok(None)
            }
            Stmt::Assignment(target_expr, value_expr) => {
                let value = self.eval_expression(value_expr, env.clone())?;
                match target_expr {
                    Expr::Variable(name) => {
                        env.borrow_mut().define(name.clone(), value);
                    }
                    Expr::Subscript(target, index_expr) => {
                        let target_val = self.eval_expression(target, env.clone())?;
                        let index_val = self.eval_expression(index_expr, env.clone())?;
                        match target_val {
                            PyObject::List(l) => {
                                let idx = index_val
                                    .as_int()
                                    .ok_or_else(|| anyhow!("List index must be an integer"))?;
                                l.borrow_mut()[*idx as usize] = value;
                            }
                            PyObject::Dict(d) => {
                                let key = index_val.to_string(); // Simple string key for now
                                d.borrow_mut().insert(key, value);
                            }
                            _ => return Err(anyhow!("Object does not support item assignment")),
                        }
                    }
                    Expr::Attribute(target, attr) => {
                        let target_val = self.eval_expression(target, env.clone())?;
                        if let PyObject::Instance { attributes, .. } = target_val {
                            attributes.borrow_mut().insert(attr.clone(), value);
                        } else {
                            return Err(anyhow!("Object has no attributes"));
                        }
                    }
                    _ => return Err(anyhow!("Invalid assignment target")),
                }
                Ok(None)
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let cond_val = self.eval_expression(condition, env.clone())?;
                if self.is_truthy(&cond_val) {
                    return self.eval_block(then_branch, env);
                } else if let Some(else_branch) = else_branch {
                    return self.eval_block(else_branch, env);
                }
                Ok(None)
            }
            Stmt::While { condition, body } => {
                loop {
                    let cond_val = self.eval_expression(condition, env.clone())?;
                    if !self.is_truthy(&cond_val) {
                        break;
                    }
                    if let Some(val) = self.eval_block(body, env.clone())? {
                        return Ok(Some(val));
                    }
                }
                Ok(None)
            }
            Stmt::For {
                target,
                iterable,
                body,
            } => {
                let iter_val = self.eval_expression(iterable, env.clone())?;
                let items = match iter_val {
                    PyObject::List(l) => l.borrow().clone(),
                    _ => return Err(anyhow!("Object is not iterable")),
                };

                for item in items {
                    env.borrow_mut().define(target.clone(), item);
                    if let Some(val) = self.eval_block(body, env.clone())? {
                        return Ok(Some(val));
                    }
                }
                Ok(None)
            }
            Stmt::FunctionDef { name, params, body } => {
                let func = PyObject::Function {
                    name: name.clone(),
                    params: params.clone(),
                    body: body.clone(),
                };
                env.borrow_mut().define(name.clone(), func);
                Ok(None)
            }
            Stmt::ClassDef { name, methods } => {
                let mut class_methods = HashMap::new();
                for stmt in methods {
                    if let Stmt::FunctionDef { name: m_name, .. } = stmt {
                        // Create method
                        if let Some(PyObject::Function { name, params, body }) =
                            self.eval_statement(stmt, env.clone())?
                        {
                            class_methods
                                .insert(m_name.clone(), PyObject::Function { name, params, body });
                        } else {
                            // eval_statement for FunctionDef returns None but defines it in env.
                            // We need it as an object.
                            let func = PyObject::Function {
                                name: m_name.clone(),
                                params: match stmt {
                                    Stmt::FunctionDef { params, .. } => params.clone(),
                                    _ => unreachable!(),
                                },
                                body: match stmt {
                                    Stmt::FunctionDef { body, .. } => body.clone(),
                                    _ => unreachable!(),
                                },
                            };
                            class_methods.insert(m_name.clone(), func);
                        }
                    }
                }
                let class = PyObject::Class {
                    name: name.clone(),
                    methods: class_methods,
                };
                env.borrow_mut().define(name.clone(), class);
                Ok(None)
            }
            Stmt::Return(expr) => {
                let val = if let Some(e) = expr {
                    self.eval_expression(e, env)?
                } else {
                    PyObject::None
                };
                Ok(Some(val))
            }
        }
    }

    fn eval_block(
        &mut self,
        statements: &[Stmt],
        env: Rc<RefCell<Environment>>,
    ) -> Result<Option<PyObject>> {
        for stmt in statements {
            if let Some(val) = self.eval_statement(stmt, env.clone())? {
                return Ok(Some(val));
            }
        }
        Ok(None)
    }

    fn eval_expression(&mut self, expr: &Expr, env: Rc<RefCell<Environment>>) -> Result<PyObject> {
        match expr {
            Expr::Literal(lit) => match lit {
                Literal::Int(n) => Ok(PyObject::Int(*n)),
                Literal::String(s) => Ok(PyObject::String(s.clone())),
                Literal::Bool(b) => Ok(PyObject::Bool(*b)),
                Literal::None => Ok(PyObject::None),
            },
            Expr::Variable(name) => env
                .borrow()
                .get(name)
                .ok_or_else(|| anyhow!("Undefined variable: {}", name)),
            Expr::Binary(left, op, right) => {
                let l = self.eval_expression(left, env.clone())?;
                let r = self.eval_expression(right, env.clone())?;
                self.eval_binary_op(l, op, r)
            }
            Expr::Logical(left, op, right) => {
                let l = self.eval_expression(left, env.clone())?;
                match op {
                    LogicalOp::And => {
                        if !self.is_truthy(&l) {
                            return Ok(l);
                        }
                    }
                    LogicalOp::Or => {
                        if self.is_truthy(&l) {
                            return Ok(l);
                        }
                    }
                    _ => unreachable!(),
                }
                self.eval_expression(right, env)
            }
            Expr::Unary(LogicalOp::Not, expr) => {
                let val = self.eval_expression(expr, env)?;
                Ok(PyObject::Bool(!self.is_truthy(&val)))
            }
            Expr::Unary(_, _) => unreachable!(),
            Expr::Call(callee_expr, args) => {
                let func = self.eval_expression(callee_expr, env.clone())?;
                let mut evaluated_args = Vec::new();
                for arg in args {
                    evaluated_args.push(self.eval_expression(arg, env.clone())?);
                }

                match func {
                    PyObject::BuiltinFunction(f) => Ok(f(evaluated_args)),
                    PyObject::Function { params, body, .. } => {
                        let mut call_args = evaluated_args;

                        // Check if this was an attribute access to bind 'self'
                        if let Expr::Attribute(target_expr, _) = &**callee_expr {
                            let target_val = self.eval_expression(target_expr, env.clone())?;
                            if let PyObject::Instance { .. } = target_val {
                                // Simple binding: insert instance as first arg
                                call_args.insert(0, target_val);
                            }
                        }

                        if params.len() != call_args.len() {
                            return Err(anyhow!(
                                "Expected {} arguments, got {}",
                                params.len(),
                                call_args.len()
                            ));
                        }
                        let call_env = Rc::new(RefCell::new(Environment::with_parent(
                            self.global_env.clone(),
                        )));
                        for (param, arg) in params.iter().zip(call_args) {
                            call_env.borrow_mut().define(param.clone(), arg);
                        }
                        self.eval_statements(&body, call_env)
                    }
                    PyObject::Class { ref methods, .. } => {
                        // Instantiation
                        let instance = PyObject::Instance {
                            class: Rc::new(RefCell::new(func.clone())),
                            attributes: Rc::new(RefCell::new(HashMap::new())),
                        };
                        // Call __init__ if exists
                        if let Some(PyObject::Function { params, body, .. }) =
                            methods.get("__init__")
                        {
                            let mut call_args = vec![instance.clone()];
                            call_args.extend(evaluated_args);
                            if params.len() != call_args.len() {
                                return Err(anyhow!("__init__ expected {} args", params.len()));
                            }
                            let call_env = Rc::new(RefCell::new(Environment::with_parent(
                                self.global_env.clone(),
                            )));
                            for (param, arg) in params.iter().zip(call_args) {
                                call_env.borrow_mut().define(param.clone(), arg);
                            }
                            self.eval_statements(body, call_env)?;
                        }
                        Ok(instance)
                    }
                    _ => Err(anyhow!("Object is not callable")),
                }
            }
            Expr::List(items) => {
                let mut evaluated_items = Vec::new();
                for item in items {
                    evaluated_items.push(self.eval_expression(item, env.clone())?);
                }
                Ok(PyObject::List(Rc::new(RefCell::new(evaluated_items))))
            }
            Expr::Dict(items) => {
                let mut evaluated_items = HashMap::new();
                for (key_expr, val_expr) in items {
                    let key = self.eval_expression(key_expr, env.clone())?.to_string();
                    let val = self.eval_expression(val_expr, env.clone())?;
                    evaluated_items.insert(key, val);
                }
                Ok(PyObject::Dict(Rc::new(RefCell::new(evaluated_items))))
            }
            Expr::Subscript(target, index_expr) => {
                let val = self.eval_expression(target, env.clone())?;
                let index = self.eval_expression(index_expr, env.clone())?;
                match val {
                    PyObject::List(l) => {
                        let i = index
                            .as_int()
                            .ok_or_else(|| anyhow!("Index must be an integer"))?;
                        Ok(l.borrow()[*i as usize].clone())
                    }
                    PyObject::Dict(d) => {
                        let key = index.to_string();
                        Ok(d.borrow()
                            .get(&key)
                            .cloned()
                            .ok_or_else(|| anyhow!("Key not found: {}", key))?)
                    }
                    PyObject::String(s) => {
                        let i = index
                            .as_int()
                            .ok_or_else(|| anyhow!("Index must be an integer"))?;
                        let char = s
                            .chars()
                            .nth(*i as usize)
                            .ok_or_else(|| anyhow!("Index out of range"))?;
                        Ok(PyObject::String(char.to_string()))
                    }
                    _ => Err(anyhow!("Object is not subscriptable")),
                }
            }
            Expr::Attribute(target, attr) => {
                let val = self.eval_expression(target, env.clone())?;
                match val {
                    PyObject::Instance { class, attributes } => {
                        if let Some(v) = attributes.borrow().get(attr) {
                            return Ok(v.clone());
                        }
                        let class_borrow = class.borrow();
                        if let PyObject::Class { methods, .. } = &*class_borrow
                            && let Some(method) = methods.get(attr)
                        {
                            // Bind method? (Omitted for simplicity, just return func)
                            return Ok(method.clone());
                        }
                        Err(anyhow!("Attribute '{}' not found", attr))
                    }
                    PyObject::List(l) => {
                        if attr == "push" {
                            let l_clone = l.clone();
                            return Ok(PyObject::BuiltinFunction(Rc::new(move |args| {
                                if let Some(val) = args.first() {
                                    l_clone.borrow_mut().push(val.clone());
                                }
                                PyObject::None
                            })));
                        }
                        Err(anyhow!("List has no attribute '{}'", attr))
                    }
                    _ => Err(anyhow!("Object has no attributes")),
                }
            }
        }
    }

    fn eval_binary_op(&self, left: PyObject, op: &BinaryOp, right: PyObject) -> Result<PyObject> {
        if let BinaryOp::In = op {
            return match right {
                PyObject::List(l) => {
                    let borrow = l.borrow();
                    Ok(PyObject::Bool(borrow.contains(&left)))
                }
                PyObject::Dict(d) => {
                    let key = left.to_string();
                    let borrow = d.borrow();
                    Ok(PyObject::Bool(borrow.contains_key(&key)))
                }
                PyObject::String(s) => {
                    let sub = left.to_string();
                    Ok(PyObject::Bool(s.contains(&sub)))
                }
                _ => Err(anyhow!("Object of type {:?} is not iterable", right)),
            };
        }
        match (left, right) {
            (PyObject::Int(l), PyObject::Int(r)) => match op {
                BinaryOp::Add => Ok(PyObject::Int(l + r)),
                BinaryOp::Sub => Ok(PyObject::Int(l - r)),
                BinaryOp::Mul => Ok(PyObject::Int(l * r)),
                BinaryOp::Div => Ok(PyObject::Int(l / r)),
                BinaryOp::Equal => Ok(PyObject::Bool(l == r)),
                BinaryOp::NotEqual => Ok(PyObject::Bool(l != r)),
                BinaryOp::Less => Ok(PyObject::Bool(l < r)),
                BinaryOp::Greater => Ok(PyObject::Bool(l > r)),
                BinaryOp::LessEqual => Ok(PyObject::Bool(l <= r)),
                BinaryOp::GreaterEqual => Ok(PyObject::Bool(l >= r)),
                BinaryOp::In => unreachable!(),
            },
            (PyObject::String(l), PyObject::String(r)) => match op {
                BinaryOp::Add => Ok(PyObject::String(format!("{}{}", l, r))),
                BinaryOp::Equal => Ok(PyObject::Bool(l == r)),
                BinaryOp::NotEqual => Ok(PyObject::Bool(l != r)),
                BinaryOp::In => unreachable!(),
                _ => Err(anyhow!("Invalid operator for strings")),
            },
            (l, r) => match op {
                BinaryOp::Equal => Ok(PyObject::Bool(l == r)),
                BinaryOp::NotEqual => Ok(PyObject::Bool(l != r)),
                BinaryOp::In => unreachable!(),
                _ => Err(anyhow!("Unsupported types for operation")),
            },
        }
    }

    fn is_truthy(&self, obj: &PyObject) -> bool {
        match obj {
            PyObject::None => false,
            PyObject::Bool(b) => *b,
            PyObject::Int(n) => *n != 0,
            PyObject::String(s) => !s.is_empty(),
            PyObject::List(l) => !l.borrow().is_empty(),
            PyObject::Dict(d) => !d.borrow().is_empty(),
            _ => true,
        }
    }
}
