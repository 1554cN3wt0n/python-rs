use crate::ast::{BinaryOp, Expr, Literal, Stmt};
use crate::env::Environment;
use crate::object::PyObject;
use anyhow::{Result, anyhow};
use std::cell::RefCell;
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
            PyObject::BuiltinFunction(|args| {
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        print!(" ");
                    }
                    print!("{}", arg);
                }
                println!();
                PyObject::None
            }),
        );

        global_env.borrow_mut().define(
            "len".to_string(),
            PyObject::BuiltinFunction(|args| {
                if args.len() != 1 {
                    return PyObject::None; // Should ideally be an error
                }
                match &args[0] {
                    PyObject::String(s) => PyObject::Int(s.len() as i64),
                    _ => PyObject::None,
                }
            }),
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
            Stmt::Assignment(name, expr) => {
                let value = self.eval_expression(expr, env.clone())?;
                env.borrow_mut().define(name.clone(), value);
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
            Stmt::FunctionDef { name, params, body } => {
                let func = PyObject::Function {
                    name: name.clone(),
                    params: params.clone(),
                    body: body.clone(),
                };
                env.borrow_mut().define(name.clone(), func);
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
            Expr::Call(name, args) => {
                let func = env
                    .borrow()
                    .get(name)
                    .ok_or_else(|| anyhow!("Undefined function: {}", name))?;
                let mut evaluated_args = Vec::new();
                for arg in args {
                    evaluated_args.push(self.eval_expression(arg, env.clone())?);
                }

                match func {
                    PyObject::BuiltinFunction(f) => Ok(f(evaluated_args)),
                    PyObject::Function { params, body, .. } => {
                        if params.len() != evaluated_args.len() {
                            return Err(anyhow!(
                                "Expected {} arguments, got {}",
                                params.len(),
                                evaluated_args.len()
                            ));
                        }
                        let call_env = Rc::new(RefCell::new(Environment::with_parent(
                            self.global_env.clone(),
                        )));
                        for (param, arg) in params.iter().zip(evaluated_args) {
                            call_env.borrow_mut().define(param.clone(), arg);
                        }
                        self.eval_statements(&body, call_env)
                    }
                    _ => Err(anyhow!("'{}' is not callable", name)),
                }
            }
        }
    }

    fn eval_binary_op(&self, left: PyObject, op: &BinaryOp, right: PyObject) -> Result<PyObject> {
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
            },
            (PyObject::String(l), PyObject::String(r)) => match op {
                BinaryOp::Add => Ok(PyObject::String(format!("{}{}", l, r))),
                BinaryOp::Equal => Ok(PyObject::Bool(l == r)),
                BinaryOp::NotEqual => Ok(PyObject::Bool(l != r)),
                _ => Err(anyhow!("Invalid operator for strings")),
            },
            (l, r) => match op {
                BinaryOp::Equal => Ok(PyObject::Bool(l == r)),
                BinaryOp::NotEqual => Ok(PyObject::Bool(l != r)),
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
            _ => true,
        }
    }
}
