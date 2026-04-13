use crate::ast::{BinaryOp, Expr, Literal, Stmt};
use crate::wasm_builder::{BlockType, WasmBuilder, WasmOp};
use std::collections::HashMap;

#[allow(dead_code)]
pub struct Codegen {
    builder: WasmBuilder,
    locals: HashMap<String, u32>,
    local_count: u32,
}

#[allow(dead_code)]
impl Codegen {
    pub fn new() -> Self {
        Self {
            builder: WasmBuilder::new(),
            locals: HashMap::new(),
            local_count: 0,
        }
    }

    pub fn gen_program(mut self, statements: &[Stmt]) -> Vec<u8> {
        for stmt in statements {
            self.gen_stmt(stmt);
        }
        self.builder.finish()
    }

    fn gen_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Expression(expr) => {
                self.gen_expr(expr);
                self.builder.push_op(WasmOp::Drop);
            }
            Stmt::Assignment(target, expr) => {
                self.gen_expr(expr);
                if let Expr::Variable(name) = target {
                    let local_idx = self.get_or_create_local(name);
                    self.builder.push_op(WasmOp::LocalSet(local_idx));
                } else {
                    todo!("Non-variable assignment target in codegen");
                }
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.gen_expr(condition);
                self.builder.push_op(WasmOp::If(BlockType::Empty));
                for s in then_branch {
                    self.gen_stmt(s);
                }
                if let Some(else_stmts) = else_branch {
                    self.builder.push_op(WasmOp::Else);
                    for s in else_stmts {
                        self.gen_stmt(s);
                    }
                }
                self.builder.push_op(WasmOp::End);
            }
            Stmt::While { condition, body } => {
                let loop_label = self.builder.create_label();

                self.builder.push_op(WasmOp::Loop(BlockType::Empty));
                self.builder.set_label(loop_label);

                self.gen_expr(condition);
                self.builder.push_op(WasmOp::If(BlockType::Empty));

                for s in body {
                    self.gen_stmt(s);
                }

                // Jump back to loop start
                // Note: In WASM, br to a loop jumps to the start.
                // We need to resolve the label to a relative depth later.
                self.builder.push_op(WasmOp::Br(loop_label));

                self.builder.push_op(WasmOp::End); // End If
                self.builder.push_op(WasmOp::End); // End Loop
            }
            _ => todo!("Statement type not supported in codegen yet"),
        }
    }

    fn gen_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Literal(lit) => match lit {
                Literal::Int(n) => self.builder.push_op(WasmOp::I32Const(*n as i32)),
                Literal::Bool(b) => self
                    .builder
                    .push_op(WasmOp::I32Const(if *b { 1 } else { 0 })),
                _ => todo!("Literal type not supported in codegen yet"),
            },
            Expr::Variable(name) => {
                let idx = *self.locals.get(name).expect("Undefined variable");
                self.builder.push_op(WasmOp::LocalGet(idx));
            }
            Expr::Binary(left, op, right) => {
                self.gen_expr(left);
                self.gen_expr(right);
                match op {
                    BinaryOp::Add => self.builder.push_op(WasmOp::I32Add),
                    BinaryOp::Sub => self.builder.push_op(WasmOp::I32Sub),
                    BinaryOp::Mul => self.builder.push_op(WasmOp::I32Mul),
                    BinaryOp::Div => self.builder.push_op(WasmOp::I32DivS),
                    BinaryOp::Mod => self.builder.push_op(WasmOp::I32RemS),
                    BinaryOp::Equal => self.builder.push_op(WasmOp::I32Eq),
                    BinaryOp::NotEqual => self.builder.push_op(WasmOp::I32Ne),
                    BinaryOp::Less => self.builder.push_op(WasmOp::I32LtS),
                    BinaryOp::Greater => self.builder.push_op(WasmOp::I32GtS),
                    BinaryOp::LessEqual => self.builder.push_op(WasmOp::I32LeS),
                    BinaryOp::GreaterEqual => self.builder.push_op(WasmOp::I32GeS),
                    BinaryOp::In => todo!("In operator not supported in codegen yet"),
                    BinaryOp::BitwiseAnd => self.builder.push_op(WasmOp::I32And),
                    BinaryOp::BitwiseOr => self.builder.push_op(WasmOp::I32Or),
                    BinaryOp::BitwiseXor => self.builder.push_op(WasmOp::I32Xor),
                }
            }
            _ => todo!("Expression type not supported in codegen yet"),
        }
    }

    fn get_or_create_local(&mut self, name: &str) -> u32 {
        if let Some(&idx) = self.locals.get(name) {
            idx
        } else {
            let idx = self.local_count;
            self.locals.insert(name.to_string(), idx);
            self.local_count += 1;
            idx
        }
    }
}
