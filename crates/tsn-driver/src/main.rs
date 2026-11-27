use std::{env, fs, path::Path, process::Command, process::ExitCode};

use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;

mod compiler;
mod lsp;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("version") | Some("--version") | Some("-V") => {
            println!("tsn {} (compiler spike)", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("check") => match args.next() {
            Some(path) => check_file(&path),
            None => {
                eprintln!("usage: tsn check <file.ts>");
                ExitCode::from(2)
            }
        },
        Some("lsp") => lsp::run(),
        Some("build") => {
            let Some(input) = args.next() else {
                eprintln!("usage: tsn build <file.ts> -o <output>");
                return ExitCode::from(2);
            };
            let mut output = None;
            while let Some(arg) = args.next() {
                if arg == "-o" || arg == "--output" {
                    output = args.next();
                    break;
                }
            }
            match output {
                Some(output) => build_source(&input, &output),
                None => {
                    eprintln!("tsn: missing `-o <output>`");
                    ExitCode::from(2)
                }
            }
        }
        Some("compile") => {
            let Some(input) = args.next() else {
                eprintln!("usage: tsn compile <file.ts> -o <executable>");
                return ExitCode::from(2);
            };
            let output = args
                .skip_while(|arg| arg != "-o" && arg != "--output")
                .nth(1);
            match output {
                Some(output) => match compiler::compile_module(&input, &output) {
                    Ok(()) => {
                        println!("compiled {input} -> {output}");
                        ExitCode::SUCCESS
                    }
                    Err(error) => {
                        eprintln!("{}", error.render());
                        ExitCode::from(1)
                    }
                },
                None => {
                    eprintln!("tsn: missing `-o <output>`");
                    ExitCode::from(2)
                }
            }
        }
        Some("help") | Some("--help") | Some("-h") | None => {
            println!(
                "tsn — TypeScript-shaped native compiler (early spike)\n\nUsage:\n  tsn check <file.ts>\n  tsn build <file.ts> -o <output>\n  tsn compile <file.ts> -o <executable>\n  tsn lsp                 Run the experimental stdio language server\n  tsn version\n  tsn help\n\n`compile` supports a small explicitly typed subset and emits a standalone native executable.\n`lsp` supports initialize/shutdown/exit, didOpen and full-text didChange with parser diagnostics only; semantic checks, completion, hover and rename are not supported."
            );
            ExitCode::SUCCESS
        }
        Some(command) => {
            eprintln!("tsn: unknown command `{command}`; try `tsn help`");
            ExitCode::from(2)
        }
    }
}

fn build_source(input: &str, output: &str) -> ExitCode {
    let source = match fs::read_to_string(input) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("tsn: cannot read `{input}`: {error}");
            return ExitCode::from(2);
        }
    };
    let allocator = Allocator::default();
    let source_type = SourceType::from_path(input).unwrap_or_else(|_| SourceType::ts());
    let parsed = Parser::new(&allocator, &source, source_type).parse();
    if !parsed.diagnostics.is_empty() {
        for diagnostic in parsed.diagnostics {
            eprintln!("error[TSN1000]:\n --> {input}\n{diagnostic}");
        }
        return ExitCode::from(1);
    }

    if let Some(message) = parse_print_literal(&source) {
        return build_hello_message(message, output);
    }

    eprintln!(
        "tsn: `build` currently supports only print(<string literal>); use `compile` for typed integer modules"
    );
    ExitCode::from(1)
}

fn build_hello_message(message: String, output: &str) -> ExitCode {
    let mut bytes = message.into_bytes();
    bytes.push(b'\n');
    let llvm_bytes = bytes
        .iter()
        .map(|byte| format!("\\{byte:02X}"))
        .collect::<String>();
    let ir = format!(
        "@.message = private unnamed_addr constant [{} x i8] c\"{}\"\n\
         declare i64 @write(i32, ptr, i64)\n\
         define i32 @main() {{\nentry:\n  %written = call i64 @write(i32 1, ptr @.message, i64 {})\n  ret i32 0\n}}\n",
        bytes.len(),
        llvm_bytes,
        bytes.len()
    );

    let output_path = Path::new(output);
    let ir_path = output_path.with_extension("ll");
    if let Err(error) = fs::write(&ir_path, ir) {
        eprintln!("tsn: cannot write `{}`: {error}", ir_path.display());
        return ExitCode::from(2);
    }
    let result = Command::new("clang")
        .arg("-O2")
        .arg("-static")
        .arg(&ir_path)
        .arg("-o")
        .arg(output_path)
        .status();
    match result {
        Ok(status) if status.success() => {
            println!("built {output}");
            ExitCode::SUCCESS
        }
        Ok(status) => {
            eprintln!("tsn: clang failed with {status}");
            ExitCode::from(1)
        }
        Err(error) => {
            eprintln!("tsn: failed to run clang: {error}");
            ExitCode::from(2)
        }
    }
}

fn parse_print_literal(source: &str) -> Option<String> {
    let trimmed = source.trim().trim_end_matches(';').trim();
    let inner = trimmed.strip_prefix("print(")?.strip_suffix(')')?.trim();
    let literal = inner.strip_prefix('"')?.strip_suffix('"')?;
    if literal.contains(['\\', '"', '\n', '\r']) {
        return None;
    }
    Some(literal.to_owned())
}

fn check_file(path: &str) -> ExitCode {
    match compiler::check_module(path) {
        Ok(()) => {
            println!("{path}: type check ok");
            ExitCode::SUCCESS
        }
        Err(diagnostic) => {
            eprintln!("{}", diagnostic.render());
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_print_literal;

    #[test]
    fn extracts_m0_string_literal() {
        assert_eq!(parse_print_literal("print(\"hi\");\n"), Some("hi".into()));
    }

    #[test]
    fn rejects_non_literal_and_escapes_for_now() {
        assert_eq!(parse_print_literal("print(name);"), None);
        assert_eq!(parse_print_literal("print(\"a\\nb\");"), None);
    }
}
