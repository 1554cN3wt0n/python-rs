use std::collections::HashMap;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Label(pub usize);

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum WasmOp {
    I32Const(i32),
    I32Add,
    I32Sub,
    I32Mul,
    I32DivS,
    I32RemS,
    I32Eq,
    I32Ne,
    I32LtS,
    I32GtS,
    I32LeS,
    I32GeS,
    I32And,
    I32Or,
    I32Xor,
    LocalGet(u32),
    LocalSet(u32),
    Call(u32),
    Drop,
    If(BlockType),
    Else,
    Loop(BlockType),
    Block(BlockType),
    Br(Label),
    End,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub enum BlockType {
    Empty,
}

#[allow(dead_code)]
pub struct WasmBuilder {
    code: Vec<WasmOp>,
    labels: HashMap<Label, usize>,
    label_counter: usize,
}

#[allow(dead_code)]
impl WasmBuilder {
    pub fn new() -> Self {
        Self {
            code: Vec::new(),
            labels: HashMap::new(),
            label_counter: 0,
        }
    }

    pub fn create_label(&mut self) -> Label {
        let label = Label(self.label_counter);
        self.label_counter += 1;
        label
    }

    pub fn set_label(&mut self, label: Label) {
        self.labels.insert(label, self.code.len());
    }

    pub fn push_op(&mut self, op: WasmOp) {
        self.code.push(op);
    }

    pub fn finish(self) -> Vec<u8> {
        let mut bytes = Vec::new();
        // Magic number
        bytes.extend_from_slice(b"\0asm");
        // Version 1
        bytes.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]);

        // Type section (0x01)
        // Function section (0x03)
        // Export section (0x07)
        // Code section (0x0a)

        // For now, let's just implement a minimal placeholder to see if it links
        bytes
    }
}
