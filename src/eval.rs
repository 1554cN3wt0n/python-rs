use crate::ast::{BinaryOp, Expr, Literal, LogicalOp, Stmt, UnaryOp};
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

        global_env.borrow_mut().define(
            "type".to_string(),
            PyObject::BuiltinFunction(Rc::new(|args| {
                if args.len() != 1 {
                    return PyObject::None;
                }
                match &args[0] {
                    PyObject::Int(_) => PyObject::String("int".to_string()),
                    PyObject::String(_) => PyObject::String("str".to_string()),
                    PyObject::Bool(_) => PyObject::String("bool".to_string()),
                    PyObject::List(_) => PyObject::String("list".to_string()),
                    PyObject::Dict(_) => PyObject::String("dict".to_string()),
                    PyObject::Instance { class, .. } => class.borrow().clone(),
                    PyObject::Class { .. } => args[0].clone(),
                    PyObject::Type(_) => args[0].clone(),
                    PyObject::Module { .. } => args[0].clone(),
                    PyObject::Slice { .. } => PyObject::Type("slice".to_string()),
                    PyObject::Function { .. } => PyObject::String("function".to_string()),
                    PyObject::BuiltinFunction(_) => {
                        PyObject::String("builtin_function".to_string())
                    }
                    PyObject::None => PyObject::String("NoneType".to_string()),
                }
            })),
        );

        global_env.borrow_mut().define(
            "isinstance".to_string(),
            PyObject::BuiltinFunction(Rc::new(|_| PyObject::None)),
        );

        global_env.borrow_mut().define(
            "hasattr".to_string(),
            PyObject::BuiltinFunction(Rc::new(|_| PyObject::None)),
        );

        // Primitive type markers
        global_env
            .borrow_mut()
            .define("int".to_string(), PyObject::Type("int".to_string()));
        global_env
            .borrow_mut()
            .define("str".to_string(), PyObject::Type("str".to_string()));
        global_env
            .borrow_mut()
            .define("bool".to_string(), PyObject::Type("bool".to_string()));
        global_env
            .borrow_mut()
            .define("list".to_string(), PyObject::Type("list".to_string()));
        global_env
            .borrow_mut()
            .define("dict".to_string(), PyObject::Type("dict".to_string()));
        global_env.borrow_mut().define(
            "NoneType".to_string(),
            PyObject::Type("NoneType".to_string()),
        );

        // File I/O
        global_env.borrow_mut().define(
            "open".to_string(),
            PyObject::BuiltinFunction(Rc::new(|args| {
                if args.is_empty() {
                    return PyObject::None;
                }
                let filename = args[0].to_string();

                let mut attributes = HashMap::new();
                let f_name = filename.clone();

                attributes.insert(
                    "read".to_string(),
                    PyObject::BuiltinFunction(Rc::new(move |_| {
                        std::fs::read_to_string(&f_name)
                            .map(PyObject::String)
                            .unwrap_or(PyObject::None)
                    })),
                );

                let f_name2 = filename.clone();
                attributes.insert(
                    "write".to_string(),
                    PyObject::BuiltinFunction(Rc::new(move |f_args| {
                        if let Some(PyObject::String(content)) = f_args.first() {
                            std::fs::write(&f_name2, content).ok();
                        }
                        PyObject::None
                    })),
                );

                PyObject::Instance {
                    class: Rc::new(RefCell::new(PyObject::Type("file".to_string()))),
                    attributes: Rc::new(RefCell::new(attributes)),
                }
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
            Stmt::ClassDef {
                name,
                bases,
                methods,
            } => {
                let mut evaluated_bases = Vec::new();
                for base_expr in bases {
                    evaluated_bases.push(self.eval_expression(base_expr, env.clone())?);
                }

                let mut class_methods = HashMap::new();
                for stmt in methods {
                    if let Stmt::FunctionDef { name: m_name, .. } = stmt {
                        // Create method
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
                let class = PyObject::Class {
                    name: name.clone(),
                    bases: evaluated_bases,
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
            Stmt::Import(name) => {
                let filename = format!("{}.pyrs", name);
                let content = std::fs::read_to_string(&filename)
                    .map_err(|e| anyhow!("Could not import module '{}': {}", name, e))?;

                let lexer = crate::lexer::Lexer::new(&content);
                let mut parser = crate::parser::Parser::new(lexer);
                let statements = parser.parse()?;

                let mut module_evaluator = Evaluator::new();
                module_evaluator.eval(&statements)?;

                let module = PyObject::Module {
                    name: name.clone(),
                    env: Rc::new(RefCell::new(module_evaluator.global_env.borrow().values())),
                };
                env.borrow_mut().define(name.clone(), module);
                Ok(None)
            }
            Stmt::Try { body, handlers } => {
                match self.eval_block(body, env.clone()) {
                    Ok(val) => Ok(val),
                    Err(e) => {
                        for handler in handlers {
                            let mut matches = true;
                            if let Some(type_expr) = &handler.type_ {
                                let handler_type = self.eval_expression(type_expr, env.clone())?;
                                // Simple matching for now: match if it's a Type or Class
                                matches = matches
                                    && (matches!(
                                        handler_type,
                                        PyObject::Type(_) | PyObject::Class { .. }
                                    ));
                            }
                            if matches {
                                let handler_env =
                                    Rc::new(RefCell::new(Environment::with_parent(env.clone())));
                                if let Some(name) = &handler.name {
                                    handler_env
                                        .borrow_mut()
                                        .define(name.clone(), PyObject::String(e.to_string()));
                                }
                                return self.eval_block(&handler.body, handler_env);
                            }
                        }
                        Err(e)
                    }
                }
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
            Expr::Unary(op, expr) => {
                let val = self.eval_expression(expr, env)?;
                match op {
                    UnaryOp::Not => Ok(PyObject::Bool(!self.is_truthy(&val))),
                    UnaryOp::Neg => match val {
                        PyObject::Int(n) => Ok(PyObject::Int(-n)),
                        _ => Err(anyhow!("Unary minus only supports integers")),
                    },
                    UnaryOp::Pos => match val {
                        PyObject::Int(n) => Ok(PyObject::Int(n)),
                        _ => Err(anyhow!("Unary plus only supports integers")),
                    },
                }
            }
            Expr::Call(callee, args) => {
                let callee_val = self.eval_expression(callee, env.clone())?;
                let mut evaluated_args = Vec::new();
                for arg in args {
                    evaluated_args.push(self.eval_expression(arg, env.clone())?);
                }

                // Handle special built-ins that need Evaluator access
                if let Expr::Variable(name) = &**callee {
                    match name.as_str() {
                        "isinstance" => {
                            if evaluated_args.len() == 2 {
                                let obj = &evaluated_args[0];
                                let target_type = &evaluated_args[1];

                                match (obj, target_type) {
                                    (PyObject::Int(_), PyObject::Type(s)) if s == "int" => {
                                        return Ok(PyObject::Bool(true));
                                    }
                                    (PyObject::String(_), PyObject::Type(s)) if s == "str" => {
                                        return Ok(PyObject::Bool(true));
                                    }
                                    (PyObject::Bool(_), PyObject::Type(s)) if s == "bool" => {
                                        return Ok(PyObject::Bool(true));
                                    }
                                    (PyObject::List(_), PyObject::Type(s)) if s == "list" => {
                                        return Ok(PyObject::Bool(true));
                                    }
                                    (PyObject::Dict(_), PyObject::Type(s)) if s == "dict" => {
                                        return Ok(PyObject::Bool(true));
                                    }
                                    (PyObject::None, PyObject::Type(s)) if s == "NoneType" => {
                                        return Ok(PyObject::Bool(true));
                                    }
                                    (PyObject::Instance { class, .. }, _) => {
                                        return Ok(PyObject::Bool(
                                            self.is_subclass(&class.borrow(), target_type),
                                        ));
                                    }
                                    _ => return Ok(PyObject::Bool(false)),
                                }
                            }
                            return Ok(PyObject::Bool(false));
                        }
                        "hasattr" => {
                            if evaluated_args.len() == 2 {
                                let attr_name = evaluated_args[1].to_string();
                                match &evaluated_args[0] {
                                    PyObject::Instance { class, attributes } => {
                                        if attributes.borrow().contains_key(&attr_name) {
                                            return Ok(PyObject::Bool(true));
                                        }
                                        return Ok(PyObject::Bool(
                                            self.find_method(&class.borrow(), &attr_name).is_some(),
                                        ));
                                    }
                                    PyObject::Class { .. } => {
                                        return Ok(PyObject::Bool(
                                            self.find_method(&evaluated_args[0], &attr_name)
                                                .is_some(),
                                        ));
                                    }
                                    _ => return Ok(PyObject::Bool(false)),
                                }
                            }
                            return Ok(PyObject::Bool(false));
                        }
                        "type" => {
                            if evaluated_args.len() == 1 {
                                match &evaluated_args[0] {
                                    PyObject::Int(_) => {
                                        return Ok(PyObject::Type("int".to_string()));
                                    }
                                    PyObject::String(_) => {
                                        return Ok(PyObject::Type("str".to_string()));
                                    }
                                    PyObject::Bool(_) => {
                                        return Ok(PyObject::Type("bool".to_string()));
                                    }
                                    PyObject::List(_) => {
                                        return Ok(PyObject::Type("list".to_string()));
                                    }
                                    PyObject::Dict(_) => {
                                        return Ok(PyObject::Type("dict".to_string()));
                                    }
                                    PyObject::None => {
                                        return Ok(PyObject::Type("NoneType".to_string()));
                                    }
                                    PyObject::Instance { class, .. } => {
                                        return Ok(class.borrow().clone());
                                    }
                                    PyObject::Class { .. } => return Ok(evaluated_args[0].clone()),
                                    _ => {
                                        return Err(anyhow!(
                                            "type() not fully implemented for this type"
                                        ));
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }

                match callee_val {
                    PyObject::BuiltinFunction(f) => Ok(f(evaluated_args)),
                    PyObject::Type(name) => {
                        // Constructor calls
                        if evaluated_args.len() != 1 {
                            return Err(anyhow!("Type constructor expected 1 argument"));
                        }
                        match name.as_str() {
                            "str" => Ok(PyObject::String(evaluated_args[0].to_string())),
                            "int" => match &evaluated_args[0] {
                                PyObject::Int(n) => Ok(PyObject::Int(*n)),
                                PyObject::String(s) => Ok(PyObject::Int(s.parse().unwrap_or(0))),
                                _ => Err(anyhow!("Could not convert to int")),
                            },
                            _ => Err(anyhow!("Constructor not implemented for type {}", name)),
                        }
                    }
                    PyObject::Function { params, body, .. } => {
                        let mut call_args = evaluated_args;

                        // Check if this was an attribute access to bind 'self'
                        if let Expr::Attribute(target_expr, _) = &**callee {
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
                            class: Rc::new(RefCell::new(callee_val.clone())),
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
                    _ => Err(anyhow!("Object is not callable: {:?}", callee_val)),
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
                let left = self.eval_expression(target, env.clone())?;
                let index = self.eval_expression(index_expr, env.clone())?;
                let res = match (left, index) {
                    (PyObject::List(l), PyObject::Int(i)) => {
                        let items = l.borrow();
                        let mut idx = i;
                        if idx < 0 {
                            idx += items.len() as i64;
                        }
                        items
                            .get(idx as usize)
                            .cloned()
                            .ok_or_else(|| anyhow!("List index out of range"))
                    }
                    (PyObject::List(l), PyObject::Slice { start, stop, step }) => {
                        let items = l.borrow();
                        let len = items.len() as i64;

                        let step_val = match step.as_deref() {
                            Some(PyObject::Int(n)) => *n,
                            Some(PyObject::None) | None => 1,
                            _ => return Err(anyhow!("slice step must be an integer")),
                        };
                        if step_val == 0 {
                            return Err(anyhow!("slice step cannot be zero"));
                        }

                        let mut start_val = match start.as_deref() {
                            Some(PyObject::Int(n)) => *n,
                            Some(PyObject::None) | None => {
                                if step_val > 0 {
                                    0
                                } else {
                                    len - 1
                                }
                            }
                            _ => return Err(anyhow!("slice start must be an integer")),
                        };

                        let mut stop_val = match stop.as_deref() {
                            Some(PyObject::Int(n)) => *n,
                            Some(PyObject::None) | None => {
                                if step_val > 0 {
                                    len
                                } else {
                                    -1
                                }
                            }
                            _ => return Err(anyhow!("slice stop must be an integer")),
                        };

                        if start_val < 0 {
                            start_val += len;
                        }
                        if stop_val < 0 && stop.is_some() {
                            stop_val += len;
                        }

                        let mut result = Vec::new();
                        let mut curr = start_val;
                        if step_val > 0 {
                            while curr < stop_val && curr < len {
                                if curr >= 0 {
                                    result.push(items[curr as usize].clone());
                                }
                                curr += step_val;
                            }
                        } else {
                            while curr > stop_val && curr >= 0 {
                                if curr < len {
                                    result.push(items[curr as usize].clone());
                                }
                                curr += step_val;
                            }
                        }
                        Ok(PyObject::List(Rc::new(RefCell::new(result))))
                    }
                    (PyObject::String(s), PyObject::Int(i)) => {
                        let mut idx = i;
                        if idx < 0 {
                            idx += s.len() as i64;
                        }
                        s.chars()
                            .nth(idx as usize)
                            .map(|c| PyObject::String(c.to_string()))
                            .ok_or_else(|| anyhow!("String index out of range"))
                    }
                    (PyObject::String(s), PyObject::Slice { start, stop, step }) => {
                        let len = s.len() as i64;
                        let step_val = match step.as_deref() {
                            Some(PyObject::Int(n)) => *n,
                            Some(PyObject::None) | None => 1,
                            _ => return Err(anyhow!("slice step must be an integer")),
                        };
                        if step_val == 0 {
                            return Err(anyhow!("slice step cannot be zero"));
                        }

                        let mut start_val = match start.as_deref() {
                            Some(PyObject::Int(n)) => *n,
                            Some(PyObject::None) | None => {
                                if step_val > 0 {
                                    0
                                } else {
                                    len - 1
                                }
                            }
                            _ => return Err(anyhow!("slice start must be an integer")),
                        };

                        let mut stop_val = match stop.as_deref() {
                            Some(PyObject::Int(n)) => *n,
                            Some(PyObject::None) | None => {
                                if step_val > 0 {
                                    len
                                } else {
                                    -1
                                }
                            }
                            _ => return Err(anyhow!("slice stop must be an integer")),
                        };

                        if start_val < 0 {
                            start_val += len;
                        }
                        if stop_val < 0 && stop.is_some() {
                            stop_val += len;
                        }

                        let mut result = String::new();
                        let mut curr = start_val;
                        let chars: Vec<char> = s.chars().collect();
                        if step_val > 0 {
                            while curr < stop_val && curr < len {
                                if curr >= 0 {
                                    result.push(chars[curr as usize]);
                                }
                                curr += step_val;
                            }
                        } else {
                            while curr > stop_val && curr >= 0 {
                                if curr < len {
                                    result.push(chars[curr as usize]);
                                }
                                curr += step_val;
                            }
                        }
                        Ok(PyObject::String(result))
                    }
                    (PyObject::Dict(d), key) => {
                        let key_str = key.to_string();
                        d.borrow()
                            .get(&key_str)
                            .cloned()
                            .ok_or_else(|| anyhow!("Key '{}' not found in dictionary", key_str))
                    }
                    _ => Err(anyhow!("Object is not subscriptable")),
                }?;
                Ok(res)
            }
            Expr::Attribute(target, attr) => {
                let val = self.eval_expression(target, env.clone())?;
                match val {
                    PyObject::Instance {
                        ref class,
                        ref attributes,
                    } => {
                        if let Some(attr_val) = attributes.borrow().get(attr) {
                            return Ok(attr_val.clone());
                        }
                        if let Some(method) = self.find_method(&class.borrow(), attr) {
                            return Ok(method);
                        }
                        Err(anyhow!("Attribute '{}' not found on instance", attr))
                    }
                    PyObject::Class { .. } => {
                        if let Some(method) = self.find_method(&val, attr) {
                            return Ok(method);
                        }
                        Err(anyhow!("Attribute '{}' not found on class", attr))
                    }
                    PyObject::Module { ref name, ref env } => {
                        if let Some(v) = env.borrow().get(attr) {
                            return Ok(v.clone());
                        }
                        Err(anyhow!("Module '{}' has no attribute '{}'", name, attr))
                    }
                    _ => Err(anyhow!("Object has no attributes: {:?}", val)),
                }
            }
            Expr::ListComprehension {
                expression,
                target,
                iterable,
                condition,
            } => {
                let iter_val = self.eval_expression(iterable, env.clone())?;
                let items = match iter_val {
                    PyObject::List(l) => l.borrow().clone(),
                    _ => return Err(anyhow!("Object is not iterable")),
                };

                let mut results = Vec::new();
                for item in items {
                    let mut comp_env = Environment::with_parent(env.clone());
                    comp_env.define(target.clone(), item);
                    let rc_comp_env = Rc::new(RefCell::new(comp_env));

                    let mut should_add = true;
                    if let Some(cond) = condition {
                        let cond_val = self.eval_expression(cond, rc_comp_env.clone())?;
                        if !self.is_truthy(&cond_val) {
                            should_add = false;
                        }
                    }

                    if should_add {
                        results.push(self.eval_expression(expression, rc_comp_env)?);
                    }
                }
                Ok(PyObject::List(Rc::new(RefCell::new(results))))
            }
            Expr::Lambda { params, body } => Ok(PyObject::Function {
                name: "<lambda>".to_string(),
                params: params.clone(),
                body: vec![Stmt::Return(Some(*body.clone()))],
            }),
            Expr::Slice { start, stop, step } => {
                let s = start
                    .as_ref()
                    .map(|e| self.eval_expression(e, env.clone()))
                    .transpose()?;
                let p = stop
                    .as_ref()
                    .map(|e| self.eval_expression(e, env.clone()))
                    .transpose()?;
                let t = step
                    .as_ref()
                    .map(|e| self.eval_expression(e, env.clone()))
                    .transpose()?;
                Ok(PyObject::Slice {
                    start: s.map(Box::new),
                    stop: p.map(Box::new),
                    step: t.map(Box::new),
                })
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
                BinaryOp::Div => {
                    if r == 0 {
                        return Err(anyhow!("ZeroDivisionError: division by zero"));
                    }
                    Ok(PyObject::Int(l / r))
                }
                BinaryOp::Mod => {
                    if r == 0 {
                        return Err(anyhow!("ZeroDivisionError: modulo by zero"));
                    }
                    Ok(PyObject::Int(l % r))
                }
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

    #[allow(clippy::only_used_in_recursion)]
    fn find_method(&self, class: &PyObject, name: &str) -> Option<PyObject> {
        if let PyObject::Class { methods, bases, .. } = class {
            if let Some(method) = methods.get(name) {
                return Some(method.clone());
            }
            for base in bases {
                if let Some(method) = self.find_method(base, name) {
                    return Some(method);
                }
            }
        }
        None
    }

    #[allow(clippy::only_used_in_recursion)]
    fn is_subclass(&self, child: &PyObject, parent: &PyObject) -> bool {
        if child == parent {
            return true;
        }
        if let PyObject::Class { bases, .. } = child {
            for base in bases {
                if self.is_subclass(base, parent) {
                    return true;
                }
            }
        }
        false
    }
}
