use crate::ast::{BinaryOp, Expr, Literal, LogicalOp, Stmt, UnaryOp};
use crate::env::Environment;
use crate::object::PyObject;
use anyhow::{Result, anyhow};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub struct Evaluator {
    pub global_env: Rc<RefCell<Environment>>,
    load_paths: Rc<RefCell<Vec<String>>>,
}

impl crate::object::PyCallableContext for Evaluator {
    fn call_method(
        &mut self,
        obj: &PyObject,
        name: &str,
        args: Vec<PyObject>,
    ) -> anyhow::Result<PyObject> {
        if let Some(method) = self.get_method(obj, name) {
            return self.call_func(&method, args);
        }
        Err(anyhow!("Attribute '{}' not found on {:?}", name, obj))
    }

    fn call_func(&mut self, func: &PyObject, args: Vec<PyObject>) -> anyhow::Result<PyObject> {
        self.call_pyfunc(func, args)
    }

    fn is_subclass(&self, child: &PyObject, parent: &PyObject) -> bool {
        self.is_subclass(child, parent)
    }
}

impl Evaluator {
    pub fn new() -> Self {
        let global_env = Rc::new(RefCell::new(Environment::new()));
        let load_paths = Rc::new(RefCell::new(vec![".".to_string()]));

        // Register built-ins
        global_env.borrow_mut().define(
            "print".to_string(),
            PyObject::BuiltinFunction(Rc::new(|ctx, args| {
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        print!(" ");
                    }
                    let s = if let Ok(res) = ctx.call_method(arg, "__str__", vec![arg.clone()]) {
                        res.to_string()
                    } else {
                        arg.to_string()
                    };
                    print!("{}", s);
                }
                println!();
                Ok(PyObject::None)
            })),
        );

        global_env.borrow_mut().define(
            "len".to_string(),
            PyObject::BuiltinFunction(Rc::new(|ctx, args| {
                if args.len() != 1 {
                    return Ok(PyObject::None);
                }
                match &args[0] {
                    PyObject::String(s) => Ok(PyObject::Int(s.len() as i64)),
                    PyObject::List(l) => Ok(PyObject::Int(l.borrow().len() as i64)),
                    PyObject::Dict(d) => Ok(PyObject::Int(d.borrow().len() as i64)),
                    _ => {
                        if let Ok(res) = ctx.call_method(&args[0], "__len__", vec![args[0].clone()])
                        {
                            return Ok(res);
                        }
                        Ok(PyObject::None)
                    }
                }
            })),
        );

        global_env.borrow_mut().define(
            "range".to_string(),
            PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                let (start, stop, step) = match args.len() {
                    1 => (0, args[0].as_int().cloned().unwrap_or(0), 1),
                    2 => (
                        args[0].as_int().cloned().unwrap_or(0),
                        args[1].as_int().cloned().unwrap_or(0),
                        1,
                    ),
                    3 => (
                        args[0].as_int().cloned().unwrap_or(0),
                        args[1].as_int().cloned().unwrap_or(0),
                        args[2].as_int().cloned().unwrap_or(1),
                    ),
                    _ => (0, 0, 1),
                };
                Ok(PyObject::Iterator(Rc::new(RefCell::new(
                    crate::object::PyIterator::Range(start, stop, step),
                ))))
            })),
        );

        global_env.borrow_mut().define(
            "str".to_string(),
            PyObject::BuiltinFunction(Rc::new(|ctx, args| {
                if args.len() != 1 {
                    return Ok(PyObject::None);
                }
                if let Ok(res) = ctx.call_method(&args[0], "__str__", vec![args[0].clone()]) {
                    return Ok(res);
                }
                Ok(PyObject::String(args[0].to_string()))
            })),
        );

        global_env.borrow_mut().define(
            "type".to_string(),
            PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                if args.len() != 1 {
                    return Ok(PyObject::None);
                }
                Ok(match &args[0] {
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
                    PyObject::Iterator(_) => PyObject::String("iterator".to_string()),
                    PyObject::Tuple(_) => PyObject::String("tuple".to_string()),
                    PyObject::None => PyObject::String("NoneType".to_string()),
                })
            })),
        );

        global_env.borrow_mut().define(
            "isinstance".to_string(),
            PyObject::BuiltinFunction(Rc::new(|ctx, args| {
                if args.len() == 2 {
                    let obj = &args[0];
                    let target_type = &args[1];

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
                                ctx.is_subclass(&class.borrow(), target_type),
                            ));
                        }
                        _ => return Ok(PyObject::Bool(false)),
                    }
                }
                Ok(PyObject::Bool(false))
            })),
        );

        global_env.borrow_mut().define(
            "hasattr".to_string(),
            PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                if args.len() == 2 {
                    let obj = &args[0];
                    let attr_name = args[1].to_string();
                    match obj {
                        PyObject::Instance { attributes, .. } => {
                            return Ok(PyObject::Bool(
                                attributes.borrow().contains_key(&attr_name),
                            ));
                        }
                        _ => return Ok(PyObject::Bool(false)),
                    }
                }
                Ok(PyObject::Bool(false))
            })),
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
        let open_load_paths = load_paths.clone();
        global_env.borrow_mut().define(
            "open".to_string(),
            PyObject::BuiltinFunction(Rc::new(move |_ctx, args| {
                if args.is_empty() {
                    return Ok(PyObject::None);
                }
                let filename = args[0].to_string();
                let inner_load_paths = open_load_paths.clone();

                let mut attributes = HashMap::new();
                let f_name_read = filename.clone();

                attributes.insert(
                    "read".to_string(),
                    PyObject::BuiltinFunction(Rc::new(move |_ctx, _| {
                        for path in inner_load_paths.borrow().iter() {
                            let p = std::path::Path::new(path).join(&f_name_read);
                            if let Ok(content) = std::fs::read_to_string(p) {
                                return Ok(PyObject::String(content));
                            }
                        }
                        Ok(PyObject::None)
                    })),
                );

                let f_name_write = filename.clone();
                attributes.insert(
                    "write".to_string(),
                    PyObject::BuiltinFunction(Rc::new(move |_ctx, f_args| {
                        if let Some(PyObject::String(content)) = f_args.first() {
                            std::fs::write(&f_name_write, content).ok();
                        }
                        Ok(PyObject::None)
                    })),
                );

                Ok(PyObject::Instance {
                    class: Rc::new(RefCell::new(PyObject::Type("file".to_string()))),
                    attributes: Rc::new(RefCell::new(attributes)),
                })
            })),
        );

        // Core built-ins for iterators
        global_env.borrow_mut().define(
            "iter".to_string(),
            PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                if args.is_empty() {
                    return Ok(PyObject::None);
                }
                let obj = &args[0];
                Ok(match obj {
                    PyObject::List(l) => PyObject::Iterator(Rc::new(RefCell::new(
                        crate::object::PyIterator::List(l.clone(), 0),
                    ))),
                    PyObject::String(s) => PyObject::Iterator(Rc::new(RefCell::new(
                        crate::object::PyIterator::String(s.clone(), 0),
                    ))),
                    PyObject::Iterator(it) => PyObject::Iterator(it.clone()),
                    _ => {
                        // In a full implementation, we'd check for __iter__ method
                        PyObject::None
                    }
                })
            })),
        );

        global_env.borrow_mut().define(
            "next".to_string(),
            PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                if args.is_empty() {
                    return Ok(PyObject::None);
                }
                if let PyObject::Iterator(it) = &args[0] {
                    let mut it_borrow = it.borrow_mut();
                    Ok(it_borrow.next().unwrap_or(PyObject::None))
                } else {
                    Ok(PyObject::None)
                }
            })),
        );

        global_env.borrow_mut().define(
            "sum".to_string(),
            PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                if args.is_empty() {
                    return Ok(PyObject::Int(0));
                }
                let mut total = 0;
                if let Some(it) = args[0].to_iterator() {
                    let mut it_borrow = it.borrow_mut();
                    while let Some(val) = it_borrow.next() {
                        if let PyObject::Int(n) = val {
                            total += n;
                        }
                    }
                }
                Ok(PyObject::Int(total))
            })),
        );

        global_env.borrow_mut().define(
            "max".to_string(),
            PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                if args.is_empty() {
                    return Ok(PyObject::None);
                }
                let mut max_val: Option<i64> = None;
                if let Some(it) = args[0].to_iterator() {
                    let mut it_borrow = it.borrow_mut();
                    while let Some(val) = it_borrow.next() {
                        if let PyObject::Int(n) = val
                            && (max_val.is_none() || n > max_val.unwrap())
                        {
                            max_val = Some(n);
                        }
                    }
                }
                Ok(max_val.map(PyObject::Int).unwrap_or(PyObject::None))
            })),
        );

        global_env.borrow_mut().define(
            "min".to_string(),
            PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                if args.is_empty() {
                    return Ok(PyObject::None);
                }
                let mut min_val: Option<i64> = None;
                if let Some(it) = args[0].to_iterator() {
                    let mut it_borrow = it.borrow_mut();
                    while let Some(val) = it_borrow.next() {
                        if let PyObject::Int(n) = val
                            && (min_val.is_none() || n < min_val.unwrap())
                        {
                            min_val = Some(n);
                        }
                    }
                }
                Ok(min_val.map(PyObject::Int).unwrap_or(PyObject::None))
            })),
        );

        global_env.borrow_mut().define(
            "all".to_string(),
            PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                if args.is_empty() {
                    return Ok(PyObject::Bool(true));
                }
                if let Some(it) = args[0].to_iterator() {
                    let mut it_borrow = it.borrow_mut();
                    while let Some(val) = it_borrow.next() {
                        match val {
                            PyObject::Bool(b) if !b => return Ok(PyObject::Bool(false)),
                            PyObject::None => return Ok(PyObject::Bool(false)),
                            PyObject::Int(0) => return Ok(PyObject::Bool(false)),
                            _ => {}
                        }
                    }
                }
                Ok(PyObject::Bool(true))
            })),
        );

        global_env.borrow_mut().define(
            "any".to_string(),
            PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                if args.is_empty() {
                    return Ok(PyObject::Bool(false));
                }
                if let Some(it) = args[0].to_iterator() {
                    let mut it_borrow = it.borrow_mut();
                    while let Some(val) = it_borrow.next() {
                        match val {
                            PyObject::Bool(b) if b => return Ok(PyObject::Bool(true)),
                            PyObject::Int(n) if n != 0 => return Ok(PyObject::Bool(true)),
                            PyObject::String(s) if !s.is_empty() => {
                                return Ok(PyObject::Bool(true));
                            }
                            PyObject::List(l) if !l.borrow().is_empty() => {
                                return Ok(PyObject::Bool(true));
                            }
                            _ => {}
                        }
                    }
                }
                Ok(PyObject::Bool(false))
            })),
        );

        let mut evaluator = Self {
            global_env,
            load_paths,
        };

        // Inject high-level functions using PyRS code
        let lib = "
def map(f, it):
    return [f(x) for x in it]

def filter(f, it):
    return [x for x in it if f(x)]
";
        let lexer = crate::lexer::Lexer::new(lib);
        let mut parser = crate::parser::Parser::new(lexer);
        if let Ok(stmts) = parser.parse() {
            let _ = evaluator.eval_statements(&stmts, evaluator.global_env.clone());
        }

        evaluator
    }

    pub fn add_load_path(&self, path: String) {
        self.load_paths.borrow_mut().push(path);
    }

    fn call_pyfunc(&mut self, func: &PyObject, args: Vec<PyObject>) -> Result<PyObject> {
        match func {
            PyObject::BuiltinFunction(f) => f(self, args),
            PyObject::Function { params, body, .. } => {
                if params.len() != args.len() {
                    return Err(anyhow!(
                        "Expected {} arguments, got {}",
                        params.len(),
                        args.len()
                    ));
                }
                let call_env = Rc::new(RefCell::new(Environment::with_parent(
                    self.global_env.clone(),
                )));
                for (param, arg) in params.iter().zip(args) {
                    call_env.borrow_mut().define(param.clone(), arg);
                }
                self.eval_statements(body, call_env)
            }
            PyObject::Class { methods, .. } => {
                // Instantiation
                let instance = PyObject::Instance {
                    class: Rc::new(RefCell::new(func.clone())),
                    attributes: Rc::new(RefCell::new(HashMap::new())),
                };
                // Call __init__ if exists
                if let Some(PyObject::Function { params, body, .. }) = methods.get("__init__") {
                    let mut init_args = vec![instance.clone()];
                    init_args.extend(args);
                    if params.len() != init_args.len() {
                        return Err(anyhow!("__init__ expected {} args", params.len()));
                    }
                    let call_env = Rc::new(RefCell::new(Environment::with_parent(
                        self.global_env.clone(),
                    )));
                    for (param, arg) in params.iter().zip(init_args) {
                        call_env.borrow_mut().define(param.clone(), arg);
                    }
                    self.eval_statements(body, call_env)?;
                }
                Ok(instance)
            }
            _ => Err(anyhow!("Object is not callable: {:?}", func)),
        }
    }

    fn get_method(&self, obj: &PyObject, name: &str) -> Option<PyObject> {
        match obj {
            PyObject::Instance {
                class, attributes, ..
            } => {
                if let Some(val) = attributes.borrow().get(name) {
                    return Some(val.clone());
                }
                self.find_method(&class.borrow(), name)
            }
            PyObject::Class { .. } => self.find_method(obj, name),
            _ => None,
        }
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
                self.eval_assignment(target_expr, value, env)?;
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
                let it_rc = iter_val
                    .to_iterator()
                    .ok_or_else(|| anyhow!("Object is not iterable"))?;
                let mut it = it_rc.borrow_mut();

                while let Some(val) = it.next() {
                    self.eval_assignment(target, val, env.clone())?;
                    if let Some(res) = self.eval_block(body, env.clone())? {
                        return Ok(Some(res));
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
                let parts: Vec<&str> = name.split('.').collect();
                let rel_path = name.replace('.', "/");
                let filename = format!("{}.pyrs", rel_path);

                let mut content = None;
                for path in self.load_paths.borrow().iter() {
                    let p = std::path::Path::new(path).join(&filename);
                    if let Ok(c) = std::fs::read_to_string(p) {
                        content = Some(c);
                        break;
                    }
                }

                let content =
                    content.ok_or_else(|| anyhow!("Could not import module '{}'", name))?;

                let lexer = crate::lexer::Lexer::new(&content);
                let mut parser = crate::parser::Parser::new(lexer);
                let statements = parser.parse()?;

                let mut module_evaluator = Evaluator::new();
                *module_evaluator.load_paths.borrow_mut() = self.load_paths.borrow().clone();
                module_evaluator.eval(&statements)?;

                let module = PyObject::Module {
                    name: parts.last().unwrap().to_string(),
                    env: module_evaluator.global_env.clone(),
                };

                // Handle nested naming: import a.b -> root env gets 'a', 'a' gets 'b'
                let mut current_scope = env.clone();
                for (i, part) in parts.iter().enumerate() {
                    if i == parts.len() - 1 {
                        current_scope
                            .borrow_mut()
                            .define(part.to_string(), module.clone());
                    } else {
                        let sub_module = {
                            let maybe_sub = current_scope.borrow().get(part);
                            match maybe_sub {
                                Some(PyObject::Module { name, env }) => PyObject::Module {
                                    name: name.clone(),
                                    env: env.clone(),
                                },
                                _ => {
                                    let m = PyObject::Module {
                                        name: part.to_string(),
                                        env: Rc::new(RefCell::new(Environment::new())),
                                    };
                                    current_scope
                                        .borrow_mut()
                                        .define(part.to_string(), m.clone());
                                    m
                                }
                            }
                        };

                        if let PyObject::Module { env: sub_env, .. } = sub_module {
                            current_scope = sub_env;
                        }
                    }
                }
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

                match callee_val {
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
                    _ => {
                        let mut call_args = evaluated_args;
                        if let Expr::Attribute(target_expr, _) = &**callee {
                            let target_val = self.eval_expression(target_expr, env.clone())?;
                            if matches!(target_val, PyObject::Instance { .. }) {
                                call_args.insert(0, target_val);
                            }
                        }
                        self.call_pyfunc(&callee_val, call_args)
                    }
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
                let res = match (left.clone(), index.clone()) {
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
                    (PyObject::Tuple(items), PyObject::Int(i)) => {
                        let mut idx = i;
                        if idx < 0 {
                            idx += items.len() as i64;
                        }
                        items
                            .get(idx as usize)
                            .cloned()
                            .ok_or_else(|| anyhow!("Tuple index out of range"))
                    }
                    (PyObject::Tuple(items), PyObject::Slice { start, stop, step }) => {
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
                        Ok(PyObject::Tuple(result))
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
                    _ => {
                        if let Some(method) = self.get_method(&left, "__getitem__") {
                            self.call_pyfunc(&method, vec![left.clone(), index])
                        } else {
                            Err(anyhow!("Object is not subscriptable"))
                        }
                    }
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
                    PyObject::List(l) => match attr.as_str() {
                        "append" | "push" => {
                            let l_clone = l.clone();
                            Ok(PyObject::BuiltinFunction(Rc::new(move |_ctx, args| {
                                if !args.is_empty() {
                                    l_clone.borrow_mut().push(args[0].clone());
                                }
                                Ok(PyObject::None)
                            })))
                        }
                        _ => Err(anyhow!("List object has no attribute '{}'", attr)),
                    },
                    PyObject::Dict(d) => match attr.as_str() {
                        "get" => {
                            let d_clone = d.clone();
                            Ok(PyObject::BuiltinFunction(Rc::new(move |_ctx, args| {
                                if args.is_empty() {
                                    return Ok(PyObject::None);
                                }
                                let key = args[0].to_string();
                                let borrow = d_clone.borrow();
                                if let Some(val) = borrow.get(&key) {
                                    Ok(val.clone())
                                } else if args.len() > 1 {
                                    Ok(args[1].clone())
                                } else {
                                    Ok(PyObject::None)
                                }
                            })))
                        }
                        _ => Err(anyhow!("Dict object has no attribute '{}'", attr)),
                    },
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
                let it_rc = iter_val
                    .to_iterator()
                    .ok_or_else(|| anyhow!("Object is not iterable"))?;
                let mut it = it_rc.borrow_mut();

                let mut results = Vec::new();
                while let Some(item) = it.next() {
                    let comp_env = Environment::with_parent(env.clone());
                    let rc_comp_env = Rc::new(RefCell::new(comp_env));
                    self.eval_assignment(target, item, rc_comp_env.clone())?;

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
            Expr::Tuple(exprs) => {
                let mut vals = Vec::new();
                for e in exprs {
                    vals.push(self.eval_expression(e, env.clone())?);
                }
                Ok(PyObject::Tuple(vals))
            }
            Expr::FString(parts) => {
                let mut result = String::new();
                for part in parts {
                    match part {
                        crate::ast::FStringPart::Literal(s) => result.push_str(s),
                        crate::ast::FStringPart::Expression(e) => {
                            let val = self.eval_expression(e, env.clone())?;
                            result.push_str(&val.to_string());
                        }
                    }
                }
                Ok(PyObject::String(result))
            }
        }
    }

    fn eval_binary_op(
        &mut self,
        left: PyObject,
        op: &BinaryOp,
        right: PyObject,
    ) -> Result<PyObject> {
        let dunder = match op {
            BinaryOp::Add => Some("__add__"),
            BinaryOp::Sub => Some("__sub__"),
            BinaryOp::Mul => Some("__mul__"),
            BinaryOp::Div => Some("__truediv__"),
            BinaryOp::Mod => Some("__mod__"),
            BinaryOp::Equal => Some("__eq__"),
            BinaryOp::NotEqual => Some("__ne__"),
            BinaryOp::Less => Some("__lt__"),
            BinaryOp::Greater => Some("__gt__"),
            BinaryOp::LessEqual => Some("__le__"),
            BinaryOp::GreaterEqual => Some("__ge__"),
            BinaryOp::In => Some("__contains__"),
        };

        if let Some(name) = dunder {
            let target = if op == &BinaryOp::In { &right } else { &left };
            if let Some(method) = self.get_method(target, name) {
                let args = if op == &BinaryOp::In {
                    vec![right.clone(), left.clone()]
                } else {
                    vec![left.clone(), right.clone()]
                };
                return self.call_pyfunc(&method, args);
            }
        }

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

    fn eval_assignment(
        &mut self,
        target: &Expr,
        value: PyObject,
        env: Rc<RefCell<Environment>>,
    ) -> Result<()> {
        match target {
            Expr::Variable(name) => {
                env.borrow_mut().define(name.clone(), value);
                Ok(())
            }
            Expr::Tuple(targets) | Expr::List(targets) => {
                let it_rc = value
                    .to_iterator()
                    .ok_or_else(|| anyhow!("cannot unpack non-iterable object"))?;
                let mut it = it_rc.borrow_mut();
                for target_expr in targets {
                    let val = it
                        .next()
                        .ok_or_else(|| anyhow!("not enough values to unpack"))?;
                    self.eval_assignment(target_expr, val, env.clone())?;
                }
                if it.next().is_some() {
                    return Err(anyhow!("too many values to unpack"));
                }
                Ok(())
            }
            Expr::Subscript(target, index_expr) => {
                let target_val = self.eval_expression(target, env.clone())?;
                let index_val = self.eval_expression(index_expr, env.clone())?;
                match target_val {
                    PyObject::List(l) => {
                        let idx = index_val
                            .as_int()
                            .ok_or_else(|| anyhow!("List index must be an integer"))?;
                        let mut items = l.borrow_mut();
                        if (*idx as usize) < items.len() {
                            items[*idx as usize] = value;
                        } else {
                            return Err(anyhow!("list index out of range"));
                        }
                    }
                    PyObject::Dict(d) => {
                        let key = index_val.to_string();
                        d.borrow_mut().insert(key, value);
                    }
                    _ => return Err(anyhow!("Object does not support item assignment")),
                }
                Ok(())
            }
            Expr::Attribute(target, attr) => {
                let target_val = self.eval_expression(target, env.clone())?;
                if let PyObject::Instance { attributes, .. } = target_val {
                    attributes.borrow_mut().insert(attr.clone(), value);
                } else {
                    return Err(anyhow!("Object has no attributes"));
                }
                Ok(())
            }
            _ => Err(anyhow!("Invalid assignment target")),
        }
    }
}
