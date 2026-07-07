use std::env;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        eprintln!("Usage: {} <file.c> <pattern>", args[0]);
        std::process::exit(1);
    }

    let _file = PathBuf::from(&args[1]);
    let _pattern = &args[2];

    // TODO: Implement CLI
    // 1. Load C file via CodeBase::load
    // 2. Saturate with default rules
    // 3. Search for pattern
    // 4. Print matches in grep-like format
}
