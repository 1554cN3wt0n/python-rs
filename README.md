# PyRS: A Python-Like Interpreter in Rust

PyRS is a lightweight, high-performance interpreter for a Python-like language, implemented entirely in Rust. It aims to provide a familiar syntax for Python developers while leveraging Rust's safety and speed for the underlying execution engine.

## 🚀 Features

PyRS supports a significant subset of Python's core language features:

- **Object-Oriented Programming**: Full support for classes, methods, and inheritance.
- **Generators & Iteration**: Stateful `yield` support, allowing for lazy sequence generation and complex iteration patterns.
- **Collections**: Built-in support for Lists, Tuples, Dictionaries (via HashMaps), and Sets.
- **Control Flow**: `if/elif/else`, `while` loops, and `for` loops (including sequence unpacking).
- **Exception Handling**: Robust `try/except` blocks and manual `raise` support.
- **Functional Tools**: Anonymous functions via `lambda` and list comprehensions.
- **Modern Syntax**: Support for F-Strings (formatted string literals).
- **Standard Library**: Core built-ins like `print()`, `len()`, `range()`, `type()`, `isinstance()`, `str()`, and `set()`.

## 🛠 Architecture

The project is structured into several modular components:

1.  **Lexer (`src/lexer.rs`)**: Uses the `Logos` crate to transform source text into a stream of tokens, handling complex indentation-based scoping.
2.  **Parser (`src/parser.rs`)**: A recursive descent parser that converts tokens into a structured Abstract Syntax Tree (AST).
3.  **AST (`src/ast.rs`)**: Defines the language grammar and node types.
4.  **Evaluator (`src/eval.rs`)**: The core execution engine. It handles scope management (environments) and features a state-machine-based "mini-VM" to support generator suspension and resumption.
5.  **Object Model (`src/object.rs`)**: Defines the `PyObject` representation, including the dunder method lookup system (e.g., `__add__`, `__str__`).

## 📦 Installation

Ensure you have Rust and Cargo installed. Then, clone the repository and build:

```bash
git clone https://github.com/elvin-mark/python-rs.git
cd python-rs
cargo build --release
```

## 🖥 Usage

You can run PyRS scripts by passing the file path to the executable:

```bash
cargo run -- path/to/your_script.pyrs
```

Check the `examples/` directory for sample scripts:

```bash
# Run the complex feature demo
cargo run -- examples/complex_demo.pyrs

# Run the generator test
cargo run -- examples/generator_test.pyrs
```

## 📝 Example

```python
class Vector:
    def __init__(self, x, y):
        self.x = x
        self.y = y
    
    def __add__(self, other):
        return Vector(self.x + other.x, self.y + other.y)
    
    def __str__(self):
        return "Vector(" + str(self.x) + ", " + str(self.y) + ")"

def counter(n):
    i = 0
    while i < n:
        yield i
        i = i + 1

v1 = Vector(1, 2)
v2 = Vector(3, 4)
print("Result:", v1 + v2)

for val in counter(5):
    print("Count:", val)
```
