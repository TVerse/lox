use anyhow::Result;
use clap::Parser;
use env_logger::Builder;
use log::{error, LevelFilter};
use lox::interpret;
use std::io::BufRead;
use std::io::Write;
use std::path::PathBuf;

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long)]
    file: Option<PathBuf>,
}

fn main() -> Result<()> {
    init_logger();
    let args = Args::parse();

    if let Some(path) = args.file {
        run_file(&path)?;
    } else {
        repl()?
    }

    Ok(())
}

fn repl() -> Result<()> {
    let mut stdout = std::io::stdout();
    write!(stdout, ">")?;
    stdout.flush()?;
    let stdin = std::io::stdin();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.is_empty() {
            break;
        }
        match interpret(&line, &mut std::io::stdout()) {
            Ok(_) => {}
            Err(e) => error!("Error: {e}"),
        }
        let mut stdout = std::io::stdout();
        write!(stdout, ">")?;
        stdout.flush()?;
    }
    Ok(())
}

fn run_file(path: &PathBuf) -> Result<()> {
    let contents = std::fs::read_to_string(path)?;
    interpret(&contents, &mut std::io::stdout())?;
    Ok(())
}

fn init_logger() {
    let mut builder = Builder::new();
    if cfg!(debug_assertions) {
        builder.filter_level(LevelFilter::Trace);
    }
    builder.init()
}
