use crate::ast::{BinaryOp, Expr, Literal, LogicalOp, Stmt, UnaryOp};
use crate::env::Environment;
use crate::object::{ExecutionFrame, GeneratorState, PyObject, PySocket};
use anyhow::{Result, anyhow};
use rand::RngExt;
use regex::Regex;
use serde_json::Value as JsonValue;
use socket2::{Domain, Protocol, Socket, Type};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::net::{SocketAddr, ToSocketAddrs};
use std::rc::Rc;

pub struct Evaluator {
    pub global_env: Rc<RefCell<Environment>>,
    load_paths: Rc<RefCell<Vec<String>>>,
    builtin_modules: HashMap<String, PyObject>,
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

    fn is_truthy(&self, obj: &PyObject) -> bool {
        self.is_truthy(obj)
    }

    fn resume_generator(
        &mut self,
        state: &Rc<RefCell<GeneratorState>>,
    ) -> anyhow::Result<Option<PyObject>> {
        self.resume_generator(state)
    }

    fn eval_binary_op(
        &mut self,
        left: PyObject,
        op: &crate::ast::BinaryOp,
        right: PyObject,
    ) -> anyhow::Result<PyObject> {
        self.eval_binary_op(left, op, right)
    }
}

impl Evaluator {
    pub fn new() -> Self {
        let global_env = Rc::new(RefCell::new(Environment::new()));
        let load_paths = Rc::new(RefCell::new(vec![".".to_string()]));
        let mut evaluator = Self {
            global_env,
            load_paths,
            builtin_modules: HashMap::new(),
        };

        evaluator.init_builtin_modules();
        evaluator.init_builtins();
        evaluator
    }

    fn init_builtins(&mut self) {
        let global_env = self.global_env.clone();
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
                    PyObject::Set(s) => Ok(PyObject::Int(s.borrow().len() as i64)),
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
                    1 => (0, args[0].as_i64().unwrap_or(0), 1),
                    2 => (
                        args[0].as_int().cloned().unwrap_or(0),
                        args[1].as_int().cloned().unwrap_or(0),
                        1,
                    ),
                    3 => (
                        args[0].as_i64().unwrap_or(0),
                        args[1].as_i64().unwrap_or(0),
                        args[2].as_i64().unwrap_or(1),
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
                    PyObject::Float(_) => PyObject::String("float".to_string()),
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
                    PyObject::Generator(_) => PyObject::String("generator".to_string()),
                    PyObject::Tuple(_) => PyObject::String("tuple".to_string()),
                    PyObject::Set(_) => PyObject::String("set".to_string()),
                    PyObject::Socket(_) => PyObject::String("socket".to_string()),
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
                        (PyObject::Float(_), PyObject::Type(s)) if s == "float" => {
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
                        (PyObject::Generator(_), PyObject::Type(s)) if s == "generator" => {
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
            .define("float".to_string(), PyObject::Type("float".to_string()));
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
            "Exception".to_string(),
            PyObject::Type("Exception".to_string()),
        );
        global_env.borrow_mut().define(
            "ValueError".to_string(),
            PyObject::Type("ValueError".to_string()),
        );
        global_env.borrow_mut().define(
            "TypeError".to_string(),
            PyObject::Type("TypeError".to_string()),
        );
        global_env.borrow_mut().define(
            "IndexError".to_string(),
            PyObject::Type("IndexError".to_string()),
        );
        global_env.borrow_mut().define(
            "KeyError".to_string(),
            PyObject::Type("KeyError".to_string()),
        );
        global_env
            .borrow_mut()
            .define("OSError".to_string(), PyObject::Type("OSError".to_string()));
        global_env.borrow_mut().define(
            "set".to_string(),
            PyObject::BuiltinFunction(Rc::new(|ctx, args| {
                #[allow(clippy::mutable_key_type)]
                if args.is_empty() {
                    return Ok(PyObject::Set(Rc::new(RefCell::new(HashSet::new()))));
                }
                let it_rc = args[0]
                    .to_iterator(ctx)
                    .ok_or_else(|| anyhow!("TypeError: '{}' object is not iterable", args[0]))?;
                let mut it = it_rc.borrow_mut();
                #[allow(clippy::mutable_key_type)]
                let mut set = HashSet::new();
                while let Some(item) = it.next(ctx)? {
                    if !item.is_hashable() {
                        return Err(anyhow!("TypeError: unhashable type: '{:?}'", item));
                    }
                    set.insert(item);
                }
                Ok(PyObject::Set(Rc::new(RefCell::new(set))))
            })),
        );

        global_env.borrow_mut().define(
            "NoneType".to_string(),
            PyObject::Type("NoneType".to_string()),
        );

        global_env.borrow_mut().define(
            "StopIteration".to_string(),
            PyObject::Type("StopIteration".to_string()),
        );

        global_env.borrow_mut().define(
            "iter".to_string(),
            PyObject::BuiltinFunction(Rc::new(|ctx, args| {
                if args.len() != 1 {
                    return Err(anyhow!("TypeError: iter() expects 1 argument"));
                }
                let it = args[0]
                    .to_iterator(ctx)
                    .ok_or_else(|| anyhow!("TypeError: '{}' object is not iterable", args[0]))?;
                Ok(PyObject::Iterator(it))
            })),
        );

        global_env.borrow_mut().define(
            "next".to_string(),
            PyObject::BuiltinFunction(Rc::new(|ctx, args| {
                if args.is_empty() || args.len() > 2 {
                    return Err(anyhow!("TypeError: next() expects 1 or 2 arguments"));
                }
                let it_rc = if let PyObject::Iterator(it) = &args[0] {
                    it.clone()
                } else if let PyObject::Generator(state) = &args[0] {
                    Rc::new(RefCell::new(crate::object::PyIterator::Generator(
                        state.clone(),
                    )))
                } else {
                    return Err(anyhow!(
                        "TypeError: '{}' object is not an iterator",
                        args[0]
                    ));
                };

                match it_rc.borrow_mut().next(ctx)? {
                    Some(val) => Ok(val),
                    None => {
                        if args.len() == 2 {
                            Ok(args[1].clone())
                        } else {
                            Err(anyhow!("StopIteration"))
                        }
                    }
                }
            })),
        );

        global_env.borrow_mut().define(
            "any".to_string(),
            PyObject::BuiltinFunction(Rc::new(|ctx, args| {
                if args.len() != 1 {
                    return Err(anyhow!("TypeError: any() expects 1 argument"));
                }
                let it_rc = args[0]
                    .to_iterator(ctx)
                    .ok_or_else(|| anyhow!("TypeError: '{}' object is not iterable", args[0]))?;
                let mut it = it_rc.borrow_mut();
                // We need is_truthy here, but ctx is PyCallableContext.
                // Evaluator implements PyCallableContext.
                // But BuiltinFunction closure doesn't have access to Evaluator methods directly
                // unless we cast or add them to the trait.
                // For now, let's use a simple check or we might need to add is_truthy to trait.
                while let Some(item) = it.next(ctx)? {
                    if ctx.is_truthy(&item) {
                        return Ok(PyObject::Bool(true));
                    }
                }
                Ok(PyObject::Bool(false))
            })),
        );

        global_env.borrow_mut().define(
            "all".to_string(),
            PyObject::BuiltinFunction(Rc::new(|ctx, args| {
                if args.len() != 1 {
                    return Err(anyhow!("TypeError: all() expects 1 argument"));
                }
                let it_rc = args[0]
                    .to_iterator(ctx)
                    .ok_or_else(|| anyhow!("TypeError: '{}' object is not iterable", args[0]))?;
                let mut it = it_rc.borrow_mut();
                while let Some(item) = it.next(ctx)? {
                    if !ctx.is_truthy(&item) {
                        return Ok(PyObject::Bool(false));
                    }
                }
                Ok(PyObject::Bool(true))
            })),
        );

        global_env.borrow_mut().define(
            "enumerate".to_string(),
            PyObject::BuiltinFunction(Rc::new(|ctx, args| {
                if args.is_empty() || args.len() > 2 {
                    return Err(anyhow!("TypeError: enumerate() expects 1 or 2 arguments"));
                }
                let it = args[0]
                    .to_iterator(ctx)
                    .ok_or_else(|| anyhow!("TypeError: '{}' object is not iterable", args[0]))?;
                let start = if args.len() == 2 {
                    args[1].as_int().cloned().unwrap_or(0)
                } else {
                    0
                };
                Ok(PyObject::Iterator(Rc::new(RefCell::new(
                    crate::object::PyIterator::Enumerate(it, start),
                ))))
            })),
        );

        global_env.borrow_mut().define(
            "zip".to_string(),
            PyObject::BuiltinFunction(Rc::new(|ctx, args| {
                let mut iterators = Vec::new();
                for arg in args {
                    let it = arg
                        .to_iterator(ctx)
                        .ok_or_else(|| anyhow!("TypeError: '{}' object is not iterable", arg))?;
                    iterators.push(it);
                }
                Ok(PyObject::Iterator(Rc::new(RefCell::new(
                    crate::object::PyIterator::Zip(iterators),
                ))))
            })),
        );

        global_env.borrow_mut().define(
            "map".to_string(),
            PyObject::BuiltinFunction(Rc::new(|ctx, args| {
                if args.len() < 2 {
                    return Err(anyhow!("TypeError: map() expects at least 2 arguments"));
                }
                let func = args[0].clone();
                let mut iterators = Vec::new();
                for arg in &args[1..] {
                    let it = arg
                        .to_iterator(ctx)
                        .ok_or_else(|| anyhow!("TypeError: '{}' object is not iterable", arg))?;
                    iterators.push(it);
                }
                Ok(PyObject::Iterator(Rc::new(RefCell::new(
                    crate::object::PyIterator::Map(func, iterators),
                ))))
            })),
        );

        global_env.borrow_mut().define(
            "filter".to_string(),
            PyObject::BuiltinFunction(Rc::new(|ctx, args| {
                if args.len() != 2 {
                    return Err(anyhow!("TypeError: filter() expects 2 arguments"));
                }
                let func = args[0].clone();
                let it = args[1]
                    .to_iterator(ctx)
                    .ok_or_else(|| anyhow!("TypeError: '{}' object is not iterable", args[1]))?;
                Ok(PyObject::Iterator(Rc::new(RefCell::new(
                    crate::object::PyIterator::Filter(func, it),
                ))))
            })),
        );

        // File I/O
        let open_load_paths = self.load_paths.clone();
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

        global_env.borrow_mut().define(
            "sum".to_string(),
            PyObject::BuiltinFunction(Rc::new(|ctx, args| {
                if args.is_empty() {
                    return Ok(PyObject::Int(0));
                }
                let mut total = PyObject::Int(0);
                if let Some(it) = args[0].to_iterator(ctx) {
                    let mut it_borrow = it.borrow_mut();
                    while let Some(val) = it_borrow.next(ctx)? {
                        total = ctx.eval_binary_op(total, &BinaryOp::Add, val)?;
                    }
                }
                Ok(total)
            })),
        );

        global_env.borrow_mut().define(
            "max".to_string(),
            PyObject::BuiltinFunction(Rc::new(|ctx, args| {
                if args.is_empty() {
                    return Ok(PyObject::None);
                }
                let mut max_val: Option<PyObject> = None;
                if let Some(it) = args[0].to_iterator(ctx) {
                    let mut it_borrow = it.borrow_mut();
                    while let Some(val) = it_borrow.next(ctx)? {
                        match &max_val {
                            None => max_val = Some(val),
                            Some(current_max) => {
                                let cmp = ctx.eval_binary_op(
                                    val.clone(),
                                    &BinaryOp::Greater,
                                    current_max.clone(),
                                )?;
                                if ctx.is_truthy(&cmp) {
                                    max_val = Some(val);
                                }
                            }
                        }
                    }
                }
                Ok(max_val.unwrap_or(PyObject::None))
            })),
        );

        global_env.borrow_mut().define(
            "min".to_string(),
            PyObject::BuiltinFunction(Rc::new(|ctx, args| {
                if args.is_empty() {
                    return Ok(PyObject::None);
                }
                let mut min_val: Option<PyObject> = None;
                if let Some(it) = args[0].to_iterator(ctx) {
                    let mut it_borrow = it.borrow_mut();
                    while let Some(val) = it_borrow.next(ctx)? {
                        match &min_val {
                            None => min_val = Some(val),
                            Some(current_min) => {
                                let cmp = ctx.eval_binary_op(
                                    val.clone(),
                                    &BinaryOp::Less,
                                    current_min.clone(),
                                )?;
                                if ctx.is_truthy(&cmp) {
                                    min_val = Some(val);
                                }
                            }
                        }
                    }
                }
                Ok(min_val.unwrap_or(PyObject::None))
            })),
        );
    }

    fn init_builtin_modules(&mut self) {
        // --- sys module ---
        let sys_module = self.create_module(
            "sys",
            vec![
                (
                    "argv",
                    PyObject::List(Rc::new(RefCell::new(
                        std::env::args().map(PyObject::String).collect(),
                    ))),
                ),
                (
                    "path",
                    PyObject::List(Rc::new(RefCell::new(
                        self.load_paths
                            .borrow()
                            .iter()
                            .map(|p| PyObject::String(p.clone()))
                            .collect(),
                    ))),
                ),
                ("version", PyObject::String("PyRS 0.1.0".to_string())),
                (
                    "exit",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                        let code = if let Some(arg) = args.first() {
                            match arg {
                                PyObject::Int(n) => *n as i32,
                                _ => 0,
                            }
                        } else {
                            0
                        };
                        std::process::exit(code);
                    })),
                ),
            ],
        );
        self.builtin_modules.insert("sys".to_string(), sys_module);

        // --- os module ---
        let mut os_attrs = vec![
            (
                "getcwd",
                PyObject::BuiltinFunction(Rc::new(|_ctx, _args| {
                    Ok(PyObject::String(
                        std::env::current_dir()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_default(),
                    ))
                })),
            ),
            (
                "listdir",
                PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                    let path = if let Some(PyObject::String(p)) = args.first() {
                        p.clone()
                    } else {
                        ".".to_string()
                    };
                    let mut entries = Vec::new();
                    if let Ok(read_dir) = std::fs::read_dir(path) {
                        for entry in read_dir.flatten() {
                            entries.push(PyObject::String(
                                entry.file_name().to_string_lossy().to_string(),
                            ));
                        }
                    }
                    Ok(PyObject::List(Rc::new(RefCell::new(entries))))
                })),
            ),
            (
                "mkdir",
                PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                    let path = args
                        .first()
                        .ok_or_else(|| {
                            anyhow!("TypeError: mkdir() missing 1 required positional argument")
                        })?
                        .to_string();
                    std::fs::create_dir(path)?;
                    Ok(PyObject::None)
                })),
            ),
            (
                "remove",
                PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                    let path = args
                        .first()
                        .ok_or_else(|| {
                            anyhow!("TypeError: remove() missing 1 required positional argument")
                        })?
                        .to_string();
                    std::fs::remove_file(path)?;
                    Ok(PyObject::None)
                })),
            ),
            (
                "rename",
                PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                    if args.len() < 2 {
                        return Err(anyhow!(
                            "TypeError: rename() missing 2 required positional arguments"
                        ));
                    }
                    std::fs::rename(args[0].to_string(), args[1].to_string())?;
                    Ok(PyObject::None)
                })),
            ),
        ];

        let mut env_vars = HashMap::new();
        for (k, v) in std::env::vars() {
            env_vars.insert(k, PyObject::String(v));
        }
        os_attrs.push(("environ", PyObject::Dict(Rc::new(RefCell::new(env_vars)))));

        // os.path submodule
        let os_path = self.create_module(
            "os.path",
            vec![
                (
                    "exists",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                        let path = args.first().map(|a| a.to_string()).unwrap_or_default();
                        Ok(PyObject::Bool(std::path::Path::new(&path).exists()))
                    })),
                ),
                (
                    "isdir",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                        let path = args.first().map(|a| a.to_string()).unwrap_or_default();
                        Ok(PyObject::Bool(std::path::Path::new(&path).is_dir()))
                    })),
                ),
                (
                    "isfile",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                        let path = args.first().map(|a| a.to_string()).unwrap_or_default();
                        Ok(PyObject::Bool(std::path::Path::new(&path).is_file()))
                    })),
                ),
                (
                    "join",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                        let mut path = std::path::PathBuf::new();
                        for arg in args {
                            path.push(arg.to_string());
                        }
                        Ok(PyObject::String(path.to_string_lossy().to_string()))
                    })),
                ),
                (
                    "split",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                        let path_str = args.first().map(|a| a.to_string()).unwrap_or_default();
                        let path = std::path::Path::new(&path_str);
                        let head = path
                            .parent()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_default();
                        let tail = path
                            .file_name()
                            .map(|f| f.to_string_lossy().to_string())
                            .unwrap_or_default();
                        Ok(PyObject::Tuple(vec![
                            PyObject::String(head),
                            PyObject::String(tail),
                        ]))
                    })),
                ),
            ],
        );
        os_attrs.push(("path", os_path));

        let os_module = self.create_module("os", os_attrs);
        self.builtin_modules.insert("os".to_string(), os_module);

        // --- math module ---
        let math_module = self.create_module(
            "math",
            vec![
                ("pi", PyObject::Float(std::f64::consts::PI)),
                ("e", PyObject::Float(std::f64::consts::E)),
                (
                    "sqrt",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                        let n = args
                            .first()
                            .ok_or_else(|| {
                                anyhow!("TypeError: sqrt() missing 1 required positional argument")
                            })?
                            .as_f64()
                            .ok_or_else(|| anyhow!("TypeError: a float is required"))?;
                        Ok(PyObject::Float(n.sqrt()))
                    })),
                ),
                (
                    "sin",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                        let n = args
                            .first()
                            .ok_or_else(|| {
                                anyhow!("TypeError: sin() missing 1 required positional argument")
                            })?
                            .as_f64()
                            .ok_or_else(|| anyhow!("TypeError: a float is required"))?;
                        Ok(PyObject::Float(n.sin()))
                    })),
                ),
                (
                    "cos",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                        let n = args
                            .first()
                            .ok_or_else(|| {
                                anyhow!("TypeError: cos() missing 1 required positional argument")
                            })?
                            .as_f64()
                            .ok_or_else(|| anyhow!("TypeError: a float is required"))?;
                        Ok(PyObject::Float(n.cos()))
                    })),
                ),
                (
                    "tan",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                        let n = args
                            .first()
                            .ok_or_else(|| {
                                anyhow!("TypeError: tan() missing 1 required positional argument")
                            })?
                            .as_f64()
                            .ok_or_else(|| anyhow!("TypeError: a float is required"))?;
                        Ok(PyObject::Float(n.tan()))
                    })),
                ),
                (
                    "asin",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                        let n = args
                            .first()
                            .ok_or_else(|| {
                                anyhow!("TypeError: asin() missing 1 required positional argument")
                            })?
                            .as_f64()
                            .ok_or_else(|| anyhow!("TypeError: a float is required"))?;
                        Ok(PyObject::Float(n.asin()))
                    })),
                ),
                (
                    "acos",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                        let n = args
                            .first()
                            .ok_or_else(|| {
                                anyhow!("TypeError: acos() missing 1 required positional argument")
                            })?
                            .as_f64()
                            .ok_or_else(|| anyhow!("TypeError: a float is required"))?;
                        Ok(PyObject::Float(n.acos()))
                    })),
                ),
                (
                    "atan",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                        let n = args
                            .first()
                            .ok_or_else(|| {
                                anyhow!("TypeError: atan() missing 1 required positional argument")
                            })?
                            .as_f64()
                            .ok_or_else(|| anyhow!("TypeError: a float is required"))?;
                        Ok(PyObject::Float(n.atan()))
                    })),
                ),
                (
                    "exp",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                        let n = args
                            .first()
                            .ok_or_else(|| {
                                anyhow!("TypeError: exp() missing 1 required positional argument")
                            })?
                            .as_f64()
                            .ok_or_else(|| anyhow!("TypeError: a float is required"))?;
                        Ok(PyObject::Float(n.exp()))
                    })),
                ),
                (
                    "log",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                        let n = args
                            .first()
                            .ok_or_else(|| {
                                anyhow!("TypeError: log() missing 1 required positional argument")
                            })?
                            .as_f64()
                            .ok_or_else(|| anyhow!("TypeError: a float is required"))?;
                        if args.len() > 1 {
                            let base = args[1]
                                .as_f64()
                                .ok_or_else(|| anyhow!("TypeError: a float is required"))?;
                            Ok(PyObject::Float(n.log(base)))
                        } else {
                            Ok(PyObject::Float(n.ln()))
                        }
                    })),
                ),
                (
                    "log10",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                        let n = args
                            .first()
                            .ok_or_else(|| {
                                anyhow!("TypeError: log10() missing 1 required positional argument")
                            })?
                            .as_f64()
                            .ok_or_else(|| anyhow!("TypeError: a float is required"))?;
                        Ok(PyObject::Float(n.log10()))
                    })),
                ),
                (
                    "floor",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                        let n = args
                            .first()
                            .ok_or_else(|| {
                                anyhow!("TypeError: floor() missing 1 required positional argument")
                            })?
                            .as_f64()
                            .ok_or_else(|| anyhow!("TypeError: a float is required"))?;
                        Ok(PyObject::Int(n.floor() as i64))
                    })),
                ),
                (
                    "ceil",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                        let n = args
                            .first()
                            .ok_or_else(|| {
                                anyhow!("TypeError: ceil() missing 1 required positional argument")
                            })?
                            .as_f64()
                            .ok_or_else(|| anyhow!("TypeError: a float is required"))?;
                        Ok(PyObject::Int(n.ceil() as i64))
                    })),
                ),
                (
                    "gcd",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                        if args.len() < 2 {
                            return Err(anyhow!(
                                "TypeError: gcd() missing 2 required positional arguments"
                            ));
                        }
                        let mut a = args[0]
                            .as_i64()
                            .ok_or_else(|| anyhow!("TypeError: an integer is required"))?
                            .abs();
                        let mut b = args[1]
                            .as_i64()
                            .ok_or_else(|| anyhow!("TypeError: an integer is required"))?
                            .abs();
                        while b != 0 {
                            a %= b;
                            std::mem::swap(&mut a, &mut b);
                        }
                        Ok(PyObject::Int(a))
                    })),
                ),
                (
                    "factorial",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                        let n = args
                            .first()
                            .ok_or_else(|| {
                                anyhow!(
                                    "TypeError: factorial() missing 1 required positional argument"
                                )
                            })?
                            .as_i64()
                            .ok_or_else(|| anyhow!("TypeError: an integer is required"))?;
                        if n < 0 {
                            return Err(anyhow!(
                                "ValueError: factorial() not defined for negative values"
                            ));
                        }
                        let mut res: i64 = 1;
                        for i in 1..=n {
                            res = res.checked_mul(i).ok_or_else(|| {
                                anyhow!("OverflowError: factorial result too large")
                            })?;
                        }
                        Ok(PyObject::Int(res))
                    })),
                ),
            ],
        );
        self.builtin_modules.insert("math".to_string(), math_module);

        // --- random module ---
        let random_module = self.create_module(
            "random",
            vec![
                (
                    "random",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, _args| {
                        let mut rng = rand::rng();
                        Ok(PyObject::Float(rng.random::<f64>()))
                    })),
                ),
                (
                    "uniform",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                        if args.len() < 2 {
                            return Err(anyhow!(
                                "TypeError: uniform() missing 2 required positional arguments"
                            ));
                        }
                        let a = args[0]
                            .as_f64()
                            .ok_or_else(|| anyhow!("TypeError: a float is required"))?;
                        let b = args[1]
                            .as_f64()
                            .ok_or_else(|| anyhow!("TypeError: a float is required"))?;
                        let mut rng = rand::rng();
                        Ok(PyObject::Float(rng.random_range(a..=b)))
                    })),
                ),
                (
                    "randint",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                        if args.len() < 2 {
                            return Err(anyhow!(
                                "TypeError: randint() missing 2 required positional arguments"
                            ));
                        }
                        let a = args[0]
                            .as_i64()
                            .ok_or_else(|| anyhow!("TypeError: an integer is required"))?;
                        let b = args[1]
                            .as_i64()
                            .ok_or_else(|| anyhow!("TypeError: an integer is required"))?;
                        let mut rng = rand::rng();
                        Ok(PyObject::Int(rng.random_range(a..=b)))
                    })),
                ),
                (
                    "choice",
                    PyObject::BuiltinFunction(Rc::new(|ctx, args| {
                        let seq = args.first().ok_or_else(|| {
                            anyhow!("TypeError: choice() missing 1 required positional argument")
                        })?;
                        if let Some(it) = seq.to_iterator(ctx) {
                            let mut items = Vec::new();
                            let mut it_borrow = it.borrow_mut();
                            while let Some(val) = it_borrow.next(ctx)? {
                                items.push(val);
                            }
                            if items.is_empty() {
                                return Err(anyhow!(
                                    "IndexError: Cannot choose from an empty sequence"
                                ));
                            }
                            let mut rng = rand::rng();
                            let idx = rng.random_range(0..items.len());
                            Ok(items[idx].clone())
                        } else {
                            Err(anyhow!("TypeError: object is not iterable"))
                        }
                    })),
                ),
            ],
        );
        self.builtin_modules
            .insert("random".to_string(), random_module);

        // --- time module ---
        let time_module = self.create_module(
            "time",
            vec![
                (
                    "time",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, _args| {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default();
                        Ok(PyObject::Float(now.as_secs_f64()))
                    })),
                ),
                (
                    "sleep",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                        let secs = args.first().and_then(|a| a.as_f64()).unwrap_or(0.0);
                        std::thread::sleep(std::time::Duration::from_secs_f64(secs));
                        Ok(PyObject::None)
                    })),
                ),
            ],
        );
        self.builtin_modules.insert("time".to_string(), time_module);

        // --- json module ---
        let json_module = self.create_module(
            "json",
            vec![
                (
                    "loads",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                        let s = args
                            .first()
                            .ok_or_else(|| {
                                anyhow!("TypeError: loads() missing 1 required positional argument")
                            })?
                            .to_string();
                        let val: JsonValue =
                            serde_json::from_str(&s).map_err(|e| anyhow!("ValueError: {}", e))?;
                        Ok(json_to_py(val))
                    })),
                ),
                (
                    "dumps",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                        let obj = args.first().ok_or_else(|| {
                            anyhow!("TypeError: dumps() missing 1 required positional argument")
                        })?;
                        let val = py_to_json(obj)?;
                        let s = serde_json::to_string(&val)
                            .map_err(|e| anyhow!("ValueError: {}", e))?;
                        Ok(PyObject::String(s))
                    })),
                ),
            ],
        );
        self.builtin_modules.insert("json".to_string(), json_module);

        // --- re module ---
        let re_module = self.create_module(
            "re",
            vec![
                (
                    "search",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                        if args.len() < 2 {
                            return Err(anyhow!(
                                "TypeError: search() missing 2 required positional arguments"
                            ));
                        }
                        let pattern = args[0].to_string();
                        let text = args[1].to_string();
                        let re = Regex::new(&pattern).map_err(|e| anyhow!("ValueError: {}", e))?;
                        if let Some(caps) = re.captures(&text) {
                            let mut res = HashMap::new();
                            let mut groups = Vec::new();
                            for i in 0..caps.len() {
                                groups.push(PyObject::String(
                                    caps.get(i)
                                        .map(|m| m.as_str().to_string())
                                        .unwrap_or_default(),
                                ));
                            }
                            res.insert(
                                "groups".to_string(),
                                PyObject::List(Rc::new(RefCell::new(groups))),
                            );
                            Ok(PyObject::Dict(Rc::new(RefCell::new(res))))
                        } else {
                            Ok(PyObject::None)
                        }
                    })),
                ),
                (
                    "match",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                        if args.len() < 2 {
                            return Err(anyhow!(
                                "TypeError: match() missing 2 required positional arguments"
                            ));
                        }
                        let pattern = args[0].to_string();
                        let text = args[1].to_string();
                        let re = Regex::new(&format!("^{}", pattern))
                            .map_err(|e| anyhow!("ValueError: {}", e))?;
                        if let Some(caps) = re.captures(&text) {
                            let mut res = HashMap::new();
                            let mut groups = Vec::new();
                            for i in 0..caps.len() {
                                groups.push(PyObject::String(
                                    caps.get(i)
                                        .map(|m| m.as_str().to_string())
                                        .unwrap_or_default(),
                                ));
                            }
                            res.insert(
                                "groups".to_string(),
                                PyObject::List(Rc::new(RefCell::new(groups))),
                            );
                            Ok(PyObject::Dict(Rc::new(RefCell::new(res))))
                        } else {
                            Ok(PyObject::None)
                        }
                    })),
                ),
                (
                    "findall",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                        if args.len() < 2 {
                            return Err(anyhow!(
                                "TypeError: findall() missing 2 required positional arguments"
                            ));
                        }
                        let pattern = args[0].to_string();
                        let text = args[1].to_string();
                        let re = Regex::new(&pattern).map_err(|e| anyhow!("ValueError: {}", e))?;
                        let matches: Vec<PyObject> = re
                            .find_iter(&text)
                            .map(|m| PyObject::String(m.as_str().to_string()))
                            .collect();
                        Ok(PyObject::List(Rc::new(RefCell::new(matches))))
                    })),
                ),
                (
                    "sub",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                        if args.len() < 3 {
                            return Err(anyhow!(
                                "TypeError: sub() missing 3 required positional arguments"
                            ));
                        }
                        let pattern = args[0].to_string();
                        let repl = args[1].to_string();
                        let text = args[2].to_string();
                        let re = Regex::new(&pattern).map_err(|e| anyhow!("ValueError: {}", e))?;
                        let res = re.replace_all(&text, repl.as_str()).to_string();
                        Ok(PyObject::String(res))
                    })),
                ),
            ],
        );
        self.builtin_modules.insert("re".to_string(), re_module);

        // --- socket module ---
        let socket_module = self.create_module(
            "socket",
            vec![
                ("AF_INET", PyObject::Int(2)),
                ("SOCK_STREAM", PyObject::Int(1)),
                ("SOCK_DGRAM", PyObject::Int(2)),
                ("SOL_SOCKET", PyObject::Int(1)),
                ("SO_REUSEADDR", PyObject::Int(2)),
                ("error", PyObject::Type("OSError".to_string())),
                (
                    "socket",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                        let family = args.get(0).and_then(|a| a.as_int()).cloned().unwrap_or(2);
                        let type_ = args.get(1).and_then(|a| a.as_int()).cloned().unwrap_or(1);
                        let proto = args.get(2).and_then(|a| a.as_int()).cloned().unwrap_or(0);

                        let domain = Domain::from(family as i32);
                        let rtype = Type::from(type_ as i32);
                        let protocol = Some(Protocol::from(proto as i32));

                        let socket = Socket::new(domain, rtype, protocol)
                            .map_err(|e| anyhow!("OSError: {}", e))?;

                        Ok(PyObject::Socket(Rc::new(RefCell::new(PySocket {
                            inner: socket,
                            family: family as i32,
                            type_: type_ as i32,
                        }))))
                    })),
                ),
                (
                    "gethostname",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, _args| {
                        let name = hostname::get().map_err(|e| anyhow!("OSError: {}", e))?;
                        Ok(PyObject::String(name.to_string_lossy().into_owned()))
                    })),
                ),
                (
                    "gethostbyname",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                        let name = args
                            .first()
                            .ok_or_else(|| anyhow!("TypeError: gethostbyname() takes 1 argument"))?
                            .to_string();
                        let addr_str = format!("{}:0", name);
                        let mut addrs = addr_str
                            .to_socket_addrs()
                            .map_err(|e| anyhow!("OSError: {}", e))?;
                        if let Some(addr) = addrs.next() {
                            Ok(PyObject::String(addr.ip().to_string()))
                        } else {
                            Err(anyhow!("OSError: host not found"))
                        }
                    })),
                ),
                (
                    "inet_aton",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                        let ip = args
                            .first()
                            .ok_or_else(|| anyhow!("TypeError: inet_aton() takes 1 argument"))?
                            .to_string();
                        let addr: std::net::Ipv4Addr =
                            ip.parse().map_err(|e| anyhow!("OSError: {}", e))?;
                        Ok(PyObject::String(
                            String::from_utf8_lossy(&addr.octets()).to_string(),
                        ))
                    })),
                ),
                (
                    "inet_ntoa",
                    PyObject::BuiltinFunction(Rc::new(|_ctx, args| {
                        let data = args
                            .first()
                            .ok_or_else(|| anyhow!("TypeError: inet_ntoa() takes 1 argument"))?
                            .to_string();
                        let bytes = data.as_bytes();
                        if bytes.len() != 4 {
                            return Err(anyhow!("OSError: packed IP wrong length"));
                        }
                        let addr = std::net::Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3]);
                        Ok(PyObject::String(addr.to_string()))
                    })),
                ),
            ],
        );
        self.builtin_modules
            .insert("socket".to_string(), socket_module);
    }

    fn create_module(&self, name: &str, attrs: Vec<(&str, PyObject)>) -> PyObject {
        let env = Rc::new(RefCell::new(Environment::new()));
        for (k, v) in attrs {
            env.borrow_mut().define(k.to_string(), v);
        }
        PyObject::Module {
            name: name.to_string(),
            env,
        }
    }

    pub fn resume_generator(
        &mut self,
        state_rc: &Rc<RefCell<GeneratorState>>,
    ) -> Result<Option<PyObject>> {
        let mut s = state_rc.borrow_mut();
        if s.is_finished {
            return Ok(None);
        }

        loop {
            let frame = match s.stack.pop() {
                Some(f) => f,
                None => {
                    s.is_finished = true;
                    return Ok(None);
                }
            };

            match frame {
                ExecutionFrame::Block {
                    stmts,
                    mut idx,
                    env,
                } => {
                    if idx >= stmts.len() {
                        continue;
                    }
                    let stmt = stmts[idx].clone();
                    idx += 1;

                    match stmt {
                        Stmt::Expression(Expr::Yield(inner)) => {
                            let val = if let Some(e) = inner {
                                self.eval_expression(&e, env.clone())?
                            } else {
                                PyObject::None
                            };
                            s.stack.push(ExecutionFrame::Block { stmts, idx, env });
                            return Ok(Some(val));
                        }
                        Stmt::Expression(expr) => {
                            if let Expr::Yield(inner) = expr {
                                let val = if let Some(e) = inner {
                                    self.eval_expression(&e, env.clone())?
                                } else {
                                    PyObject::None
                                };
                                s.stack.push(ExecutionFrame::Block { stmts, idx, env });
                                return Ok(Some(val));
                            }
                            self.eval_expression(&expr, env.clone())?;
                            s.stack.push(ExecutionFrame::Block { stmts, idx, env });
                        }
                        Stmt::Assignment(target, value_expr) => {
                            let value = self.eval_expression(&value_expr, env.clone())?;
                            self.eval_assignment(&target, value, env.clone())?;
                            s.stack.push(ExecutionFrame::Block { stmts, idx, env });
                        }
                        Stmt::If {
                            condition,
                            then_branch,
                            else_branch,
                        } => {
                            let cond_val = self.eval_expression(&condition, env.clone())?;
                            s.stack.push(ExecutionFrame::Block {
                                stmts: stmts.clone(),
                                idx,
                                env: env.clone(),
                            });
                            if self.is_truthy(&cond_val) {
                                s.stack.push(ExecutionFrame::Block {
                                    stmts: then_branch,
                                    idx: 0,
                                    env: env.clone(),
                                });
                            } else if let Some(else_b) = else_branch {
                                s.stack.push(ExecutionFrame::Block {
                                    stmts: else_b,
                                    idx: 0,
                                    env: env.clone(),
                                });
                            }
                        }
                        Stmt::While { condition, body } => {
                            s.stack.push(ExecutionFrame::Block {
                                stmts,
                                idx,
                                env: env.clone(),
                            });
                            s.stack.push(ExecutionFrame::While {
                                condition,
                                body,
                                env,
                                checking_condition: true,
                            });
                        }
                        Stmt::For {
                            target,
                            iterable,
                            body,
                        } => {
                            let iter_val = self.eval_expression(&iterable, env.clone())?;
                            let it_rc = iter_val
                                .to_iterator(self)
                                .ok_or_else(|| anyhow!("Not iterable"))?;
                            s.stack.push(ExecutionFrame::Block {
                                stmts,
                                idx,
                                env: env.clone(),
                            });
                            s.stack.push(ExecutionFrame::For {
                                target,
                                iterator: it_rc,
                                body,
                                env,
                            });
                        }
                        Stmt::Return(_) => {
                            s.is_finished = true;
                            return Ok(None);
                        }
                        Stmt::FunctionDef { .. } | Stmt::ClassDef { .. } => {
                            self.eval_statement(&stmt, env.clone())?;
                            s.stack.push(ExecutionFrame::Block { stmts, idx, env });
                        }
                        _ => {
                            self.eval_statement(&stmt, env.clone())?;
                            s.stack.push(ExecutionFrame::Block { stmts, idx, env });
                        }
                    }
                }
                ExecutionFrame::While {
                    condition,
                    body,
                    env,
                    mut checking_condition,
                } => {
                    if checking_condition {
                        let cond_val = self.eval_expression(&condition, env.clone())?;
                        if self.is_truthy(&cond_val) {
                            checking_condition = false;
                            s.stack.push(ExecutionFrame::While {
                                condition,
                                body: body.clone(),
                                env: env.clone(),
                                checking_condition,
                            });
                            s.stack.push(ExecutionFrame::Block {
                                stmts: body,
                                idx: 0,
                                env,
                            });
                        }
                        // if false, frame is not pushed back
                    } else {
                        checking_condition = true;
                        s.stack.push(ExecutionFrame::While {
                            condition,
                            body,
                            env,
                            checking_condition,
                        });
                    }
                }
                ExecutionFrame::For {
                    target,
                    iterator,
                    body,
                    env,
                } => {
                    let next_val = iterator.borrow_mut().next(self)?;
                    if let Some(val) = next_val {
                        self.eval_assignment(&target, val, env.clone())?;
                        s.stack.push(ExecutionFrame::For {
                            target,
                            iterator,
                            body: body.clone(),
                            env: env.clone(),
                        });
                        s.stack.push(ExecutionFrame::Block {
                            stmts: body,
                            idx: 0,
                            env,
                        });
                    }
                }
            }
        }
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

                if body.iter().any(|s| s.has_yield()) {
                    let state = GeneratorState {
                        stack: vec![ExecutionFrame::Block {
                            stmts: body.clone(),
                            idx: 0,
                            env: call_env,
                        }],
                        is_finished: false,
                    };
                    Ok(PyObject::Generator(Rc::new(RefCell::new(state))))
                } else {
                    self.eval_statements(body, call_env)
                }
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
                    .to_iterator(self)
                    .ok_or_else(|| anyhow!("Object is not iterable"))?;
                while let Some(val) = it_rc.borrow_mut().next(self)? {
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

                if let Some(builtin) = self.builtin_modules.get(name) {
                    env.borrow_mut().define(name.clone(), builtin.clone());
                    return Ok(None);
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
            Stmt::Raise(expr) => {
                let val = self.eval_expression(expr, env)?;
                Err(anyhow!("{}", val))
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
                Literal::Float(f) => Ok(PyObject::Float(*f)),
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
                let left_val = self.eval_expression(left, env.clone())?;
                match op {
                    LogicalOp::And => {
                        if !self.is_truthy(&left_val) {
                            Ok(left_val)
                        } else {
                            self.eval_expression(right, env)
                        }
                    }
                    LogicalOp::Or => {
                        if self.is_truthy(&left_val) {
                            Ok(left_val)
                        } else {
                            self.eval_expression(right, env)
                        }
                    }
                }
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
                        if evaluated_args.is_empty() {
                            return match name.as_str() {
                                "str" => Ok(PyObject::String("".to_string())),
                                "int" => Ok(PyObject::Int(0)),
                                "bool" => Ok(PyObject::Bool(false)),
                                "list" => Ok(PyObject::List(Rc::new(RefCell::new(Vec::new())))),
                                "tuple" => Ok(PyObject::Tuple(Vec::new())),
                                "dict" => Ok(PyObject::Dict(Rc::new(RefCell::new(HashMap::new())))),
                                _ => Err(anyhow!("Constructor not implemented for type {}", name)),
                            };
                        }
                        if evaluated_args.len() != 1 {
                            return Err(anyhow!("Type constructor expected 0 or 1 argument"));
                        }
                        match name.as_str() {
                            "str" => Ok(PyObject::String(evaluated_args[0].to_string())),
                            "int" => match &evaluated_args[0] {
                                PyObject::Int(n) => Ok(PyObject::Int(*n)),
                                PyObject::String(s) => Ok(PyObject::Int(s.parse().unwrap_or(0))),
                                _ => Err(anyhow!("Could not convert to int")),
                            },
                            "bool" => Ok(PyObject::Bool(self.is_truthy(&evaluated_args[0]))),
                            "list" => {
                                if let Some(it) = evaluated_args[0].to_iterator(self) {
                                    let mut items = Vec::new();
                                    while let Some(val) = it.borrow_mut().next(self)? {
                                        items.push(val);
                                    }
                                    Ok(PyObject::List(Rc::new(RefCell::new(items))))
                                } else {
                                    Err(anyhow!(
                                        "TypeError: '{}' is not iterable",
                                        evaluated_args[0]
                                    ))
                                }
                            }
                            "tuple" => {
                                if let Some(it) = evaluated_args[0].to_iterator(self) {
                                    let mut items = Vec::new();
                                    while let Some(val) = it.borrow_mut().next(self)? {
                                        items.push(val);
                                    }
                                    Ok(PyObject::Tuple(items))
                                } else {
                                    Err(anyhow!(
                                        "TypeError: '{}' is not iterable",
                                        evaluated_args[0]
                                    ))
                                }
                            }
                            "dict" => {
                                if let Some(it) = evaluated_args[0].to_iterator(self) {
                                    let mut items = HashMap::new();
                                    while let Some(item) = it.borrow_mut().next(self)? {
                                        if let PyObject::Tuple(ref kv) = item
                                            && kv.len() == 2
                                        {
                                            items.insert(kv[0].to_string(), kv[1].clone());
                                            continue;
                                        }
                                        return Err(anyhow!(
                                            "TypeError: dictionary update sequence element has length {}; 2 is required",
                                            match item {
                                                PyObject::Tuple(ref v) => v.len(),
                                                _ => 0,
                                            }
                                        ));
                                    }
                                    Ok(PyObject::Dict(Rc::new(RefCell::new(items))))
                                } else {
                                    Err(anyhow!(
                                        "TypeError: '{}' is not iterable",
                                        evaluated_args[0]
                                    ))
                                }
                            }
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
            Expr::Set(items) => {
                #[allow(clippy::mutable_key_type)]
                let mut evaluated_items = HashSet::new();
                for item_expr in items {
                    let item = self.eval_expression(item_expr, env.clone())?;
                    if !item.is_hashable() {
                        return Err(anyhow!("TypeError: unhashable type"));
                    }
                    evaluated_items.insert(item);
                }
                Ok(PyObject::Set(Rc::new(RefCell::new(evaluated_items))))
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
                    PyObject::Socket(s) => match attr.as_str() {
                        "bind" => {
                            let s_clone = s.clone();
                            Ok(PyObject::BuiltinFunction(Rc::new(move |_ctx, args| {
                                let addr_obj = args
                                    .first()
                                    .ok_or_else(|| anyhow!("TypeError: bind() takes 1 argument"))?;
                                let addr = resolve_addr(addr_obj)?;
                                s_clone
                                    .borrow()
                                    .inner
                                    .bind(&addr.into())
                                    .map_err(|e| anyhow!("OSError: {}", e))?;
                                Ok(PyObject::None)
                            })))
                        }
                        "listen" => {
                            let s_clone = s.clone();
                            Ok(PyObject::BuiltinFunction(Rc::new(move |_ctx, args| {
                                let backlog = args
                                    .first()
                                    .and_then(|a| a.as_int())
                                    .cloned()
                                    .unwrap_or(128);
                                s_clone
                                    .borrow()
                                    .inner
                                    .listen(backlog as i32)
                                    .map_err(|e| anyhow!("OSError: {}", e))?;
                                Ok(PyObject::None)
                            })))
                        }
                        "accept" => {
                            let s_clone = s.clone();
                            Ok(PyObject::BuiltinFunction(Rc::new(move |_ctx, _args| {
                                let (client, addr) = s_clone
                                    .borrow()
                                    .inner
                                    .accept()
                                    .map_err(|e| anyhow!("OSError: {}", e))?;
                                let family = s_clone.borrow().family;
                                let type_ = s_clone.borrow().type_;
                                let py_client = PyObject::Socket(Rc::new(RefCell::new(PySocket {
                                    inner: client,
                                    family,
                                    type_,
                                })));
                                let addr_tuple = match addr.as_socket() {
                                    Some(addr) => PyObject::Tuple(vec![
                                        PyObject::String(addr.ip().to_string()),
                                        PyObject::Int(addr.port() as i64),
                                    ]),
                                    None => PyObject::None,
                                };
                                Ok(PyObject::Tuple(vec![py_client, addr_tuple]))
                            })))
                        }
                        "connect" => {
                            let s_clone = s.clone();
                            Ok(PyObject::BuiltinFunction(Rc::new(move |_ctx, args| {
                                let addr_obj = args.first().ok_or_else(|| {
                                    anyhow!("TypeError: connect() takes 1 argument")
                                })?;
                                let addr = resolve_addr(addr_obj)?;
                                s_clone
                                    .borrow()
                                    .inner
                                    .connect(&addr.into())
                                    .map_err(|e| anyhow!("OSError: {}", e))?;
                                Ok(PyObject::None)
                            })))
                        }
                        "send" => {
                            let s_clone = s.clone();
                            Ok(PyObject::BuiltinFunction(Rc::new(move |_ctx, args| {
                                let data = args
                                    .first()
                                    .ok_or_else(|| anyhow!("TypeError: send() takes 1 argument"))?;
                                let bytes = data.to_string().into_bytes();
                                let sent = s_clone
                                    .borrow()
                                    .inner
                                    .send(&bytes)
                                    .map_err(|e| anyhow!("OSError: {}", e))?;
                                Ok(PyObject::Int(sent as i64))
                            })))
                        }
                        "recv" => {
                            let s_clone = s.clone();
                            Ok(PyObject::BuiltinFunction(Rc::new(move |_ctx, args| {
                                let bufsize = args
                                    .first()
                                    .and_then(|a| a.as_int())
                                    .cloned()
                                    .unwrap_or(4096);
                                let mut buf = vec![0u8; bufsize as usize];
                                // Prepare buffer for socket2 recv
                                let n = unsafe {
                                    let uninit_buf = std::slice::from_raw_parts_mut(
                                        buf.as_mut_ptr() as *mut std::mem::MaybeUninit<u8>,
                                        buf.len(),
                                    );
                                    s_clone.borrow().inner.recv(uninit_buf)
                                }
                                .map_err(|e| anyhow!("OSError: {}", e))?;
                                Ok(PyObject::String(
                                    String::from_utf8_lossy(&buf[..n]).to_string(),
                                ))
                            })))
                        }
                        "close" => {
                            let s_clone = s.clone();
                            Ok(PyObject::BuiltinFunction(Rc::new(move |_ctx, _args| {
                                // Socket2 close is dropping the object or shutdown
                                // Standard Python close() shuts down and closes.
                                // We'll just ignore for now as dropping often does it,
                                // but we can call shutdown.
                                let _ = s_clone.borrow().inner.shutdown(std::net::Shutdown::Both);
                                Ok(PyObject::None)
                            })))
                        }
                        "setsockopt" => {
                            let s_clone = s.clone();
                            Ok(PyObject::BuiltinFunction(Rc::new(move |_ctx, args| {
                                if args.len() < 3 {
                                    return Err(anyhow!(
                                        "TypeError: setsockopt() takes 3 arguments"
                                    ));
                                }
                                let level = args[0].as_int().cloned().unwrap_or(0) as i32;
                                let optname = args[1].as_int().cloned().unwrap_or(0) as i32;
                                // In socket2 0.5, we can use set_sockopt for raw options
                                // but often we need specific types. For simplicity, we'll try to use set_sockopt
                                // if it exists, or just support SO_REUSEADDR specifically for now.
                                if level == 1 && optname == 2 {
                                    // SOL_SOCKET, SO_REUSEADDR
                                    let val = args[2].as_int().cloned().unwrap_or(0) != 0;
                                    s_clone
                                        .borrow()
                                        .inner
                                        .set_reuse_address(val)
                                        .map_err(|e| anyhow!("OSError: {}", e))?;
                                }
                                Ok(PyObject::None)
                            })))
                        }
                        "setblocking" => {
                            let s_clone = s.clone();
                            Ok(PyObject::BuiltinFunction(Rc::new(move |_ctx, args| {
                                let blocking =
                                    args.first().map(|a| _ctx.is_truthy(a)).unwrap_or(true);
                                s_clone
                                    .borrow()
                                    .inner
                                    .set_nonblocking(!blocking)
                                    .map_err(|e| anyhow!("OSError: {}", e))?;
                                Ok(PyObject::None)
                            })))
                        }
                        _ => Err(anyhow!("Socket object has no attribute '{}'", attr)),
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
                    .to_iterator(self)
                    .ok_or_else(|| anyhow!("Object is not iterable"))?;
                let mut it = it_rc.borrow_mut();

                let mut results = Vec::new();
                while let Some(item) = it.next(self)? {
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
            Expr::Yield(_) => Err(anyhow!(
                "RuntimeError: yield expression not supported in this context"
            )),
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
            BinaryOp::BitwiseAnd => Some("__and__"),
            BinaryOp::BitwiseOr => Some("__or__"),
            BinaryOp::BitwiseXor => Some("__xor__"),
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
                PyObject::Set(s) => {
                    if !left.is_hashable() {
                        return Err(anyhow!("TypeError: unhashable type"));
                    }
                    Ok(PyObject::Bool(s.borrow().contains(&left)))
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
                    Ok(PyObject::Float(l as f64 / r as f64))
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
                BinaryOp::BitwiseAnd => Ok(PyObject::Int(l & r)),
                BinaryOp::BitwiseOr => Ok(PyObject::Int(l | r)),
                BinaryOp::BitwiseXor => Ok(PyObject::Int(l ^ r)),
                BinaryOp::In => unreachable!(),
            },
            (PyObject::Float(l), PyObject::Float(r)) => match op {
                BinaryOp::Add => Ok(PyObject::Float(l + r)),
                BinaryOp::Sub => Ok(PyObject::Float(l - r)),
                BinaryOp::Mul => Ok(PyObject::Float(l * r)),
                BinaryOp::Div => {
                    if r == 0.0 {
                        return Err(anyhow!("ZeroDivisionError: float division by zero"));
                    }
                    Ok(PyObject::Float(l / r))
                }
                BinaryOp::Mod => Ok(PyObject::Float(l % r)),
                BinaryOp::Equal => Ok(PyObject::Bool(l == r)),
                BinaryOp::NotEqual => Ok(PyObject::Bool(l != r)),
                BinaryOp::Less => Ok(PyObject::Bool(l < r)),
                BinaryOp::Greater => Ok(PyObject::Bool(l > r)),
                BinaryOp::LessEqual => Ok(PyObject::Bool(l <= r)),
                BinaryOp::GreaterEqual => Ok(PyObject::Bool(l >= r)),
                _ => Err(anyhow!("TypeError: unsupported operand type(s) for float")),
            },
            (PyObject::Int(l), PyObject::Float(r)) => {
                let l_f = l as f64;
                match op {
                    BinaryOp::Add => Ok(PyObject::Float(l_f + r)),
                    BinaryOp::Sub => Ok(PyObject::Float(l_f - r)),
                    BinaryOp::Mul => Ok(PyObject::Float(l_f * r)),
                    BinaryOp::Div => {
                        if r == 0.0 {
                            return Err(anyhow!("ZeroDivisionError: division by zero"));
                        }
                        Ok(PyObject::Float(l_f / r))
                    }
                    BinaryOp::Mod => Ok(PyObject::Float(l_f % r)),
                    BinaryOp::Equal => Ok(PyObject::Bool(l_f == r)),
                    BinaryOp::NotEqual => Ok(PyObject::Bool(l_f != r)),
                    BinaryOp::Less => Ok(PyObject::Bool(l_f < r)),
                    BinaryOp::Greater => Ok(PyObject::Bool(l_f > r)),
                    BinaryOp::LessEqual => Ok(PyObject::Bool(l_f <= r)),
                    BinaryOp::GreaterEqual => Ok(PyObject::Bool(l_f >= r)),
                    _ => Err(anyhow!("TypeError: unsupported operand type(s)")),
                }
            }
            (PyObject::Float(l), PyObject::Int(r)) => {
                let r_f = r as f64;
                match op {
                    BinaryOp::Add => Ok(PyObject::Float(l + r_f)),
                    BinaryOp::Sub => Ok(PyObject::Float(l - r_f)),
                    BinaryOp::Mul => Ok(PyObject::Float(l * r_f)),
                    BinaryOp::Div => {
                        if r == 0 {
                            return Err(anyhow!("ZeroDivisionError: division by zero"));
                        }
                        Ok(PyObject::Float(l / r_f))
                    }
                    BinaryOp::Mod => Ok(PyObject::Float(l % r_f)),
                    BinaryOp::Equal => Ok(PyObject::Bool(l == r_f)),
                    BinaryOp::NotEqual => Ok(PyObject::Bool(l != r_f)),
                    BinaryOp::Less => Ok(PyObject::Bool(l < r_f)),
                    BinaryOp::Greater => Ok(PyObject::Bool(l > r_f)),
                    BinaryOp::LessEqual => Ok(PyObject::Bool(l <= r_f)),
                    BinaryOp::GreaterEqual => Ok(PyObject::Bool(l >= r_f)),
                    _ => Err(anyhow!("TypeError: unsupported operand type(s)")),
                }
            }
            (PyObject::String(l), PyObject::String(r)) => match op {
                BinaryOp::Add => Ok(PyObject::String(format!("{}{}", l, r))),
                BinaryOp::Equal => Ok(PyObject::Bool(l == r)),
                BinaryOp::NotEqual => Ok(PyObject::Bool(l != r)),
                BinaryOp::In => unreachable!(),
                _ => Err(anyhow!("Invalid operator for strings")),
            },
            (PyObject::Set(l), PyObject::Set(r)) => {
                let s1 = l.borrow();
                let s2 = r.borrow();
                match op {
                    #[allow(clippy::mutable_key_type)]
                    BinaryOp::BitwiseOr => {
                        let res: HashSet<_> = s1.union(&s2).cloned().collect();
                        Ok(PyObject::Set(Rc::new(RefCell::new(res))))
                    }
                    #[allow(clippy::mutable_key_type)]
                    BinaryOp::BitwiseAnd => {
                        let res: HashSet<_> = s1.intersection(&s2).cloned().collect();
                        Ok(PyObject::Set(Rc::new(RefCell::new(res))))
                    }
                    #[allow(clippy::mutable_key_type)]
                    BinaryOp::Sub => {
                        let res: HashSet<_> = s1.difference(&s2).cloned().collect();
                        Ok(PyObject::Set(Rc::new(RefCell::new(res))))
                    }
                    #[allow(clippy::mutable_key_type)]
                    BinaryOp::BitwiseXor => {
                        let res: HashSet<_> = s1.symmetric_difference(&s2).cloned().collect();
                        Ok(PyObject::Set(Rc::new(RefCell::new(res))))
                    }
                    BinaryOp::Equal => Ok(PyObject::Bool(*s1 == *s2)),
                    BinaryOp::NotEqual => Ok(PyObject::Bool(*s1 != *s2)),
                    _ => Err(anyhow!("Unsupported operator for sets")),
                }
            }
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
            PyObject::Float(n) => *n != 0.0,
            PyObject::String(s) => !s.is_empty(),
            PyObject::List(l) => !l.borrow().is_empty(),
            PyObject::Dict(d) => !d.borrow().is_empty(),
            PyObject::Set(s) => !s.borrow().is_empty(),
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
                    .to_iterator(self)
                    .ok_or_else(|| anyhow!("cannot unpack non-iterable object"))?;
                let mut it = it_rc.borrow_mut();
                for target_expr in targets {
                    let val = it
                        .next(self)?
                        .ok_or_else(|| anyhow!("not enough values to unpack"))?;
                    self.eval_assignment(target_expr, val, env.clone())?;
                }
                if it.next(self)?.is_some() {
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

fn resolve_addr(addr: &PyObject) -> Result<SocketAddr> {
    if let PyObject::Tuple(items) = addr {
        if items.len() >= 2 {
            let host = items[0].to_string();
            let port = items[1]
                .as_int()
                .ok_or_else(|| anyhow!("TypeError: Port must be an integer"))?;
            let addr_str = format!("{}:{}", host, port);
            let mut addrs = addr_str
                .to_socket_addrs()
                .map_err(|e| anyhow!("OSError: {}", e))?;
            return addrs
                .next()
                .ok_or_else(|| anyhow!("OSError: Could not resolve address"));
        }
    }
    Err(anyhow!("TypeError: address must be a tuple (host, port)"))
}

fn py_to_json(obj: &PyObject) -> Result<JsonValue> {
    match obj {
        PyObject::Int(n) => Ok(JsonValue::Number((*n).into())),
        PyObject::Float(f) => {
            let n = serde_json::Number::from_f64(*f)
                .ok_or_else(|| anyhow!("ValueError: Invalid float for JSON"))?;
            Ok(JsonValue::Number(n))
        }
        PyObject::String(s) => Ok(JsonValue::String(s.clone())),
        PyObject::Bool(b) => Ok(JsonValue::Bool(*b)),
        PyObject::None => Ok(JsonValue::Null),
        PyObject::List(l) => {
            let mut arr = Vec::new();
            for item in l.borrow().iter() {
                arr.push(py_to_json(item)?);
            }
            Ok(JsonValue::Array(arr))
        }
        PyObject::Tuple(t) => {
            let mut arr = Vec::new();
            for item in t.iter() {
                arr.push(py_to_json(item)?);
            }
            Ok(JsonValue::Array(arr))
        }
        PyObject::Dict(d) => {
            let mut map = serde_json::Map::new();
            for (k, v) in d.borrow().iter() {
                map.insert(k.clone(), py_to_json(v)?);
            }
            Ok(JsonValue::Object(map))
        }
        _ => Err(anyhow!(
            "TypeError: Object of type {} is not JSON serializable",
            type_name(obj)
        )),
    }
}

fn json_to_py(val: JsonValue) -> PyObject {
    match val {
        JsonValue::Null => PyObject::None,
        JsonValue::Bool(b) => PyObject::Bool(b),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                PyObject::Int(i)
            } else if let Some(f) = n.as_f64() {
                PyObject::Float(f)
            } else {
                PyObject::None
            }
        }
        JsonValue::String(s) => PyObject::String(s),
        JsonValue::Array(arr) => {
            let items: Vec<PyObject> = arr.into_iter().map(json_to_py).collect();
            PyObject::List(Rc::new(RefCell::new(items)))
        }
        JsonValue::Object(obj) => {
            let mut map = HashMap::new();
            for (k, v) in obj {
                map.insert(k, json_to_py(v));
            }
            PyObject::Dict(Rc::new(RefCell::new(map)))
        }
    }
}

fn type_name(obj: &PyObject) -> &'static str {
    match obj {
        PyObject::Int(_) => "int",
        PyObject::Float(_) => "float",
        PyObject::String(_) => "str",
        PyObject::Bool(_) => "bool",
        PyObject::List(_) => "list",
        PyObject::Tuple(_) => "tuple",
        PyObject::Dict(_) => "dict",
        PyObject::Set(_) => "set",
        PyObject::Function { .. } => "function",
        PyObject::BuiltinFunction(_) => "builtin_function",
        PyObject::Class { .. } => "type",
        PyObject::Type(_) => "type",
        PyObject::Module { .. } => "module",
        PyObject::Slice { .. } => "slice",
        PyObject::Instance { .. } => "instance",
        PyObject::Iterator(_) => "iterator",
        PyObject::Generator(_) => "generator",
        PyObject::Socket(_) => "socket",
        PyObject::None => "NoneType",
    }
}
