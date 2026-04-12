mod ast;
mod codegen;
mod env;
mod eval;
mod lexer;
mod object;
mod parser;
mod wasm_builder;

use anyhow::{Context, Result};
use eval::Evaluator;
use lexer::Lexer;
use parser::Parser;
use rustyline::DefaultEditor;
use std::env as std_env;
use std::fs;

fn main() -> Result<()> {
    let args: Vec<String> = std_env::args().collect();
    let mut evaluator = Evaluator::new();

    if args.len() > 1 {
        run_file(&args[1], &mut evaluator)?;
    } else {
        run_repl(&mut evaluator)?;
    }

    Ok(())
}

fn run_file(path: &str, evaluator: &mut Evaluator) -> Result<()> {
    if let Some(parent) = std::path::Path::new(path).parent()
        && let Some(s) = parent.to_str()
        && !s.is_empty()
    {
        evaluator.add_load_path(s.to_string());
    }
    let source =
        fs::read_to_string(path).with_context(|| format!("Failed to read file: {}", path))?;
    execute(&source, evaluator)
}

fn run_repl(evaluator: &mut Evaluator) -> Result<()> {
    let mut rl = DefaultEditor::new()?;
    println!("PyRS 0.1.0");
    println!("Use Ctrl-D to exit.");

    loop {
        let readline = rl.readline(">>> ");
        match readline {
            Ok(line) => {
                rl.add_history_entry(line.as_str())?;
                if let Err(e) = execute(&line, evaluator) {
                    eprintln!("Error: {}", e);
                }
            }
            Err(_) => {
                println!("Goodbye!");
                break;
            }
        }
    }
    Ok(())
}

fn execute(source: &str, evaluator: &mut Evaluator) -> Result<()> {
    let lexer = Lexer::new(source);
    let mut parser = Parser::new(lexer);
    let statements = parser.parse()?;
    evaluator.eval(&statements)?;
    Ok(())
}
