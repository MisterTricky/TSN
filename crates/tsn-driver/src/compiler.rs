//! AST-based native compiler slice for the standalone benchmark.
//!
//! Oxc parses TS syntax; this module accepts a small typed subset, lowers the
//! AST to a typed C intermediate, then uses Clang/LLVM to emit optimized IR and
//! a standalone ELF. Unsupported syntax is rejected rather than ignored.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    AssignmentTarget, BinaryOperator, Declaration, Expression, ForStatementInit, Function,
    ImportDeclarationSpecifier, ImportOrExportKind, LogicalOperator, ModuleExportName, Statement,
    UnaryOperator, VariableDeclarationKind,
};
use oxc_parser::Parser;
use oxc_span::{GetSpan, SourceType, Span};

const DIAGNOSTIC_MARKER: &str = "\u{1e}TSN|";

#[derive(Debug)]
pub(crate) struct Diagnostic {
    code: String,
    message: String,
    path: String,
    source: String,
    span: Option<Span>,
}

impl Diagnostic {
    fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        path: &str,
        source: &str,
        span: Option<Span>,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            path: path.to_string(),
            source: source.to_string(),
            span,
        }
    }

    pub(crate) fn render(&self) -> String {
        let Some(span) = self.span else {
            return format!("error[{}]: {}\n --> {}", self.code, self.message, self.path);
        };
        let start = (span.start as usize).min(self.source.len());
        let end = (span.end as usize).min(self.source.len()).max(start);
        let mut line_start = 0;
        let mut line_number = 1;
        let mut cursor = 0;
        while cursor < start {
            let Some(ch) = self.source[cursor..].chars().next() else {
                break;
            };
            match ch {
                '\r' => {
                    cursor += 1;
                    if cursor < start && self.source[cursor..].starts_with('\n') {
                        cursor += 1;
                    }
                    line_start = cursor;
                    line_number += 1;
                }
                '\n' | '\u{2028}' | '\u{2029}' => {
                    cursor += ch.len_utf8();
                    line_start = cursor;
                    line_number += 1;
                }
                _ => cursor += ch.len_utf8(),
            }
        }
        let column = self.source[line_start..start].chars().count() + 1;
        let mut line_end = start;
        while line_end < self.source.len() {
            let Some(ch) = self.source[line_end..].chars().next() else {
                break;
            };
            if matches!(ch, '\r' | '\n' | '\u{2028}' | '\u{2029}') {
                break;
            }
            line_end += ch.len_utf8();
        }
        let line = &self.source[line_start..line_end];
        let caret_len = self.source[start..end].chars().count().max(1);
        format!(
            "error[{}]: {}\n --> {}:{}:{}\n  |\n{:>3} | {}\n  | {}{}",
            self.code,
            self.message,
            self.path,
            line_number,
            column,
            line_number,
            line,
            " ".repeat(column - 1),
            "^".repeat(caret_len)
        )
    }

    fn from_lowering_error(path: &str, source: &str, error: String) -> Self {
        if let Some(marker) = error.strip_prefix(DIAGNOSTIC_MARKER) {
            let mut parts = marker.splitn(5, '|');
            if let (Some(code), Some(module_path), Some(start), Some(end), Some(message)) = (
                parts.next(),
                parts.next(),
                parts.next(),
                parts.next(),
                parts.next(),
            ) && let (Ok(start), Ok(end)) = (start.parse::<u32>(), end.parse::<u32>())
            {
                let diagnostic_path = if module_path.is_empty() {
                    path.to_string()
                } else {
                    module_path.to_string()
                };
                let module_source = if module_path.is_empty() {
                    source.to_string()
                } else {
                    fs::read_to_string(module_path).unwrap_or_else(|_| source.to_string())
                };
                return Self::new(
                    code.to_string(),
                    message,
                    &diagnostic_path,
                    &module_source,
                    Some(Span::new(start, end)),
                );
            }
        }
        Self::new("TSN2000", error, path, source, None)
    }
}

fn marked_type_error(span: Option<Span>, message: impl Into<String>) -> String {
    marked_diagnostic("TSN2001", span, message)
}

fn marked_diagnostic(code: &str, span: Option<Span>, message: impl Into<String>) -> String {
    marked_diagnostic_in_module(code, None, span, message)
}

fn marked_diagnostic_in_module(
    code: &str,
    path: Option<&Path>,
    span: Option<Span>,
    message: impl Into<String>,
) -> String {
    let message = message.into();
    let path = path.map_or_else(String::new, |path| path.display().to_string());
    match span {
        Some(span) => format!(
            "{DIAGNOSTIC_MARKER}{code}|{path}|{}|{}|{message}",
            span.start, span.end
        ),
        None => format!("{DIAGNOSTIC_MARKER}{code}|{path}|0|0|{message}"),
    }
}

fn with_module_path(error: String, path: &Path) -> String {
    if let Some(marker) = error.strip_prefix(DIAGNOSTIC_MARKER) {
        let mut parts = marker.splitn(5, '|');
        if let (Some(code), Some(existing_path), Some(start), Some(end), Some(message)) = (
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
        ) {
            if existing_path.is_empty() {
                return format!(
                    "{DIAGNOSTIC_MARKER}{code}|{}|{start}|{end}|{message}",
                    path.display()
                );
            }
            return error;
        }
    }
    marked_diagnostic_in_module("TSN2000", Some(path), None, error)
}

fn statement_always_returns(statement: &Statement<'_>) -> bool {
    match statement {
        Statement::ReturnStatement(_) => true,
        // In a block, reaching any definitely-returning statement guarantees
        // the whole block returns. This remains correct only because nested
        // blocks and conditionals are themselves analyzed path-sensitively.
        Statement::BlockStatement(block) => block.body.iter().any(statement_always_returns),
        Statement::IfStatement(if_statement) => {
            if_statement.alternate.as_ref().is_some_and(|alt| {
                statement_always_returns(&if_statement.consequent) && statement_always_returns(alt)
            })
        }
        _ => false,
    }
}

fn parse_annotated_type(
    source: &str,
    annotation: Option<&oxc_ast::ast::TSTypeAnnotation<'_>>,
    fallback_span: Span,
    what: &str,
) -> Result<CType, String> {
    let Some(annotation) = annotation else {
        return Err(marked_type_error(
            Some(fallback_span),
            format!("{what} needs an explicit type annotation"),
        ));
    };
    CType::parse(source, Some(annotation.span), what)
}

struct LocalImport {
    target: PathBuf,
    imported: String,
    local: String,
}

struct ModuleSource {
    path: PathBuf,
    source: String,
    local_imports: Vec<LocalImport>,
}

fn import_name(specifier: &oxc_ast::ast::ImportSpecifier<'_>) -> Result<String, String> {
    match &specifier.imported {
        ModuleExportName::IdentifierName(name) => Ok(name.name.to_string()),
        ModuleExportName::StringLiteral(name) => name
            .value
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| "imported name must be valid UTF-8".into()),
        ModuleExportName::IdentifierReference(name) => Ok(name.name.to_string()),
    }
}

fn resolve_local_import(from: &Path, specifier: &str) -> Result<PathBuf, String> {
    if !specifier.starts_with("./") && !specifier.starts_with("../") {
        return Err(format!(
            "local import `{specifier}` must start with `./` or `../`"
        ));
    }
    let mut candidate = from
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(specifier);
    if !candidate.exists() && candidate.extension().is_none() {
        candidate.set_extension("ts");
    }
    candidate
        .canonicalize()
        .map_err(|error| format!("cannot resolve `{specifier}`: {error}"))
}

fn load_module_graph(input: &str) -> Result<Vec<ModuleSource>, Diagnostic> {
    let root = Path::new(input).canonicalize().map_err(|error| {
        Diagnostic::new(
            "TSN9000",
            format!("cannot resolve source: {error}"),
            input,
            "",
            None,
        )
    })?;
    let mut queue = VecDeque::from([root.clone()]);
    let mut visited = HashSet::new();
    let mut modules = Vec::new();

    while let Some(path) = queue.pop_front() {
        if !visited.insert(path.clone()) {
            continue;
        }
        let path_display = path.display().to_string();
        let source = fs::read_to_string(&path).map_err(|error| {
            Diagnostic::new(
                "TSN9000",
                format!("cannot read module: {error}"),
                &path_display,
                "",
                None,
            )
        })?;
        let allocator = Allocator::default();
        let source_type = SourceType::from_path(&path).unwrap_or_else(|_| SourceType::ts());
        let parsed = Parser::new(&allocator, &source, source_type).parse();
        if !parsed.diagnostics.is_empty() {
            return Err(Diagnostic::new(
                "TSN1000",
                parsed
                    .diagnostics
                    .into_iter()
                    .map(|d| d.to_string())
                    .collect::<Vec<_>>()
                    .join("\n"),
                &path_display,
                &source,
                None,
            ));
        }

        let mut local_imports = Vec::new();
        for statement in &parsed.program.body {
            let Statement::ImportDeclaration(import) = statement else {
                continue;
            };
            let module = import.source.value.as_str().ok_or_else(|| {
                Diagnostic::new(
                    "TSN2000",
                    "module specifier must be UTF-8",
                    &path_display,
                    &source,
                    Some(import.span),
                )
            })?;
            if import.import_kind == ImportOrExportKind::Type {
                continue;
            }
            if module.starts_with("tsn:") {
                continue;
            }
            let target = resolve_local_import(&path, module).map_err(|error| {
                Diagnostic::new("TSN2000", error, &path_display, &source, Some(import.span))
            })?;
            let specifiers = import.specifiers.as_ref().ok_or_else(|| {
                Diagnostic::new(
                    "TSN2000",
                    "side-effect imports are unsupported",
                    &path_display,
                    &source,
                    Some(import.span),
                )
            })?;
            for item in specifiers {
                let ImportDeclarationSpecifier::ImportSpecifier(specifier) = item else {
                    return Err(Diagnostic::new(
                        "TSN2000",
                        "only named local imports are supported",
                        &path_display,
                        &source,
                        Some(import.span),
                    ));
                };
                if specifier.import_kind == ImportOrExportKind::Type {
                    continue;
                }
                let imported = import_name(specifier).map_err(|error| {
                    Diagnostic::new(
                        "TSN2000",
                        error,
                        &path_display,
                        &source,
                        Some(specifier.span),
                    )
                })?;
                local_imports.push(LocalImport {
                    target: target.clone(),
                    imported,
                    local: specifier.local.name.to_string(),
                });
            }
            queue.push_back(target);
        }
        modules.push(ModuleSource {
            path,
            source,
            local_imports,
        });
    }

    modules.sort_by_key(|module| if module.path == root { 0 } else { 1 });
    Ok(modules)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CType {
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
    F32,
    F64,
    Bool,
    String,
    U8Array,
    U32Array,
    IntLiteral,
    FloatLiteral,
    Void,
}

impl CType {
    fn parse(source: &str, span: Option<Span>, what: &str) -> Result<Self, String> {
        let span = span.ok_or_else(|| format!("{what} needs an explicit type annotation"))?;
        let annotation = source[span.start as usize..span.end as usize]
            .trim()
            .trim_start_matches(':')
            .trim();
        match annotation {
            "u8" => Ok(Self::U8),
            "u16" => Ok(Self::U16),
            "u32" => Ok(Self::U32),
            "u64" => Ok(Self::U64),
            "i8" => Ok(Self::I8),
            "i16" => Ok(Self::I16),
            "i32" => Ok(Self::I32),
            "i64" => Ok(Self::I64),
            "f32" => Ok(Self::F32),
            "f64" | "number" => Ok(Self::F64),
            "bool" => Ok(Self::Bool),
            "string" => Ok(Self::String),
            "Uint8Array" => Ok(Self::U8Array),
            "Uint32Array" => Ok(Self::U32Array),
            "void" => Ok(Self::Void),
            _ => Err(marked_type_error(
                Some(span),
                format!("{what} has unsupported type `{annotation}`"),
            )),
        }
    }

    fn c_name(self) -> &'static str {
        match self {
            Self::U8 => "uint8_t",
            Self::U16 => "uint16_t",
            Self::U32 => "uint32_t",
            Self::U64 => "uint64_t",
            Self::I8 => "int8_t",
            Self::I16 => "int16_t",
            Self::I32 => "int32_t",
            Self::I64 => "int64_t",
            Self::F32 => "float",
            Self::F64 => "double",
            Self::Bool => "_Bool",
            Self::String => "TsnString",
            Self::U8Array => "TsnByteArray *",
            Self::U32Array => "TsnWordArray *",
            Self::IntLiteral => "int",
            Self::FloatLiteral => "double",
            Self::Void => "void",
        }
    }

    fn is_integer(self) -> bool {
        matches!(
            self,
            Self::U8
                | Self::U16
                | Self::U32
                | Self::U64
                | Self::I8
                | Self::I16
                | Self::I32
                | Self::I64
                | Self::IntLiteral
        )
    }

    fn is_numeric(self) -> bool {
        self.is_integer() || matches!(self, Self::F32 | Self::F64 | Self::FloatLiteral)
    }
}

fn prepare_module(input: &str) -> Result<(String, String), Diagnostic> {
    let modules = load_module_graph(input)?;
    let root = modules
        .first()
        .ok_or_else(|| Diagnostic::new("TSN9000", "module graph is empty", input, "", None))?;
    let allocator = Allocator::default();
    let parsed = modules
        .iter()
        .map(|module| {
            let source_type =
                SourceType::from_path(&module.path).unwrap_or_else(|_| SourceType::ts());
            Parser::new(&allocator, &module.source, source_type).parse()
        })
        .collect::<Vec<_>>();
    let programs = parsed
        .iter()
        .map(|parsed| &parsed.program)
        .collect::<Vec<_>>();
    let generated = emit_c_modules(&modules, &programs)
        .map_err(|error| Diagnostic::from_lowering_error(input, &root.source, error))?;
    Ok((root.source.clone(), generated))
}

pub(crate) fn check_module(input: &str) -> Result<(), Diagnostic> {
    let (source, generated) = prepare_module(input)?;
    validate_c_source(input, &source, &generated)
}

fn validate_c_source(input: &str, source: &str, generated: &str) -> Result<(), Diagnostic> {
    let mut child = Command::new("clang")
        .args([
            "-x",
            "c",
            "-fsyntax-only",
            "-fwrapv",
            "-pthread",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-Wconversion",
            "-Werror=conversion",
            "-Werror=sign-conversion",
            "-Werror=return-type",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            Diagnostic::new(
                "TSN9001",
                format!("failed to start Clang: {e}"),
                input,
                source,
                None,
            )
        })?;
    let mut stdin = child.stdin.take().ok_or_else(|| {
        Diagnostic::new("TSN9001", "failed to open Clang stdin", input, source, None)
    })?;
    stdin.write_all(generated.as_bytes()).map_err(|e| {
        Diagnostic::new(
            "TSN9001",
            format!("failed to send generated source to Clang: {e}"),
            input,
            source,
            None,
        )
    })?;
    drop(stdin);
    let output = child.wait_with_output().map_err(|e| {
        Diagnostic::new(
            "TSN9001",
            format!("failed to wait for Clang: {e}"),
            input,
            source,
            None,
        )
    })?;
    if output.status.success() {
        Ok(())
    } else {
        let backend_message = String::from_utf8_lossy(&output.stderr);
        Err(Diagnostic::new(
            "TSN9002",
            format!("generated-code validation failed:\n{backend_message}"),
            input,
            source,
            None,
        ))
    }
}

pub(crate) fn compile_module(input: &str, output: &str) -> Result<(), Diagnostic> {
    let (source, generated) = prepare_module(input)?;

    let output_path = Path::new(output);
    let c_path = output_path.with_extension("c");
    let ir_path = output_path.with_extension("ll");
    fs::write(&c_path, generated).map_err(|e| {
        Diagnostic::new(
            "TSN9000",
            format!("cannot write generated C: {e}"),
            input,
            &source,
            None,
        )
    })?;

    let status = Command::new("clang")
        .args([
            "-O3",
            "-march=native",
            "-fwrapv",
            "-pthread",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-Wconversion",
            "-Werror=conversion",
            "-Werror=sign-conversion",
            "-Werror=return-type",
            "-S",
            "-emit-llvm",
        ])
        .arg(&c_path)
        .arg("-o")
        .arg(&ir_path)
        .status()
        .map_err(|e| {
            Diagnostic::new(
                "TSN9001",
                format!("failed to run Clang: {e}"),
                input,
                &source,
                None,
            )
        })?;
    if !status.success() {
        return Err(Diagnostic::new(
            "TSN9002",
            format!("Clang rejected generated code ({status}); inspect the .c intermediate"),
            input,
            &source,
            None,
        ));
    }
    let status = Command::new("clang")
        .args(["-O0", "-march=native", "-fwrapv", "-pthread", "-static"])
        .arg(&ir_path)
        .arg("-o")
        .arg(output)
        .status()
        .map_err(|e| {
            Diagnostic::new(
                "TSN9001",
                format!("failed to run native linker: {e}"),
                input,
                &source,
                None,
            )
        })?;
    if !status.success() {
        return Err(Diagnostic::new(
            "TSN9003",
            format!("native link failed ({status})"),
            input,
            &source,
            None,
        ));
    }
    Ok(())
}

fn emit_c_modules(
    modules: &[ModuleSource],
    programs: &[&oxc_ast::ast::Program<'_>],
) -> Result<String, String> {
    if modules.len() != programs.len() {
        return Err("internal error: module/program count mismatch".into());
    }

    struct Entry<'f, 'a> {
        module: usize,
        function: &'f Function<'a>,
    }

    let mut entries = Vec::new();
    let mut module_builtins = Vec::with_capacity(modules.len());
    let mut exports = vec![HashSet::<String>::new(); modules.len()];
    for (module_index, program) in programs.iter().enumerate() {
        let mut builtins = HashMap::new();
        for statement in &program.body {
            match statement {
                Statement::ImportDeclaration(import) => {
                    if import.import_kind == ImportOrExportKind::Type {
                        continue;
                    }
                    let module_specifier = import
                        .source
                        .value
                        .as_str()
                        .ok_or("module specifier must be valid UTF-8")?;
                    if !module_specifier.starts_with("tsn:") {
                        continue;
                    }
                    let specifiers = import
                        .specifiers
                        .as_ref()
                        .ok_or("side-effect imports are unsupported")?;
                    for item in specifiers {
                        let ImportDeclarationSpecifier::ImportSpecifier(specifier) = item else {
                            return Err("only named built-in imports are supported".into());
                        };
                        if specifier.import_kind == ImportOrExportKind::Type {
                            continue;
                        }
                        let imported = import_name(specifier)?;
                        let builtin = match (module_specifier, imported.as_str()) {
                            ("tsn:time", "monotonicMs") => Builtin::MonotonicMs,
                            ("tsn:io", "printString") => Builtin::PrintString,
                            ("tsn:io", "printU32") => Builtin::PrintU32,
                            ("tsn:io", "printI32") => Builtin::PrintI32,
                            ("tsn:io", "printF64") => Builtin::PrintF64,
                            ("tsn:io", "readStdin") => Builtin::ReadStdin,
                            ("tsn:strings", "sliceUtf8") => Builtin::SliceUtf8,
                            ("tsn:strings", "stringByteAt") => Builtin::StringByteAt,
                            ("tsn:net", "listenTcp") => Builtin::ListenTcp,
                            ("tsn:net", "acceptTcp") => Builtin::AcceptTcp,
                            ("tsn:net", "readTcp") => Builtin::ReadTcp,
                            ("tsn:net", "writeTcp") => Builtin::WriteTcp,
                            ("tsn:net", "writeTcpString") => Builtin::WriteTcpString,
                            ("tsn:net", "closeTcp") => Builtin::CloseTcp,
                            ("tsn:os", "shutdownRequested") => Builtin::ShutdownRequested,
                            ("tsn:os", "spawnThread") => Builtin::SpawnThread,
                            ("tsn:bench", "printBenchmark") => Builtin::PrintBenchmark,
                            _ => {
                                return Err(format!(
                                    "unknown standard-library import `{imported}` from `{module_specifier}`"
                                ));
                            }
                        };
                        let local = specifier.local.name.to_string();
                        if builtins.insert(local.clone(), builtin).is_some() {
                            return Err(format!("duplicate import binding `{local}`"));
                        }
                    }
                }
                Statement::FunctionDeclaration(function) => {
                    if function.r#async || function.generator {
                        return Err(
                            "async and generator functions are unsupported in this compiler slice"
                                .into(),
                        );
                    }
                    entries.push(Entry {
                        module: module_index,
                        function,
                    });
                }
                Statement::ExportDeclaration(export) => match &export.declaration {
                    Declaration::FunctionDeclaration(function) => {
                        if function.r#async || function.generator {
                            return Err("async and generator functions are unsupported in this compiler slice".into());
                        }
                        let name = function
                            .id
                            .as_ref()
                            .ok_or("anonymous exported function")?
                            .name
                            .to_string();
                        exports[module_index].insert(name);
                        entries.push(Entry {
                            module: module_index,
                            function,
                        });
                    }
                    _ => return Err("only function declarations may be exported".into()),
                },
                _ => {
                    return Err(
                        "only imports and top-level function declarations are supported".into(),
                    );
                }
            }
        }
        module_builtins.push(builtins);
    }
    if entries.is_empty() {
        return Err("source graph must declare functions".into());
    }

    let mut emitter = CEmitter::new(&modules[0].source, HashMap::new());
    for entry in &entries {
        emitter.source = &modules[entry.module].source;
        let signature = emitter
            .signature(entry.function)
            .map_err(|error| with_module_path(error, &modules[entry.module].path))?;
        if emitter.functions.contains_key(&signature.name) {
            return Err(format!(
                "function `{}` is declared in multiple modules",
                signature.name
            ));
        }
        emitter.functions.insert(signature.name.clone(), signature);
    }
    let paths = modules
        .iter()
        .enumerate()
        .map(|(index, module)| (module.path.clone(), index))
        .collect::<HashMap<_, _>>();
    let mut module_callables = vec![HashMap::<String, String>::new(); modules.len()];
    for entry in &entries {
        let name = entry
            .function
            .id
            .as_ref()
            .ok_or("anonymous function declaration")?
            .name
            .to_string();
        module_callables[entry.module].insert(name.clone(), name);
    }
    for (module_index, module) in modules.iter().enumerate() {
        for import in &module.local_imports {
            let target_index = *paths.get(&import.target).ok_or_else(|| {
                format!(
                    "internal error: unresolved module {}",
                    import.target.display()
                )
            })?;
            if !exports[target_index].contains(&import.imported) {
                return Err(format!(
                    "module {} does not export `{}`",
                    import.target.display(),
                    import.imported
                ));
            }
            if !emitter.functions.contains_key(&import.imported) {
                return Err(format!(
                    "imported function `{}` was not emitted",
                    import.imported
                ));
            }
            if module_callables[module_index]
                .insert(import.local.clone(), import.imported.clone())
                .is_some()
            {
                return Err(format!("duplicate local module binding `{}`", import.local));
            }
        }
        if module_builtins[module_index]
            .keys()
            .any(|name| module_callables[module_index].contains_key(name))
        {
            return Err("built-in import conflicts with a function binding".into());
        }
    }

    let root_has_main = entries.iter().any(|entry| {
        entry.module == 0
            && entry
                .function
                .id
                .as_ref()
                .is_some_and(|id| id.name.as_str() == "main")
    });
    if !root_has_main {
        return Err("entry module must declare `function main(): i32`".into());
    }

    let mut generated = String::new();
    generated.push_str(&emitter.preamble());
    for entry in &entries {
        emitter.source = &modules[entry.module].source;
        generated.push_str(
            &emitter
                .prototype(entry.function)
                .map_err(|error| with_module_path(error, &modules[entry.module].path))?,
        );
    }
    generated.push('\n');
    for entry in &entries {
        emitter.source = &modules[entry.module].source;
        emitter.builtins.clone_from(&module_builtins[entry.module]);
        emitter
            .call_targets
            .clone_from(&module_callables[entry.module]);
        generated.push_str(
            &emitter
                .function(entry.function)
                .map_err(|error| with_module_path(error, &modules[entry.module].path))?,
        );
    }
    generated.push_str(
        "\nint main(void) {\n\
         tsn_install_signals();\n\
         if (setjmp(tsn_trap_env) != 0) { tsn_trap_active = 0; tsn_shutdown_flag = 1; tsn_wait_workers(); tsn_cleanup_all(); fputs(\"error[TSN3002]: Uint8Array index out of bounds\\n\", stderr); return 1; }\n\
         tsn_trap_active = 1; int result = (int)tsn_main(); tsn_trap_active = 0; tsn_shutdown_flag = 1; tsn_wait_workers(); tsn_cleanup_all(); return result;\n}\n",
    );
    Ok(generated)
}

#[cfg(test)]
fn emit_c_source(source: &str, program: &oxc_ast::ast::Program<'_>) -> Result<String, String> {
    let mut functions = Vec::new();
    let mut builtins = HashMap::new();
    for statement in &program.body {
        match statement {
            Statement::ImportDeclaration(import) => {
                if import.import_kind == ImportOrExportKind::Type {
                    continue;
                }
                let module = import
                    .source
                    .value
                    .as_str()
                    .ok_or("standard-library module name must be valid UTF-8")?;
                let specifiers = import
                    .specifiers
                    .as_ref()
                    .ok_or("side-effect imports are unsupported")?;
                for specifier in specifiers {
                    let ImportDeclarationSpecifier::ImportSpecifier(specifier) = specifier else {
                        return Err("only named standard-library imports are supported".into());
                    };
                    if specifier.import_kind == ImportOrExportKind::Type {
                        continue;
                    }
                    let imported = match &specifier.imported {
                        ModuleExportName::IdentifierName(name) => name.name.as_str(),
                        ModuleExportName::StringLiteral(name) => name
                            .value
                            .as_str()
                            .ok_or("imported name is invalid UTF-8")?,
                        ModuleExportName::IdentifierReference(name) => name.name.as_str(),
                    };
                    let builtin = match (module, imported) {
                        ("tsn:time", "monotonicMs") => Builtin::MonotonicMs,
                        ("tsn:io", "printString") => Builtin::PrintString,
                        ("tsn:io", "printU32") => Builtin::PrintU32,
                        ("tsn:io", "printI32") => Builtin::PrintI32,
                        ("tsn:io", "printF64") => Builtin::PrintF64,
                        ("tsn:io", "readStdin") => Builtin::ReadStdin,
                        ("tsn:strings", "sliceUtf8") => Builtin::SliceUtf8,
                        ("tsn:strings", "stringByteAt") => Builtin::StringByteAt,
                        ("tsn:net", "listenTcp") => Builtin::ListenTcp,
                        ("tsn:net", "acceptTcp") => Builtin::AcceptTcp,
                        ("tsn:net", "readTcp") => Builtin::ReadTcp,
                        ("tsn:net", "writeTcp") => Builtin::WriteTcp,
                        ("tsn:net", "writeTcpString") => Builtin::WriteTcpString,
                        ("tsn:net", "closeTcp") => Builtin::CloseTcp,
                        ("tsn:os", "shutdownRequested") => Builtin::ShutdownRequested,
                        ("tsn:os", "spawnThread") => Builtin::SpawnThread,
                        ("tsn:bench", "printBenchmark") => Builtin::PrintBenchmark,
                        _ => {
                            return Err(format!(
                                "unknown standard-library import `{imported}` from `{module}`"
                            ));
                        }
                    };
                    let local_name = specifier.local.name.to_string();
                    if builtins.insert(local_name.clone(), builtin).is_some() {
                        return Err(format!("duplicate import binding `{local_name}`"));
                    }
                }
            }
            Statement::FunctionDeclaration(function) => {
                if function.r#async || function.generator {
                    return Err(
                        "async and generator functions are unsupported in this compiler slice"
                            .into(),
                    );
                }
                functions.push(function.as_ref());
            }
            Statement::ExportDeclaration(export) => {
                match &export.declaration {
                    Declaration::FunctionDeclaration(function) => {
                        if function.r#async || function.generator {
                            return Err("async and generator functions are unsupported in this compiler slice".into());
                        }
                        functions.push(function.as_ref());
                    }
                    _ => return Err("only function declarations may be exported".into()),
                }
            }
            _ => {
                return Err(
                    "only imports and top-level function declarations are supported".into(),
                );
            }
        }
    }
    if functions.is_empty() {
        return Err("source must declare functions".into());
    }

    let mut emitter = CEmitter::new(source, builtins);
    for function in &functions {
        let signature = emitter.signature(function)?;
        if emitter.builtins.contains_key(&signature.name) {
            return Err(format!(
                "function `{}` conflicts with an imported binding",
                signature.name
            ));
        }
        if emitter.functions.contains_key(&signature.name) {
            return Err(format!("duplicate function `{}`", signature.name));
        }
        emitter.functions.insert(signature.name.clone(), signature);
    }
    for function in &functions {
        let name = function
            .id
            .as_ref()
            .ok_or("anonymous function declaration")?
            .name
            .to_string();
        emitter.call_targets.insert(name.clone(), name);
    }
    if !emitter.functions.contains_key("main") {
        return Err("standalone programs must declare `function main(): i32`".into());
    }

    let mut generated = String::new();
    generated.push_str(&emitter.preamble());
    for function in &functions {
        generated.push_str(&emitter.prototype(function)?);
    }
    generated.push('\n');
    for function in &functions {
        generated.push_str(&emitter.function(function)?);
    }
    generated.push_str(
        "\nint main(void) {\n\
         tsn_install_signals();\n\
         if (setjmp(tsn_trap_env) != 0) { tsn_trap_active = 0; tsn_shutdown_flag = 1; tsn_wait_workers(); tsn_cleanup_all(); fputs(\"error[TSN3002]: Uint8Array index out of bounds\\n\", stderr); return 1; }\n\
         tsn_trap_active = 1; int result = (int)tsn_main(); tsn_trap_active = 0; tsn_shutdown_flag = 1; tsn_wait_workers(); tsn_cleanup_all(); return result;\n}\n",
    );
    Ok(generated)
}

struct CEmitter<'s> {
    source: &'s str,
    functions: HashMap<String, Signature>,
    builtins: HashMap<String, Builtin>,
    call_targets: HashMap<String, String>,
    scopes: Vec<HashMap<String, Local>>,
    owned_buffers: Vec<Vec<String>>,
    owned_words: Vec<Vec<String>>,
    loop_cleanup_bases: Vec<usize>,
    current_return: Option<CType>,
    return_temp_counter: u32,
}

struct Local {
    ty: CType,
    mutable: bool,
    buffer_len: Option<u32>,
}

#[derive(Clone)]
struct Signature {
    name: String,
    params: Vec<(String, CType)>,
    return_type: CType,
}

#[derive(Clone, Copy)]
enum Builtin {
    MonotonicMs,
    PrintString,
    PrintU32,
    PrintI32,
    PrintF64,
    ReadStdin,
    SliceUtf8,
    StringByteAt,
    ListenTcp,
    AcceptTcp,
    ReadTcp,
    WriteTcp,
    WriteTcpString,
    CloseTcp,
    ShutdownRequested,
    SpawnThread,
    PrintBenchmark,
}

impl<'s> CEmitter<'s> {
    fn new(source: &'s str, builtins: HashMap<String, Builtin>) -> Self {
        Self {
            source,
            functions: HashMap::new(),
            builtins,
            call_targets: HashMap::new(),
            scopes: Vec::new(),
            owned_buffers: Vec::new(),
            owned_words: Vec::new(),
            loop_cleanup_bases: Vec::new(),
            current_return: None,
            return_temp_counter: 0,
        }
    }

    fn local(&self, name: &str) -> Option<&Local> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }

    fn known_buffer_len(&self, expression: &Expression<'_>) -> Option<u32> {
        let Expression::NewExpression(new_expression) = expression else {
            return None;
        };
        let length = new_expression.arguments.first()?.as_expression()?;
        u32::try_from(self.integer_literal_value(length)?).ok()
    }

    fn define_local(&mut self, name: String, local: Local) -> Result<(), String> {
        let scope = self
            .scopes
            .last_mut()
            .ok_or("internal error: no active scope")?;
        if scope.contains_key(&name) {
            return Err(format!("duplicate local `{name}` in this scope"));
        }
        scope.insert(name, local);
        Ok(())
    }

    fn next_return_temp(&mut self) -> String {
        loop {
            let name = format!("tsn_return_tmp_{}", self.return_temp_counter);
            self.return_temp_counter = self.return_temp_counter.wrapping_add(1);
            if self.local(&name).is_none() {
                return name;
            }
        }
    }

    fn buffer_name(&self, expression: &Expression<'_>) -> Result<String, String> {
        let Expression::Identifier(identifier) = expression else {
            return Err("Uint8Array access requires a local buffer variable".into());
        };
        let name = identifier.name.as_str();
        match self.local(name) {
            Some(local) if local.ty == CType::U8Array => Ok(name.to_string()),
            Some(_) => Err(format!("`{name}` is not a Uint8Array")),
            None => Err(format!("unknown local `{name}`")),
        }
    }

    fn word_array_name(&self, expression: &Expression<'_>) -> Result<String, String> {
        let Expression::Identifier(identifier) = expression else {
            return Err("Uint32Array access requires a local array variable".into());
        };
        let name = identifier.name.as_str();
        match self.local(name) {
            Some(local) if local.ty == CType::U32Array => Ok(name.to_string()),
            Some(_) => Err(format!("`{name}` is not a Uint32Array")),
            None => Err(format!("unknown local `{name}`")),
        }
    }

    fn typed_array_type(&self, expression: &Expression<'_>) -> Result<CType, String> {
        let Expression::Identifier(identifier) = expression else {
            return Err("typed array access requires a local array variable".into());
        };
        let name = identifier.name.as_str();
        match self.local(name).map(|local| local.ty) {
            Some(ty @ (CType::U8Array | CType::U32Array)) => Ok(ty),
            Some(_) => Err(format!("`{name}` is not a typed array")),
            None => Err(format!("unknown local `{name}`")),
        }
    }

    fn builtin(&self, name: &str) -> Option<Builtin> {
        self.builtins.get(name).copied().or(match name {
            // Kept temporarily for the benchmark fixture; ordinary examples
            // should import the names from tsn:time / tsn:io / tsn:bench.
            "clockMs" => Some(Builtin::MonotonicMs),
            "printBenchmark" => Some(Builtin::PrintBenchmark),
            _ => None,
        })
    }

    fn cleanup_buffers(&self) -> String {
        let bytes = self
            .owned_buffers
            .iter()
            .rev()
            .flat_map(|scope| scope.iter().rev())
            .map(|name| format!("tsn_u8_array_free({name});\n"))
            .collect::<String>();
        let words = self
            .owned_words
            .iter()
            .rev()
            .flat_map(|scope| scope.iter().rev())
            .map(|name| format!("tsn_u32_array_free({name});\n"))
            .collect::<String>();
        format!("{words}{bytes}")
    }

    fn cleanup_current_scope(&self) -> String {
        let bytes = self
            .owned_buffers
            .last()
            .into_iter()
            .flatten()
            .rev()
            .map(|name| format!("tsn_u8_array_free({name});\n"))
            .collect::<String>();
        let words = self
            .owned_words
            .last()
            .into_iter()
            .flatten()
            .rev()
            .map(|name| format!("tsn_u32_array_free({name});\n"))
            .collect::<String>();
        format!("{words}{bytes}")
    }

    fn exit_with_cleanup(&self, cleanup: String, exit: String) -> String {
        if cleanup.is_empty() {
            format!("{exit}\n")
        } else {
            format!("{{\n{cleanup}{exit}\n}}\n")
        }
    }

    fn cleanup_loop_scopes(&self) -> Result<String, String> {
        let base = *self
            .loop_cleanup_bases
            .last()
            .ok_or("break/continue must be inside a loop")?;
        let bytes = self
            .owned_buffers
            .iter()
            .skip(base)
            .rev()
            .flat_map(|scope| scope.iter().rev())
            .map(|name| format!("tsn_u8_array_free({name});\n"))
            .collect::<String>();
        let words = self
            .owned_words
            .iter()
            .skip(base)
            .rev()
            .flat_map(|scope| scope.iter().rev())
            .map(|name| format!("tsn_u32_array_free({name});\n"))
            .collect::<String>();
        Ok(format!("{words}{bytes}"))
    }

    fn preamble(&self) -> String {
        "#include <stdint.h>\n#include <inttypes.h>\n#include <stdio.h>\n#include <time.h>\n#include <stdlib.h>\n#include <setjmp.h>\n#include <string.h>\n#include <errno.h>\n#include <signal.h>\n#include <pthread.h>\n#include <sys/types.h>\n#include <sys/socket.h>\n#include <netinet/in.h>\n#include <arpa/inet.h>\n#include <unistd.h>\n\
         _Static_assert(INT8_MIN == -128 && INT8_MAX == 127, \"tsn requires two's-complement i8\");\n\
         _Static_assert(INT16_MIN == -32768 && INT16_MAX == 32767, \"tsn requires two's-complement i16\");\n\
         static inline __attribute__((unused)) int8_t tsn_i8_wrap(int32_t value) { uint8_t bits = (uint8_t)value; int8_t result; memcpy(&result, &bits, sizeof(result)); return result; }\n\
         static inline __attribute__((unused)) int16_t tsn_i16_wrap(int32_t value) { uint16_t bits = (uint16_t)value; int16_t result; memcpy(&result, &bits, sizeof(result)); return result; }\n\
         static inline __attribute__((unused)) int32_t tsn_i32_from_bits(uint32_t bits) { int32_t result; memcpy(&result, &bits, sizeof(result)); return result; }\n\
         static inline __attribute__((unused)) int64_t tsn_i64_from_bits(uint64_t bits) { int64_t result; memcpy(&result, &bits, sizeof(result)); return result; }\n\
         static inline __attribute__((unused)) int8_t tsn_i8_post_inc(int8_t *value) { int8_t old = *value; *value = tsn_i8_wrap((int32_t)old + 1); return old; }\n\
         static inline __attribute__((unused)) int16_t tsn_i16_post_inc(int16_t *value) { int16_t old = *value; *value = tsn_i16_wrap((int32_t)old + 1); return old; }\n\
         static inline __attribute__((unused)) int8_t tsn_i8_post_dec(int8_t *value) { int8_t old = *value; *value = tsn_i8_wrap((int32_t)old - 1); return old; }\n\
         static inline __attribute__((unused)) int16_t tsn_i16_post_dec(int16_t *value) { int16_t old = *value; *value = tsn_i16_wrap((int32_t)old - 1); return old; }\n\
         typedef struct TsnByteArray { uint32_t len; uint8_t *data; struct TsnByteArray *next; } TsnByteArray;\n\
         typedef struct TsnWordArray { uint32_t len; uint32_t *data; struct TsnWordArray *next; } TsnWordArray;\n\
         typedef struct { uint32_t len; const uint8_t *data; } TsnString;\n\
         typedef struct TsnOwnedString { uint8_t *data; struct TsnOwnedString *next; } TsnOwnedString;\n\
         static _Thread_local jmp_buf tsn_trap_env; static _Thread_local int tsn_trap_active = 0; static _Thread_local TsnByteArray *tsn_live_buffers = NULL; static _Thread_local TsnWordArray *tsn_live_words = NULL; static _Thread_local TsnOwnedString *tsn_live_strings = NULL;\n\
         static volatile sig_atomic_t tsn_shutdown_flag = 0;\n\
         static inline __attribute__((unused)) void tsn_signal_handler(int signal_number) { (void)signal_number; tsn_shutdown_flag = 1; }\n\
         static inline __attribute__((unused)) void tsn_install_signals(void) { struct sigaction action; memset(&action, 0, sizeof(action)); action.sa_handler = tsn_signal_handler; sigemptyset(&action.sa_mask); action.sa_flags = 0; (void)sigaction(SIGINT, &action, NULL); (void)sigaction(SIGTERM, &action, NULL); }\n\
         static inline __attribute__((unused)) _Bool tsn_shutdown_requested(void) { return tsn_shutdown_flag != 0; }\n\
         typedef void (*TsnWorkerFn)(int32_t); typedef struct { TsnWorkerFn function; int32_t argument; } TsnWorkerArg;\n\
         static pthread_mutex_t tsn_workers_mutex = PTHREAD_MUTEX_INITIALIZER; static pthread_cond_t tsn_workers_cond = PTHREAD_COND_INITIALIZER; static uint32_t tsn_workers_active = 0;\n\
         static inline __attribute__((unused,noreturn)) void tsn_bounds_fail(void) { if (tsn_trap_active) longjmp(tsn_trap_env, 1); fputs(\"fatal TSN bounds trap outside main scope\\n\", stderr); abort(); }\n\
         static inline __attribute__((unused)) void tsn_cleanup_all(void) { while (tsn_live_buffers) { TsnByteArray *a = tsn_live_buffers; tsn_live_buffers = a->next; free(a->data); free(a); } while (tsn_live_words) { TsnWordArray *a = tsn_live_words; tsn_live_words = a->next; free(a->data); free(a); } while (tsn_live_strings) { TsnOwnedString *s = tsn_live_strings; tsn_live_strings = s->next; free(s->data); free(s); } }\n\
         static inline __attribute__((unused)) void *tsn_worker_start(void *opaque) { TsnWorkerArg *argument = (TsnWorkerArg *)opaque; TsnWorkerFn function = argument->function; int32_t value = argument->argument; free(argument); if (setjmp(tsn_trap_env) == 0) { tsn_trap_active = 1; function(value); } tsn_trap_active = 0; tsn_cleanup_all(); (void)pthread_mutex_lock(&tsn_workers_mutex); tsn_workers_active--; (void)pthread_cond_broadcast(&tsn_workers_cond); (void)pthread_mutex_unlock(&tsn_workers_mutex); return NULL; }\n\
         static inline __attribute__((unused)) int32_t tsn_spawn_thread(TsnWorkerFn function, int32_t argument) { (void)pthread_mutex_lock(&tsn_workers_mutex); if (tsn_workers_active >= 128 || tsn_shutdown_flag) { (void)pthread_mutex_unlock(&tsn_workers_mutex); return -1; } TsnWorkerArg *context = malloc(sizeof(*context)); if (!context) { (void)pthread_mutex_unlock(&tsn_workers_mutex); return -1; } context->function = function; context->argument = argument; pthread_t thread; tsn_workers_active++; int status = pthread_create(&thread, NULL, tsn_worker_start, context); if (status != 0) { tsn_workers_active--; free(context); (void)pthread_cond_broadcast(&tsn_workers_cond); (void)pthread_mutex_unlock(&tsn_workers_mutex); return -1; } (void)pthread_detach(thread); (void)pthread_mutex_unlock(&tsn_workers_mutex); return 0; }\n\
         static inline __attribute__((unused)) void tsn_wait_workers(void) { (void)pthread_mutex_lock(&tsn_workers_mutex); while (tsn_workers_active != 0) (void)pthread_cond_wait(&tsn_workers_cond, &tsn_workers_mutex); (void)pthread_mutex_unlock(&tsn_workers_mutex); }\n\
         static inline __attribute__((unused)) TsnByteArray *tsn_u8_array_new(uint32_t len) { TsnByteArray *a = malloc(sizeof(*a)); if (!a) abort(); a->len = len; a->data = calloc((size_t)len, sizeof(uint8_t)); if (len != 0 && !a->data) abort(); a->next = tsn_live_buffers; tsn_live_buffers = a; return a; }\n\
         static inline __attribute__((unused)) TsnWordArray *tsn_u32_array_new(uint32_t len) { TsnWordArray *a = malloc(sizeof(*a)); if (!a) abort(); a->len = len; a->data = calloc((size_t)len, sizeof(uint32_t)); if (len != 0 && !a->data) abort(); a->next = tsn_live_words; tsn_live_words = a; return a; }\n\
         static inline __attribute__((unused)) uint32_t tsn_u32_get(TsnWordArray *a, uint32_t i) { if (i >= a->len) tsn_bounds_fail(); return a->data[i]; }\n\
         static inline __attribute__((unused)) uint32_t tsn_u32_get_known(TsnWordArray *a, uint32_t i, uint32_t len) { if (i >= len) tsn_bounds_fail(); return a->data[i]; }\n\
         static inline __attribute__((unused)) void tsn_u32_set(TsnWordArray *a, uint32_t i, uint32_t v) { if (i >= a->len) tsn_bounds_fail(); a->data[i] = v; }\n\
         static inline __attribute__((unused)) void tsn_u32_array_free(TsnWordArray *a) { if (a) { TsnWordArray **link = &tsn_live_words; while (*link && *link != a) link = &(*link)->next; if (*link == a) *link = a->next; free(a->data); free(a); } }\n\
         static inline __attribute__((unused)) TsnString tsn_string_concat(TsnString a, TsnString b) { if (a.len > UINT32_MAX - b.len) abort(); uint32_t len = a.len + b.len; uint8_t *data = malloc(len ? (size_t)len : 1); if (!data) abort(); if (a.len) memcpy(data, a.data, a.len); if (b.len) memcpy(data + a.len, b.data, b.len); TsnOwnedString *owner = malloc(sizeof(*owner)); if (!owner) { free(data); abort(); } owner->data = data; owner->next = tsn_live_strings; tsn_live_strings = owner; return (TsnString){len, data}; }\n\
         static inline __attribute__((unused)) uint8_t tsn_u8_get(TsnByteArray *a, uint32_t i) { if (i >= a->len) tsn_bounds_fail(); return a->data[i]; }\n\
         static inline __attribute__((unused)) uint8_t tsn_u8_get_known(TsnByteArray *a, uint32_t i, uint32_t len) { if (i >= len) tsn_bounds_fail(); return a->data[i]; }\n\
         static inline __attribute__((unused)) void tsn_u8_set(TsnByteArray *a, uint32_t i, uint8_t v) { if (i >= a->len) tsn_bounds_fail(); a->data[i] = v; }\n\
         static inline __attribute__((unused)) void tsn_u8_array_free(TsnByteArray *a) { if (a) { TsnByteArray **link = &tsn_live_buffers; while (*link && *link != a) link = &(*link)->next; if (*link == a) *link = a->next; free(a->data); free(a); } }\n\
         static inline __attribute__((unused)) double tsn_clock_ms(void) { struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t); return (double)t.tv_sec * 1000.0 + (double)t.tv_nsec / 1e6; }\n\
         static inline __attribute__((unused)) void tsn_print_benchmark(uint32_t checksum, double elapsed_ms) { printf(\"{\\\"checksum\\\":%\" PRIu32 \",\\\"elapsedMs\\\":%.3f}\\n\", checksum, elapsed_ms); }\n\
         static inline __attribute__((unused)) void tsn_print_string(TsnString value) { fwrite(value.data, 1, value.len, stdout); fputc(10, stdout); }\n\
         static inline __attribute__((unused)) uint32_t tsn_read_stdin(TsnByteArray *buffer) { if (buffer->len == 0) return 0; size_t count = fread(buffer->data, 1, (size_t)buffer->len, stdin); if (count == 0 && ferror(stdin)) return UINT32_MAX; return (uint32_t)count; }\n\
         static inline __attribute__((unused)) int32_t tsn_listen_tcp(uint16_t port, _Bool loopback) { int fd = socket(AF_INET, SOCK_STREAM, 0); if (fd < 0) return -1; int yes = 1; if (setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &yes, (socklen_t)sizeof(yes)) < 0) { (void)close(fd); return -1; } struct sockaddr_in address; memset(&address, 0, sizeof(address)); address.sin_family = AF_INET; address.sin_addr.s_addr = htonl(loopback ? INADDR_LOOPBACK : INADDR_ANY); address.sin_port = htons(port); if (bind(fd, (struct sockaddr *)&address, (socklen_t)sizeof(address)) < 0 || listen(fd, 128) < 0) { (void)close(fd); return -1; } return (int32_t)fd; }\n\
         static inline __attribute__((unused)) int32_t tsn_accept_tcp(int32_t listener) { int fd; do { fd = accept(listener, NULL, NULL); } while (fd < 0 && errno == EINTR && !tsn_shutdown_flag); return (int32_t)fd; }\n\
         static inline __attribute__((unused)) uint32_t tsn_read_tcp(int32_t fd, TsnByteArray *buffer, uint32_t offset) { if (offset > buffer->len) return UINT32_MAX; if (offset == buffer->len) return 0; size_t len = (size_t)(buffer->len - offset); if (len > (size_t)INT32_MAX) len = (size_t)INT32_MAX; ssize_t count; do { count = read(fd, buffer->data + offset, len); } while (count < 0 && errno == EINTR && !tsn_shutdown_flag); if (count < 0 && errno == EINTR && tsn_shutdown_flag) return 0; if (count < 0) return UINT32_MAX; return (uint32_t)count; }\n\
         static inline __attribute__((unused)) uint32_t tsn_write_tcp(int32_t fd, TsnByteArray *buffer, uint32_t offset) { if (offset > buffer->len) return UINT32_MAX; size_t len = (size_t)(buffer->len - offset); if (len == 0) return 0; ssize_t count; do { count = send(fd, buffer->data + offset, len, MSG_NOSIGNAL); } while (count < 0 && errno == EINTR); if (count < 0) return UINT32_MAX; return (uint32_t)count; }\n\
         static inline __attribute__((unused)) uint32_t tsn_write_tcp_string(int32_t fd, TsnString value, uint32_t offset) { if (offset > value.len) return UINT32_MAX; size_t len = (size_t)(value.len - offset); if (len == 0) return 0; ssize_t count; do { count = send(fd, value.data + offset, len, MSG_NOSIGNAL); } while (count < 0 && errno == EINTR); if (count < 0) return UINT32_MAX; return (uint32_t)count; }\n\
         static inline __attribute__((unused)) int32_t tsn_close_tcp(int32_t fd) { return (int32_t)close(fd); }\n\
         static inline __attribute__((unused)) void tsn_print_u32(uint32_t value) { printf(\"%\" PRIu32 \"\\n\", value); }\n\
         static inline __attribute__((unused)) void tsn_print_i32(int32_t value) { printf(\"%\" PRId32 \"\\n\", value); }\n\
         static inline __attribute__((unused)) void tsn_print_f64(double value) { printf(\"%.17g\\n\", value); }\n\
         static inline __attribute__((unused)) _Bool tsn_string_equal(TsnString a, TsnString b) { return a.len == b.len && memcmp(a.data, b.data, a.len) == 0; }\n\
         static inline __attribute__((unused)) TsnString tsn_string_slice_utf8(TsnString value, uint32_t start, uint32_t end) { if (start > end || end > value.len) tsn_bounds_fail(); return (TsnString){end - start, value.data + start}; }\n\
         static inline __attribute__((unused)) uint8_t tsn_string_byte_at(TsnString value, uint32_t index) { if (index >= value.len) tsn_bounds_fail(); return value.data[index]; }\n\n"
            .to_string()
    }

    fn function_name(&self, function: &Function<'_>) -> Result<String, String> {
        Ok(function
            .id
            .as_ref()
            .ok_or("anonymous functions are unsupported")?
            .name
            .to_string())
    }

    fn signature(&self, function: &Function<'_>) -> Result<Signature, String> {
        let name = self.function_name(function)?;
        let return_type = parse_annotated_type(
            self.source,
            function.return_type.as_deref(),
            function.span,
            "function return type",
        )?;
        if matches!(return_type, CType::U8Array | CType::U32Array) {
            return Err("returning owned typed arrays is not supported yet".into());
        }
        let params = function
            .params
            .items
            .iter()
            .map(|parameter| {
                let id = match &parameter.pattern {
                    oxc_ast::ast::BindingPattern::BindingIdentifier(id) => id,
                    _ => return Err("destructured parameters are unsupported".to_string()),
                };
                let ty = parse_annotated_type(
                    self.source,
                    parameter.type_annotation.as_deref(),
                    parameter.span,
                    "parameter",
                )?;
                Ok((id.name.to_string(), ty))
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Signature {
            name,
            params,
            return_type,
        })
    }

    fn prototype(&self, function: &Function<'_>) -> Result<String, String> {
        let signature = self.signature(function)?;
        if signature.name == "main"
            && (signature.return_type != CType::I32 || !signature.params.is_empty())
        {
            return Err("entry point must have signature `function main(): i32`".into());
        }
        let params = if signature.params.is_empty() {
            "void".to_string()
        } else {
            signature
                .params
                .iter()
                .map(|(name, ty)| format!("{} {name}", ty.c_name()))
                .collect::<Vec<_>>()
                .join(", ")
        };
        Ok(format!(
            "{} tsn_{}({params});\n",
            signature.return_type.c_name(),
            signature.name
        ))
    }

    fn function(&mut self, function: &Function<'_>) -> Result<String, String> {
        let signature = self.signature(function)?;
        self.scopes.clear();
        self.scopes.push(HashMap::new());
        self.owned_buffers.clear();
        self.owned_buffers.push(Vec::new());
        self.owned_words.clear();
        self.owned_words.push(Vec::new());
        self.loop_cleanup_bases.clear();
        self.current_return = Some(signature.return_type);
        self.return_temp_counter = 0;
        let params_c = signature
            .params
            .iter()
            .map(|(name, ty)| {
                self.define_local(
                    name.clone(),
                    Local {
                        ty: *ty,
                        mutable: true,
                        buffer_len: None,
                    },
                )?;
                Ok(format!("{} {name}", ty.c_name()))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let body = function.body.as_ref().ok_or("function body is required")?;
        if signature.return_type != CType::Void
            && !body.statements.iter().any(statement_always_returns)
        {
            return Err(marked_type_error(
                Some(body.span),
                format!(
                    "function `{}` may complete without returning a value",
                    signature.name
                ),
            ));
        }
        let statements = body
            .statements
            .iter()
            .map(|statement| self.statement(statement))
            .collect::<Result<String, _>>()?;
        let normal_cleanup = self.cleanup_current_scope();
        let params_c = if params_c.is_empty() {
            "void".to_string()
        } else {
            params_c.join(", ")
        };
        Ok(format!(
            "{} tsn_{}({params_c}) {{\n{statements}{normal_cleanup}}}\n",
            signature.return_type.c_name(),
            signature.name
        ))
    }

    fn statement(&mut self, statement: &Statement<'_>) -> Result<String, String> {
        match statement {
            Statement::BlockStatement(block) => {
                self.scopes.push(HashMap::new());
                self.owned_buffers.push(Vec::new());
                self.owned_words.push(Vec::new());
                let inner = block
                    .body
                    .iter()
                    .map(|s| self.statement(s))
                    .collect::<Result<String, _>>()?;
                let cleanup = self.cleanup_current_scope();
                self.scopes.pop();
                self.owned_buffers.pop();
                self.owned_words.pop();
                Ok(format!("{{\n{inner}{cleanup}}}\n"))
            }
            Statement::VariableDeclaration(declaration) => self.variable_declaration(declaration),
            Statement::ForStatement(for_statement) => {
                self.scopes.push(HashMap::new());
                self.owned_buffers.push(Vec::new());
                self.owned_words.push(Vec::new());
                let init = match for_statement
                    .init
                    .as_ref()
                    .ok_or("for-loop initializer is required")?
                {
                    ForStatementInit::VariableDeclaration(declaration) => {
                        if declaration.declarations.iter().any(|declarator| {
                            declarator
                                .type_annotation
                                .as_deref()
                                .is_some_and(|annotation| {
                                    let source = &self.source[annotation.span.start as usize
                                        ..annotation.span.end as usize];
                                    matches!(
                                        source.trim().trim_start_matches(':').trim(),
                                        "Uint8Array" | "Uint32Array"
                                    )
                                })
                        }) {
                            return Err(
                                "Uint8Array declaration in a for initializer is unsupported".into(),
                            );
                        }
                        self.variable_declaration(declaration)?
                    }
                    _ => return Err("for initializer must be a variable declaration".into()),
                };
                let condition = self.condition(
                    for_statement
                        .test
                        .as_ref()
                        .ok_or("for-loop condition is required")?,
                )?;
                let update = self.expression(
                    for_statement
                        .update
                        .as_ref()
                        .ok_or("for-loop update is required")?,
                )?;
                self.loop_cleanup_bases.push(self.owned_buffers.len());
                let body = self.statement(&for_statement.body)?;
                self.loop_cleanup_bases.pop();
                let cleanup = self.cleanup_current_scope();
                self.scopes.pop();
                self.owned_buffers.pop();
                self.owned_words.pop();
                Ok(format!(
                    "for ({init} {condition}; {update}) {body}{cleanup}"
                ))
            }
            Statement::WhileStatement(while_statement) => {
                let condition = self.condition(&while_statement.test)?;
                self.loop_cleanup_bases.push(self.owned_buffers.len());
                let body = self.statement(&while_statement.body)?;
                self.loop_cleanup_bases.pop();
                Ok(format!("while ({condition}) {body}"))
            }
            Statement::DoWhileStatement(do_while) => {
                self.loop_cleanup_bases.push(self.owned_buffers.len());
                let body = self.statement(&do_while.body)?;
                self.loop_cleanup_bases.pop();
                let condition = self.condition(&do_while.test)?;
                Ok(format!("do {body} while ({condition});\n"))
            }
            Statement::IfStatement(if_statement) => {
                let condition = self.condition(&if_statement.test)?;
                let consequent = self.statement(&if_statement.consequent)?;
                let alternate = if let Some(alternate) = &if_statement.alternate {
                    format!(" else {}", self.statement(alternate)?)
                } else {
                    String::new()
                };
                Ok(format!("if ({condition}) {consequent}{alternate}"))
            }
            Statement::ReturnStatement(ret) => match (self.current_return, ret.argument.as_ref()) {
                (Some(CType::Void), None) => {
                    Ok(self.exit_with_cleanup(self.cleanup_buffers(), "return;".into()))
                }
                (Some(CType::Void), Some(_)) => Err("void function cannot return a value".into()),
                (Some(expected), Some(expr)) => {
                    self.ensure_assignable(expr, expected, "return value")?;
                    let value = self.expression(expr)?;
                    let temporary = self.next_return_temp();
                    let cleanup = self.cleanup_buffers();
                    Ok(format!(
                        "{{\n{} {temporary} = {value};\n{cleanup}return {temporary};\n}}\n",
                        expected.c_name()
                    ))
                }
                (Some(_), None) => Err("non-void function must return a value".into()),
                (None, _) => Err("internal error: no active function".into()),
            },
            Statement::BreakStatement(statement) => {
                if statement.label.is_some() {
                    return Err("labeled break is not supported".into());
                }
                Ok(self.exit_with_cleanup(self.cleanup_loop_scopes()?, "break;".into()))
            }
            Statement::ContinueStatement(statement) => {
                if statement.label.is_some() {
                    return Err("labeled continue is not supported".into());
                }
                Ok(self.exit_with_cleanup(self.cleanup_loop_scopes()?, "continue;".into()))
            }
            Statement::ExpressionStatement(expr) => {
                Ok(format!("{};\n", self.expression(&expr.expression)?))
            }
            _ => Err(format!("unsupported statement in M1: {statement:?}")),
        }
    }

    fn variable_declaration(
        &mut self,
        declaration: &oxc_ast::ast::VariableDeclaration<'_>,
    ) -> Result<String, String> {
        let mut output = String::new();
        for declarator in &declaration.declarations {
            let id = match &declarator.id {
                oxc_ast::ast::BindingPattern::BindingIdentifier(id) => id,
                _ => return Err("destructured locals are unsupported".into()),
            };
            let ty = parse_annotated_type(
                self.source,
                declarator.type_annotation.as_deref(),
                declarator.span,
                "local variable",
            )?;
            if ty == CType::Void {
                return Err("local variable cannot have type void".into());
            }
            if matches!(ty, CType::U8Array | CType::U32Array) && self.owned_buffers.is_empty() {
                return Err("internal error: typed array declared outside a cleanup scope".into());
            }
            if matches!(ty, CType::U8Array | CType::U32Array)
                && !matches!(declarator.init.as_ref(), Some(Expression::NewExpression(_)))
            {
                return Err(
                    "typed array aliasing is unsupported until ownership transfer is defined"
                        .into(),
                );
            }
            let init = self.expression(
                declarator
                    .init
                    .as_ref()
                    .ok_or("initialized locals are required")?,
            )?;
            let name = id.name.to_string();
            self.ensure_assignable(
                declarator
                    .init
                    .as_ref()
                    .ok_or("initialized locals are required")?,
                ty,
                &format!("initializer of `{name}`"),
            )?;
            let mutable = match declaration.kind {
                VariableDeclarationKind::Let => true,
                VariableDeclarationKind::Const => false,
                _ => return Err("only `let` and `const` declarations are supported".into()),
            };
            let buffer_len = declarator
                .init
                .as_ref()
                .and_then(|init| self.known_buffer_len(init));
            self.define_local(
                name.clone(),
                Local {
                    ty,
                    mutable,
                    buffer_len,
                },
            )?;
            if ty == CType::U8Array {
                self.owned_buffers
                    .last_mut()
                    .ok_or("internal error: missing cleanup scope")?
                    .push(name.clone());
            } else if ty == CType::U32Array {
                self.owned_words
                    .last_mut()
                    .ok_or("internal error: missing cleanup scope")?
                    .push(name.clone());
            }
            let const_qualifier = if mutable || matches!(ty, CType::U8Array | CType::U32Array) {
                ""
            } else {
                "const "
            };
            output.push_str(&format!(
                "{const_qualifier}{} {name} = {init};\n",
                ty.c_name()
            ));
        }
        Ok(output)
    }

    fn ensure_assignable(
        &self,
        expression: &Expression<'_>,
        expected: CType,
        context: &str,
    ) -> Result<(), String> {
        let actual = self.infer_expression_type(expression)?;
        if actual == CType::IntLiteral {
            if expected.is_integer() && !self.integer_literal_fits(expression, expected) {
                return Err(format!(
                    "integer literal is outside the range of `{expected:?}` in {context}"
                ));
            }
            if expected == CType::F32 && !self.integer_literal_exact(expression, 24) {
                return Err(format!(
                    "integer literal is not exactly representable as f32 in {context}"
                ));
            }
            if expected == CType::F64 && !self.integer_literal_exact(expression, 53) {
                return Err(format!(
                    "integer literal is not exactly representable as f64 in {context}"
                ));
            }
        }
        if Self::can_assign(actual, expected) {
            Ok(())
        } else {
            Err(marked_type_error(
                Some(expression.span()),
                format!("type mismatch in {context}: expected `{expected:?}`, got `{actual:?}`"),
            ))
        }
    }

    fn integer_literal_fits(&self, expression: &Expression<'_>, expected: CType) -> bool {
        let Some(value) = self.integer_literal_value(expression) else {
            return false;
        };
        let width = match expected {
            CType::U8 | CType::I8 => 8,
            CType::U16 | CType::I16 => 16,
            CType::U32 | CType::I32 => 32,
            CType::U64 | CType::I64 => 64,
            _ => return false,
        };
        if matches!(expected, CType::I8 | CType::I16 | CType::I32 | CType::I64) {
            let min = -(1_i128 << (width - 1));
            let max = (1_i128 << (width - 1)) - 1;
            value >= min && value <= max
        } else {
            value >= 0 && (value as u128) < (1_u128 << width)
        }
    }

    fn integer_literal_exact(&self, expression: &Expression<'_>, precision: u32) -> bool {
        let Some(value) = self.integer_literal_value(expression) else {
            return false;
        };
        let magnitude = value.unsigned_abs();
        if magnitude == 0 {
            return true;
        }
        let significant_bits = 128 - magnitude.leading_zeros() - magnitude.trailing_zeros();
        significant_bits <= precision
    }

    fn integer_literal_value(&self, expression: &Expression<'_>) -> Option<i128> {
        match expression {
            Expression::NumericLiteral(literal) => self
                .parse_unsigned_literal(literal)
                .and_then(|v| i128::try_from(v).ok()),
            Expression::UnaryExpression(unary) if unary.operator == UnaryOperator::UnaryPlus => {
                self.integer_literal_value(&unary.argument)
            }
            Expression::UnaryExpression(unary)
                if unary.operator == UnaryOperator::UnaryNegation =>
            {
                let Expression::NumericLiteral(literal) = &unary.argument else {
                    return None;
                };
                let magnitude = self.parse_unsigned_literal(literal)?;
                if magnitude == (1_u128 << 127) {
                    Some(i128::MIN)
                } else {
                    i128::try_from(magnitude).ok()?.checked_neg()
                }
            }
            _ => None,
        }
    }

    fn parse_unsigned_literal(&self, literal: &oxc_ast::ast::NumericLiteral<'_>) -> Option<u128> {
        let raw =
            self.source[literal.span.start as usize..literal.span.end as usize].replace('_', "");
        if let Some(hex) = raw.strip_prefix("0x").or_else(|| raw.strip_prefix("0X")) {
            u128::from_str_radix(hex, 16).ok()
        } else if let Some(bin) = raw.strip_prefix("0b").or_else(|| raw.strip_prefix("0B")) {
            u128::from_str_radix(bin, 2).ok()
        } else if let Some(oct) = raw.strip_prefix("0o").or_else(|| raw.strip_prefix("0O")) {
            u128::from_str_radix(oct, 8).ok()
        } else {
            raw.parse::<u128>().ok()
        }
    }

    fn can_assign(actual: CType, expected: CType) -> bool {
        if actual == expected {
            return true;
        }
        if actual == CType::IntLiteral {
            return expected.is_integer() || matches!(expected, CType::F32 | CType::F64);
        }
        if actual == CType::FloatLiteral {
            return matches!(expected, CType::F32 | CType::F64);
        }
        matches!(
            (actual, expected),
            (CType::U8, CType::U16 | CType::U32 | CType::U64)
                | (CType::U16, CType::U32 | CType::U64)
                | (CType::U32, CType::U64)
                | (CType::I8, CType::I16 | CType::I32 | CType::I64)
                | (CType::I16, CType::I32 | CType::I64)
                | (CType::I32, CType::I64)
                | (CType::U8, CType::I16 | CType::I32 | CType::I64)
                | (CType::U16, CType::I32 | CType::I64)
                | (CType::U32, CType::I64)
                | (CType::U8 | CType::U16 | CType::I8 | CType::I16, CType::F32)
                | (
                    CType::U8 | CType::U16 | CType::U32 | CType::I8 | CType::I16 | CType::I32,
                    CType::F64
                )
                | (CType::F32, CType::F64)
        )
    }

    fn infer_expression_type(&self, expression: &Expression<'_>) -> Result<CType, String> {
        self.infer_expression_type_inner(expression)
            .map_err(|message| {
                if message.starts_with(DIAGNOSTIC_MARKER) {
                    message
                } else {
                    marked_type_error(Some(expression.span()), message)
                }
            })
    }

    fn infer_expression_type_inner(&self, expression: &Expression<'_>) -> Result<CType, String> {
        match expression {
            Expression::NumericLiteral(literal) => {
                let raw = &self.source[literal.span.start as usize..literal.span.end as usize];
                let is_prefixed_integer = raw.starts_with("0x")
                    || raw.starts_with("0X")
                    || raw.starts_with("0b")
                    || raw.starts_with("0B")
                    || raw.starts_with("0o")
                    || raw.starts_with("0O");
                if !is_prefixed_integer
                    && (raw.contains('.') || raw.contains('e') || raw.contains('E'))
                {
                    Ok(CType::FloatLiteral)
                } else {
                    Ok(CType::IntLiteral)
                }
            }
            Expression::BooleanLiteral(_) => Ok(CType::Bool),
            Expression::StringLiteral(literal) => {
                let value = literal.value.as_str().ok_or_else(|| {
                    marked_type_error(
                        Some(literal.span),
                        "string contains a lone UTF-16 surrogate",
                    )
                })?;
                if value.len() > u32::MAX as usize {
                    return Err(marked_type_error(
                        Some(literal.span),
                        "string literal exceeds the u32 byte-length limit",
                    ));
                }
                Ok(CType::String)
            }
            Expression::Identifier(identifier) => {
                let name = identifier.name.as_str();
                self.local(name)
                    .map(|local| local.ty)
                    .ok_or_else(|| format!("unknown local `{name}`"))
            }
            Expression::ParenthesizedExpression(paren) => {
                self.infer_expression_type(&paren.expression)
            }
            Expression::TSAsExpression(cast) => {
                let target = CType::parse(
                    self.source,
                    Some(cast.type_annotation.span()),
                    "cast target",
                )?;
                let source = self.infer_expression_type(&cast.expression)?;
                if !source.is_integer() || !target.is_integer() {
                    return Err(marked_type_error(
                        Some(cast.span),
                        "explicit `as` casts currently require integer source and target types",
                    ));
                }
                Ok(target)
            }
            Expression::BinaryExpression(binary) => {
                if matches!(
                    binary.operator,
                    BinaryOperator::Equality | BinaryOperator::Inequality
                ) {
                    return Err(marked_diagnostic(
                        "TSN1301",
                        Some(binary.span),
                        "loose equality is not supported; use `===` or `!==`",
                    ));
                }
                let left = self.infer_expression_type(&binary.left)?;
                let right = self.infer_expression_type(&binary.right)?;
                match binary.operator {
                    BinaryOperator::Division => {
                        self.unify_numeric(left, right, false)?;
                        Ok(CType::F64)
                    }
                    BinaryOperator::Addition
                    | BinaryOperator::Subtraction
                    | BinaryOperator::Multiplication => {
                        if binary.operator == BinaryOperator::Addition
                            && (left == CType::String || right == CType::String)
                        {
                            if left == CType::String && right == CType::String {
                                Ok(CType::String)
                            } else {
                                Err(marked_type_error(
                                    Some(binary.span),
                                    "string concatenation requires two string operands; implicit conversion is not supported",
                                ))
                            }
                        } else {
                            self.unify_numeric(left, right, binary.operator.is_bitwise())
                        }
                    }
                    BinaryOperator::BitwiseAnd
                    | BinaryOperator::BitwiseOR
                    | BinaryOperator::BitwiseXOR => self.unify_integer(left, right),
                    BinaryOperator::ShiftRightZeroFill | BinaryOperator::ShiftLeft => {
                        if !left.is_integer() || !right.is_integer() {
                            return Err("bit shifts require integer operands".into());
                        }
                        if !matches!(left, CType::U32 | CType::U64) {
                            return Err(
                                "shift operations currently require a u32 or u64 left operand"
                                    .into(),
                            );
                        }
                        Ok(left)
                    }
                    BinaryOperator::LessThan
                    | BinaryOperator::LessEqualThan
                    | BinaryOperator::GreaterThan
                    | BinaryOperator::GreaterEqualThan
                    | BinaryOperator::StrictEquality
                    | BinaryOperator::StrictInequality => {
                        let unified = self.unify_comparable(left, right)?;
                        if unified == CType::U8Array {
                            return Err(marked_type_error(
                                Some(binary.span),
                                "equality on Uint8Array values is not implemented in this compiler slice",
                            ));
                        }
                        if unified == CType::String
                            && matches!(
                                binary.operator,
                                BinaryOperator::LessThan
                                    | BinaryOperator::LessEqualThan
                                    | BinaryOperator::GreaterThan
                                    | BinaryOperator::GreaterEqualThan
                            )
                        {
                            return Err(marked_type_error(
                                Some(binary.span),
                                "relational comparisons on strings are not implemented",
                            ));
                        }
                        if matches!(
                            binary.operator,
                            BinaryOperator::LessThan
                                | BinaryOperator::LessEqualThan
                                | BinaryOperator::GreaterThan
                                | BinaryOperator::GreaterEqualThan
                        ) && !unified.is_numeric()
                        {
                            return Err("relational comparisons require numeric operands".into());
                        }
                        Ok(CType::Bool)
                    }
                    _ => Err(format!("unsupported binary operator {:?}", binary.operator)),
                }
            }
            Expression::LogicalExpression(logical) => {
                let left = self.infer_expression_type(&logical.left)?;
                let right = self.infer_expression_type(&logical.right)?;
                if left != CType::Bool || right != CType::Bool {
                    return Err("logical operators require boolean operands".into());
                }
                Ok(CType::Bool)
            }
            Expression::ConditionalExpression(conditional) => {
                let test_type = self.infer_expression_type(&conditional.test)?;
                if test_type != CType::Bool {
                    return Err(marked_type_error(
                        Some(conditional.test.span()),
                        "conditional expression test must be boolean",
                    ));
                }
                let consequent = self.infer_expression_type(&conditional.consequent)?;
                let alternate = self.infer_expression_type(&conditional.alternate)?;
                let result = self.unify_comparable(consequent, alternate)?;
                if matches!(result, CType::U8Array | CType::U32Array) {
                    return Err(marked_type_error(
                        Some(conditional.span),
                        "conditional expressions cannot select owned typed arrays",
                    ));
                }
                Ok(result)
            }
            Expression::UnaryExpression(unary) => {
                let argument_type = self.infer_expression_type(&unary.argument)?;
                match unary.operator {
                    UnaryOperator::LogicalNot if argument_type == CType::Bool => Ok(CType::Bool),
                    UnaryOperator::LogicalNot => {
                        Err("logical not requires a boolean operand".into())
                    }
                    UnaryOperator::UnaryPlus | UnaryOperator::UnaryNegation => {
                        if !argument_type.is_numeric() {
                            return Err("unary arithmetic requires a numeric operand".into());
                        }
                        if unary.operator == UnaryOperator::UnaryNegation
                            && matches!(
                                argument_type,
                                CType::U8 | CType::U16 | CType::U32 | CType::U64
                            )
                        {
                            return Err("cannot negate an unsigned value".into());
                        }
                        Ok(argument_type)
                    }
                    UnaryOperator::BitwiseNot if argument_type == CType::IntLiteral => {
                        Ok(CType::I32)
                    }
                    UnaryOperator::BitwiseNot if argument_type.is_integer() => Ok(argument_type),
                    UnaryOperator::BitwiseNot => Err("bitwise not requires an integer".into()),
                    _ => Err(format!(
                        "unary operator {:?} is unsupported",
                        unary.operator
                    )),
                }
            }
            Expression::NewExpression(new_expression) => {
                let Expression::Identifier(callee) = &new_expression.callee else {
                    return Err("only fixed-width typed array allocation is supported".into());
                };
                let array_type = match callee.name.as_str() {
                    "Uint8Array" => CType::U8Array,
                    "Uint32Array" => CType::U32Array,
                    _ => {
                        return Err(
                            "only Uint8Array and Uint32Array allocation is supported".into()
                        );
                    }
                };
                if new_expression.arguments.len() != 1 {
                    return Err("typed array allocation expects one length".into());
                }
                let length = new_expression.arguments[0]
                    .as_expression()
                    .ok_or("spread allocation arguments are unsupported")?;
                let length_type = self.infer_expression_type(length)?;
                if !Self::can_assign(length_type, CType::U32) {
                    return Err(marked_type_error(
                        Some(length.span()),
                        "Uint8Array length must be an integer no wider than u32",
                    ));
                }
                self.ensure_assignable(length, CType::U32, "Uint8Array length")?;
                Ok(array_type)
            }
            Expression::ComputedMemberExpression(member) => {
                let array_type = self.typed_array_type(&member.object)?;
                let index_type = self.infer_expression_type(&member.expression)?;
                if !Self::can_assign(index_type, CType::U32) {
                    return Err(marked_type_error(
                        Some(member.expression.span()),
                        "typed array index must be an integer no wider than u32",
                    ));
                }
                self.ensure_assignable(&member.expression, CType::U32, "typed array index")?;
                Ok(if array_type == CType::U8Array {
                    CType::U8
                } else {
                    CType::U32
                })
            }
            Expression::StaticMemberExpression(member) => {
                if member.property.name.as_str() != "length" {
                    return Err("only `.length` is supported on strings and Uint8Array".into());
                }
                let Expression::Identifier(identifier) = &member.object else {
                    return Err("`.length` requires a local string or Uint8Array".into());
                };
                let name = identifier.name.as_str();
                match self.local(name).map(|local| local.ty) {
                    Some(CType::String | CType::U8Array | CType::U32Array) => Ok(CType::U32),
                    Some(_) => Err(format!("`{name}` has no `.length` property")),
                    None => Err(format!("unknown local `{name}`")),
                }
            }
            Expression::CallExpression(call) => self.call_type(call),
            Expression::AssignmentExpression(assign) => {
                let target_type = match &assign.left {
                    AssignmentTarget::AssignmentTargetIdentifier(id) => {
                        let name = id.name.as_str();
                        let local = self
                            .local(name)
                            .ok_or_else(|| format!("unknown local `{name}`"))?;
                        if !local.mutable {
                            return Err(format!("cannot assign to immutable local `{name}`"));
                        }
                        if matches!(local.ty, CType::U8Array | CType::U32Array) {
                            return Err(marked_type_error(
                                Some(assign.span),
                                "typed array rebinding is unsupported until ownership transfer is defined",
                            ));
                        }
                        local.ty
                    }
                    AssignmentTarget::ComputedMemberExpression(member) => {
                        let array_type = self.typed_array_type(&member.object)?;
                        let index_type = self.infer_expression_type(&member.expression)?;
                        if !Self::can_assign(index_type, CType::U32) {
                            return Err(marked_type_error(
                                Some(member.expression.span()),
                                "typed array index must be an integer no wider than u32",
                            ));
                        }
                        self.ensure_assignable(
                            &member.expression,
                            CType::U32,
                            "typed array index",
                        )?;
                        if array_type == CType::U8Array {
                            CType::U8
                        } else {
                            CType::U32
                        }
                    }
                    _ => return Err("assignment target is unsupported".into()),
                };
                if assign.operator != oxc_syntax::operator::AssignmentOperator::Assign {
                    return Err("compound assignment is unsupported".into());
                }
                self.ensure_assignable(&assign.right, target_type, "assignment")?;
                Ok(target_type)
            }
            Expression::UpdateExpression(update) => {
                let oxc_ast::ast::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) =
                    &update.argument
                else {
                    return Err("only local-variable increment is supported".into());
                };
                let name = id.name.as_str();
                let local = self
                    .local(name)
                    .ok_or_else(|| format!("unknown local `{name}`"))?;
                if !local.mutable {
                    return Err(format!("cannot update immutable local `{name}`"));
                }
                if !local.ty.is_integer() {
                    return Err(format!("increment requires an integer local, got `{name}`"));
                }
                Ok(local.ty)
            }
            _ => Err(format!("unsupported expression in M1: {expression:?}")),
        }
    }

    fn unify_numeric(
        &self,
        left: CType,
        right: CType,
        integer_only: bool,
    ) -> Result<CType, String> {
        if integer_only && (!left.is_integer() || !right.is_integer()) {
            return Err("bitwise operators require integer operands".into());
        }
        if !left.is_numeric() || !right.is_numeric() {
            return Err("arithmetic operators require numeric operands".into());
        }
        self.unify_comparable(left, right)
    }

    fn unify_integer(&self, left: CType, right: CType) -> Result<CType, String> {
        if !left.is_integer() || !right.is_integer() {
            return Err("bitwise operators require integer operands".into());
        }
        self.unify_comparable(left, right)
    }

    fn unify_comparable(&self, left: CType, right: CType) -> Result<CType, String> {
        if matches!(left, CType::IntLiteral | CType::FloatLiteral)
            && matches!(right, CType::IntLiteral | CType::FloatLiteral)
        {
            return Ok(
                if left == CType::FloatLiteral || right == CType::FloatLiteral {
                    CType::F64
                } else {
                    CType::I32
                },
            );
        }
        if left == right {
            return Ok(left);
        }
        if Self::can_assign(left, right) {
            return Ok(right);
        }
        if Self::can_assign(right, left) {
            return Ok(left);
        }
        Err(format!(
            "incompatible operand types `{left:?}` and `{right:?}`"
        ))
    }

    fn call_type(&self, call: &oxc_ast::ast::CallExpression<'_>) -> Result<CType, String> {
        if let Expression::StaticMemberExpression(member) = &call.callee {
            let is_math =
                matches!(&member.object, Expression::Identifier(id) if id.name.as_str() == "Math");
            if !is_math || member.property.name.as_str() != "imul" || call.arguments.len() != 2 {
                return Err("only `Math.imul(a, b)` is supported for member calls".into());
            }
            for arg in &call.arguments {
                let expression = arg
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                if !self.infer_expression_type(expression)?.is_integer() {
                    return Err("Math.imul arguments must be integers".into());
                }
            }
            return Ok(CType::U32);
        }

        let Expression::Identifier(callee) = &call.callee else {
            return Err("indirect calls are unsupported".into());
        };
        let name = callee.name.as_str();
        if self.local(name).is_some() {
            return Err(marked_type_error(
                Some(call.span),
                format!("local binding `{name}` shadows the callable and cannot be called"),
            ));
        }
        match self.builtin(name) {
            Some(Builtin::MonotonicMs) if call.arguments.is_empty() => Ok(CType::F64),
            Some(Builtin::MonotonicMs) => Err("monotonicMs expects no arguments".into()),
            Some(Builtin::PrintBenchmark) if call.arguments.len() == 2 => {
                for (index, expected) in [CType::U32, CType::F64].into_iter().enumerate() {
                    let expression = call.arguments[index]
                        .as_expression()
                        .ok_or("spread arguments are unsupported")?;
                    self.ensure_assignable(expression, expected, "printBenchmark argument")?;
                }
                Ok(CType::Void)
            }
            Some(Builtin::PrintBenchmark) => Err("printBenchmark expects (u32, f64)".into()),
            Some(Builtin::PrintString) if call.arguments.len() == 1 => {
                let expression = call.arguments[0]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                self.ensure_assignable(expression, CType::String, "printString argument")?;
                Ok(CType::Void)
            }
            Some(Builtin::PrintString) => Err("printString expects one string argument".into()),
            Some(Builtin::PrintU32) if call.arguments.len() == 1 => {
                let expression = call.arguments[0]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                self.ensure_assignable(expression, CType::U32, "printU32 argument")?;
                Ok(CType::Void)
            }
            Some(Builtin::PrintU32) => Err("printU32 expects one u32 argument".into()),
            Some(Builtin::PrintI32) if call.arguments.len() == 1 => {
                let expression = call.arguments[0]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                self.ensure_assignable(expression, CType::I32, "printI32 argument")?;
                Ok(CType::Void)
            }
            Some(Builtin::PrintI32) => Err("printI32 expects one i32 argument".into()),
            Some(Builtin::PrintF64) if call.arguments.len() == 1 => {
                let expression = call.arguments[0]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                self.ensure_assignable(expression, CType::F64, "printF64 argument")?;
                Ok(CType::Void)
            }
            Some(Builtin::PrintF64) => Err("printF64 expects one f64 argument".into()),
            Some(Builtin::ReadStdin) if call.arguments.len() == 1 => {
                let buffer = call.arguments[0]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                self.buffer_name(buffer)?;
                Ok(CType::U32)
            }
            Some(Builtin::ReadStdin) => Err("readStdin expects one Uint8Array buffer".into()),
            Some(Builtin::SliceUtf8) if call.arguments.len() == 3 => {
                let value = call.arguments[0]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                let start = call.arguments[1]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                let end = call.arguments[2]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                self.ensure_assignable(value, CType::String, "sliceUtf8 value")?;
                self.ensure_assignable(start, CType::U32, "sliceUtf8 start")?;
                self.ensure_assignable(end, CType::U32, "sliceUtf8 end")?;
                Ok(CType::String)
            }
            Some(Builtin::SliceUtf8) => {
                Err("sliceUtf8 expects (string value, u32 start, u32 end)".into())
            }
            Some(Builtin::StringByteAt) if call.arguments.len() == 2 => {
                let value = call.arguments[0]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                let index = call.arguments[1]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                self.ensure_assignable(value, CType::String, "stringByteAt value")?;
                self.ensure_assignable(index, CType::U32, "stringByteAt index")?;
                Ok(CType::U8)
            }
            Some(Builtin::StringByteAt) => {
                Err("stringByteAt expects (string value, u32 index)".into())
            }
            Some(Builtin::ListenTcp) if call.arguments.len() == 2 => {
                let port = call.arguments[0]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                let loopback = call.arguments[1]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                self.ensure_assignable(port, CType::U16, "listenTcp port")?;
                self.ensure_assignable(loopback, CType::Bool, "listenTcp loopback flag")?;
                Ok(CType::I32)
            }
            Some(Builtin::ListenTcp) => Err("listenTcp expects (u16 port, bool loopback)".into()),
            Some(Builtin::AcceptTcp) if call.arguments.len() == 1 => {
                let fd = call.arguments[0]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                self.ensure_assignable(fd, CType::I32, "acceptTcp listener")?;
                Ok(CType::I32)
            }
            Some(Builtin::AcceptTcp) => Err("acceptTcp expects one i32 listener".into()),
            Some(Builtin::ReadTcp) if call.arguments.len() == 3 => {
                let fd = call.arguments[0]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                let buffer = call.arguments[1]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                let offset = call.arguments[2]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                self.ensure_assignable(fd, CType::I32, "readTcp connection")?;
                self.buffer_name(buffer)?;
                self.ensure_assignable(offset, CType::U32, "readTcp offset")?;
                Ok(CType::U32)
            }
            Some(Builtin::ReadTcp) => {
                Err("readTcp expects (i32 connection, Uint8Array buffer, u32 offset)".into())
            }
            Some(Builtin::WriteTcp) if call.arguments.len() == 3 => {
                let fd = call.arguments[0]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                let buffer = call.arguments[1]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                let offset = call.arguments[2]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                self.ensure_assignable(fd, CType::I32, "writeTcp connection")?;
                self.buffer_name(buffer)?;
                self.ensure_assignable(offset, CType::U32, "writeTcp offset")?;
                Ok(CType::U32)
            }
            Some(Builtin::WriteTcp) => {
                Err("writeTcp expects (i32 connection, Uint8Array buffer, u32 offset)".into())
            }
            Some(Builtin::WriteTcpString) if call.arguments.len() == 3 => {
                let fd = call.arguments[0]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                let text = call.arguments[1]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                let offset = call.arguments[2]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                self.ensure_assignable(fd, CType::I32, "writeTcpString connection")?;
                self.ensure_assignable(text, CType::String, "writeTcpString value")?;
                self.ensure_assignable(offset, CType::U32, "writeTcpString offset")?;
                Ok(CType::U32)
            }
            Some(Builtin::WriteTcpString) => {
                Err("writeTcpString expects (i32 connection, string value, u32 offset)".into())
            }
            Some(Builtin::CloseTcp) if call.arguments.len() == 1 => {
                let fd = call.arguments[0]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                self.ensure_assignable(fd, CType::I32, "closeTcp connection")?;
                Ok(CType::I32)
            }
            Some(Builtin::CloseTcp) => Err("closeTcp expects one i32 descriptor".into()),
            Some(Builtin::ShutdownRequested) if call.arguments.is_empty() => Ok(CType::Bool),
            Some(Builtin::ShutdownRequested) => {
                Err("shutdownRequested expects no arguments".into())
            }
            Some(Builtin::SpawnThread) if call.arguments.len() == 2 => {
                let worker = call.arguments[0]
                    .as_expression()
                    .and_then(|expr| match expr {
                        Expression::Identifier(identifier) => Some(identifier.name.as_str()),
                        _ => None,
                    })
                    .ok_or("spawnThread expects a named worker function")?;
                let target = self
                    .call_targets
                    .get(worker)
                    .ok_or_else(|| format!("unknown worker function `{worker}`"))?;
                let signature = self
                    .functions
                    .get(target)
                    .ok_or_else(|| format!("unknown worker function `{worker}`"))?;
                if signature.return_type != CType::Void
                    || signature.params.len() != 1
                    || signature.params[0].1 != CType::I32
                {
                    return Err(
                        "spawnThread worker must have signature `function(i32): void`".into(),
                    );
                }
                let argument = call.arguments[1]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                self.ensure_assignable(argument, CType::I32, "spawnThread argument")?;
                Ok(CType::I32)
            }
            Some(Builtin::SpawnThread) => Err("spawnThread expects (worker, i32 argument)".into()),
            None => {
                let target = self
                    .call_targets
                    .get(name)
                    .ok_or_else(|| format!("unknown function or builtin `{name}`"))?;
                let signature = self
                    .functions
                    .get(target)
                    .ok_or_else(|| format!("unknown imported function `{target}`"))?;
                if call.arguments.len() != signature.params.len() {
                    return Err(format!(
                        "function `{name}` expects {} argument(s), got {}",
                        signature.params.len(),
                        call.arguments.len()
                    ));
                }
                for (argument, (_, expected)) in call.arguments.iter().zip(&signature.params) {
                    let expression = argument
                        .as_expression()
                        .ok_or("spread arguments are unsupported")?;
                    self.ensure_assignable(
                        expression,
                        *expected,
                        &format!("argument to `{name}`"),
                    )?;
                }
                Ok(signature.return_type)
            }
        }
    }

    fn expression(&self, expression: &Expression<'_>) -> Result<String, String> {
        self.infer_expression_type(expression)?;
        match expression {
            Expression::BooleanLiteral(literal) => Ok(if literal.value { "1" } else { "0" }.into()),
            Expression::StringLiteral(literal) => {
                let value = literal.value.as_str().ok_or_else(|| {
                    marked_type_error(
                        Some(literal.span),
                        "string contains a lone UTF-16 surrogate",
                    )
                })?;
                let encoded = value
                    .as_bytes()
                    .iter()
                    .map(|byte| format!("\\x{byte:02x}"))
                    .collect::<String>();
                Ok(format!(
                    "((TsnString){{{}u, (const uint8_t *)\"{encoded}\"}})",
                    value.len()
                ))
            }
            Expression::NumericLiteral(literal) => {
                let raw = &self.source[literal.span.start as usize..literal.span.end as usize];
                Ok(raw.replace('_', ""))
            }
            Expression::Identifier(identifier) => {
                let name = identifier.name.as_str();
                if self.local(name).is_some() {
                    Ok(name.to_string())
                } else {
                    Err(format!("unknown local `{name}`"))
                }
            }
            Expression::BinaryExpression(binary) => {
                if binary.operator == BinaryOperator::Division {
                    let left = self.expression(&binary.left)?;
                    let right = self.expression(&binary.right)?;
                    return Ok(format!("((double)({left}) / (double)({right}))"));
                }
                if matches!(
                    binary.operator,
                    BinaryOperator::StrictEquality | BinaryOperator::StrictInequality
                ) && self.infer_expression_type(&binary.left)? == CType::String
                {
                    let equal = format!(
                        "tsn_string_equal({}, {})",
                        self.expression(&binary.left)?,
                        self.expression(&binary.right)?
                    );
                    return Ok(if binary.operator == BinaryOperator::StrictEquality {
                        equal
                    } else {
                        format!("(!{equal})")
                    });
                }
                if binary.operator == BinaryOperator::Addition
                    && self.infer_expression_type(expression)? == CType::String
                {
                    return Ok(format!(
                        "tsn_string_concat({}, {})",
                        self.expression(&binary.left)?,
                        self.expression(&binary.right)?
                    ));
                }
                let op = match binary.operator {
                    BinaryOperator::Addition => "+",
                    BinaryOperator::Subtraction => "-",
                    BinaryOperator::Multiplication => "*",
                    BinaryOperator::BitwiseXOR => "^",
                    BinaryOperator::BitwiseAnd => "&",
                    BinaryOperator::BitwiseOR => "|",
                    BinaryOperator::ShiftRightZeroFill => ">>",
                    BinaryOperator::ShiftLeft => "<<",
                    BinaryOperator::StrictEquality => "==",
                    BinaryOperator::StrictInequality => "!=",
                    BinaryOperator::LessThan => "<",
                    BinaryOperator::LessEqualThan => "<=",
                    BinaryOperator::GreaterThan => ">",
                    BinaryOperator::GreaterEqualThan => ">=",
                    _ => return Err(format!("unsupported binary operator {:?}", binary.operator)),
                };
                let left = self.expression(&binary.left)?;
                let right = self.expression(&binary.right)?;
                let result_type = self.infer_expression_type(expression)?;
                if matches!(result_type, CType::I8 | CType::I16) {
                    let wrap = if result_type == CType::I8 {
                        "tsn_i8_wrap"
                    } else {
                        "tsn_i16_wrap"
                    };
                    return Ok(format!(
                        "{wrap}((int32_t)((int32_t)({left}) {op} (int32_t)({right})))"
                    ));
                }
                if matches!(result_type, CType::U8 | CType::U16) {
                    return Ok(format!(
                        "(({})(({left}) {op} ({right})))",
                        result_type.c_name()
                    ));
                }
                if matches!(
                    binary.operator,
                    BinaryOperator::ShiftRightZeroFill | BinaryOperator::ShiftLeft
                ) {
                    let left_type = self.infer_expression_type(&binary.left)?;
                    let shift_mask = if left_type == CType::U64 { "63" } else { "31" };
                    Ok(format!("({} {op} ({} & {shift_mask}))", left, right))
                } else {
                    Ok(format!("({left} {op} {right})"))
                }
            }
            Expression::LogicalExpression(logical) => {
                let operator = match logical.operator {
                    LogicalOperator::And => "&&",
                    LogicalOperator::Or => "||",
                    _ => return Err("unsupported logical operator".into()),
                };
                Ok(format!(
                    "({} {operator} {})",
                    self.expression(&logical.left)?,
                    self.expression(&logical.right)?
                ))
            }
            Expression::ConditionalExpression(conditional) => Ok(format!(
                "(({}) ? ({}) : ({}))",
                self.expression(&conditional.test)?,
                self.expression(&conditional.consequent)?,
                self.expression(&conditional.alternate)?
            )),
            Expression::ParenthesizedExpression(paren) => {
                Ok(format!("({})", self.expression(&paren.expression)?))
            }
            Expression::TSAsExpression(cast) => {
                let target = CType::parse(
                    self.source,
                    Some(cast.type_annotation.span()),
                    "cast target",
                )?;
                let value = self.expression(&cast.expression)?;
                match target {
                    CType::U8 => Ok(format!("((uint8_t)({value}))")),
                    CType::U16 => Ok(format!("((uint16_t)({value}))")),
                    CType::U32 => Ok(format!("((uint32_t)({value}))")),
                    CType::U64 => Ok(format!("((uint64_t)({value}))")),
                    CType::I8 => Ok(format!("tsn_i8_wrap((int32_t)(uint8_t)({value}))")),
                    CType::I16 => Ok(format!("tsn_i16_wrap((int32_t)(uint16_t)({value}))")),
                    CType::I32 => Ok(format!("tsn_i32_from_bits((uint32_t)({value}))")),
                    CType::I64 => Ok(format!("tsn_i64_from_bits((uint64_t)({value}))")),
                    _ => Err(marked_type_error(
                        Some(cast.span),
                        "explicit `as` casts currently require integer source and target types",
                    )),
                }
            }
            Expression::UnaryExpression(unary) => match unary.operator {
                UnaryOperator::UnaryPlus => Ok(format!("(+{})", self.expression(&unary.argument)?)),
                UnaryOperator::UnaryNegation => {
                    if self.integer_literal_value(expression) == Some(i64::MIN as i128) {
                        Ok("INT64_MIN".into())
                    } else if matches!(
                        self.infer_expression_type(expression)?,
                        CType::I8 | CType::I16
                    ) {
                        let wrap = if self.infer_expression_type(expression)? == CType::I8 {
                            "tsn_i8_wrap"
                        } else {
                            "tsn_i16_wrap"
                        };
                        Ok(format!(
                            "{wrap}(-(int32_t)({}))",
                            self.expression(&unary.argument)?
                        ))
                    } else {
                        Ok(format!("(-{})", self.expression(&unary.argument)?))
                    }
                }
                UnaryOperator::LogicalNot => {
                    Ok(format!("(!{})", self.expression(&unary.argument)?))
                }
                UnaryOperator::BitwiseNot => {
                    let argument = self.expression(&unary.argument)?;
                    match self.infer_expression_type(expression)? {
                        CType::I8 => Ok(format!("tsn_i8_wrap((int32_t)(~(int32_t)({argument})))")),
                        CType::I16 => {
                            Ok(format!("tsn_i16_wrap((int32_t)(~(int32_t)({argument})))"))
                        }
                        CType::U8 => Ok(format!("((uint8_t)(~(uint8_t)({argument})))")),
                        CType::U16 => Ok(format!("((uint16_t)(~(uint16_t)({argument})))")),
                        _ => Ok(format!("(~{argument})")),
                    }
                }
                _ => Err(format!("unsupported unary operator {:?}", unary.operator)),
            },
            Expression::NewExpression(new_expression) => {
                let Expression::Identifier(callee) = &new_expression.callee else {
                    return Err("only fixed-width typed array allocation is supported".into());
                };
                let allocator = match callee.name.as_str() {
                    "Uint8Array" => "tsn_u8_array_new",
                    "Uint32Array" => "tsn_u32_array_new",
                    _ => {
                        return Err(
                            "only Uint8Array and Uint32Array allocation is supported".into()
                        );
                    }
                };
                if new_expression.arguments.len() != 1 {
                    return Err("typed array allocation expects one length".into());
                }
                let length = new_expression.arguments[0]
                    .as_expression()
                    .ok_or("spread allocation arguments are unsupported")?;
                Ok(format!("{allocator}({})", self.expression(length)?))
            }
            Expression::ComputedMemberExpression(member) => {
                let array_type = self.typed_array_type(&member.object)?;
                let buffer = if array_type == CType::U8Array {
                    self.buffer_name(&member.object)?
                } else {
                    self.word_array_name(&member.object)?
                };
                let (get, get_known) = if array_type == CType::U8Array {
                    ("tsn_u8_get", "tsn_u8_get_known")
                } else {
                    ("tsn_u32_get", "tsn_u32_get_known")
                };
                if let Expression::Identifier(identifier) = &member.object
                    && let Some(len) = self
                        .local(identifier.name.as_str())
                        .and_then(|local| local.buffer_len)
                {
                    return Ok(format!(
                        "{get_known}({buffer}, {}, {len})",
                        self.expression(&member.expression)?
                    ));
                }
                Ok(format!(
                    "{get}({buffer}, {})",
                    self.expression(&member.expression)?
                ))
            }
            Expression::StaticMemberExpression(member) => {
                if member.property.name.as_str() != "length" {
                    return Err("only `.length` is supported on strings and Uint8Array".into());
                }
                let Expression::Identifier(identifier) = &member.object else {
                    return Err("`.length` requires a local string or Uint8Array".into());
                };
                let name = identifier.name.as_str();
                match self.local(name).map(|local| local.ty) {
                    Some(CType::U8Array | CType::U32Array) => {
                        if let Some(len) = self.local(name).and_then(|local| local.buffer_len) {
                            Ok(format!("{len}u"))
                        } else {
                            Ok(format!("({name}->len)"))
                        }
                    }
                    Some(CType::String) => Ok(format!("({name}.len)")),
                    Some(_) => Err(format!("`{name}` has no `.length` property")),
                    None => Err(format!("unknown local `{name}`")),
                }
            }
            Expression::CallExpression(call) => self.call_expression(call),
            Expression::AssignmentExpression(assign) => {
                if assign.operator != oxc_syntax::operator::AssignmentOperator::Assign {
                    return Err("compound assignment is unsupported".into());
                }
                match &assign.left {
                    AssignmentTarget::AssignmentTargetIdentifier(id) => {
                        let name = id.name.as_str();
                        let Some(local) = self.local(name) else {
                            return Err(format!("unknown local `{name}`"));
                        };
                        if !local.mutable {
                            return Err(format!("cannot assign to immutable local `{name}`"));
                        }
                        Ok(format!("({name} = {})", self.expression(&assign.right)?))
                    }
                    AssignmentTarget::ComputedMemberExpression(member) => {
                        let array_type = self.typed_array_type(&member.object)?;
                        let buffer = if array_type == CType::U8Array {
                            self.buffer_name(&member.object)?
                        } else {
                            self.word_array_name(&member.object)?
                        };
                        let setter = if array_type == CType::U8Array {
                            "tsn_u8_set"
                        } else {
                            "tsn_u32_set"
                        };
                        Ok(format!(
                            "{setter}({buffer}, {}, {})",
                            self.expression(&member.expression)?,
                            self.expression(&assign.right)?
                        ))
                    }
                    _ => Err("only local variables and Uint8Array elements are assignable".into()),
                }
            }
            Expression::UpdateExpression(update) => {
                let oxc_ast::ast::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) =
                    &update.argument
                else {
                    return Err("only local-variable increment is supported".into());
                };
                let name = id.name.as_str();
                let Some(local) = self.local(name) else {
                    return Err(format!("unknown local `{name}`"));
                };
                if !local.mutable {
                    return Err(format!("cannot update immutable local `{name}`"));
                }
                if !matches!(
                    local.ty,
                    CType::U8
                        | CType::U16
                        | CType::U32
                        | CType::U64
                        | CType::I8
                        | CType::I16
                        | CType::I32
                        | CType::I64
                ) {
                    return Err(format!("increment requires an integer local, got `{name}`"));
                }
                let (operator, helper_suffix) = match update.operator {
                    oxc_syntax::operator::UpdateOperator::Increment => ("+", "inc"),
                    oxc_syntax::operator::UpdateOperator::Decrement => ("-", "dec"),
                };
                match local.ty {
                    CType::I8 | CType::I16 if update.prefix => {
                        let wrap = if local.ty == CType::I8 {
                            "tsn_i8_wrap"
                        } else {
                            "tsn_i16_wrap"
                        };
                        Ok(format!("{name} = {wrap}((int32_t){name} {operator} 1)"))
                    }
                    CType::I8 | CType::I16 => {
                        let helper = if local.ty == CType::I8 {
                            format!("tsn_i8_post_{helper_suffix}")
                        } else {
                            format!("tsn_i16_post_{helper_suffix}")
                        };
                        Ok(format!("{helper}(&{name})"))
                    }
                    _ if update.prefix && helper_suffix == "inc" => Ok(format!("(++{name})")),
                    _ if update.prefix => Ok(format!("(--{name})")),
                    _ if helper_suffix == "inc" => Ok(format!("({name}++)")),
                    _ => Ok(format!("({name}--)")),
                }
            }
            _ => Err(format!("unsupported expression in M1: {expression:?}")),
        }
    }

    fn call_expression(&self, call: &oxc_ast::ast::CallExpression<'_>) -> Result<String, String> {
        if let Expression::StaticMemberExpression(member) = &call.callee {
            let is_math =
                matches!(&member.object, Expression::Identifier(id) if id.name.as_str() == "Math");
            if is_math && member.property.name.as_str() == "imul" && call.arguments.len() == 2 {
                let a = call.arguments[0]
                    .as_expression()
                    .ok_or("unsupported Math.imul argument")?;
                let b = call.arguments[1]
                    .as_expression()
                    .ok_or("unsupported Math.imul argument")?;
                return Ok(format!(
                    "((uint32_t)({}) * (uint32_t)({}))",
                    self.expression(a)?,
                    self.expression(b)?
                ));
            }
            return Err("only Math.imul is supported for member calls".into());
        }
        let Expression::Identifier(callee) = &call.callee else {
            return Err("indirect calls are unsupported".into());
        };
        let name = callee.name.as_str();
        match self.builtin(name) {
            Some(Builtin::MonotonicMs) if call.arguments.is_empty() => Ok("tsn_clock_ms()".into()),
            Some(Builtin::MonotonicMs) => Err("monotonicMs expects no arguments".into()),
            Some(Builtin::PrintBenchmark) if call.arguments.len() == 2 => {
                let args = call
                    .arguments
                    .iter()
                    .map(|arg| {
                        arg.as_expression()
                            .ok_or_else(|| "spread arguments are unsupported".to_string())
                            .and_then(|expr| self.expression(expr))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(format!("tsn_print_benchmark({}, {})", args[0], args[1]))
            }
            Some(Builtin::PrintBenchmark) => Err("printBenchmark expects two arguments".into()),
            Some(Builtin::PrintString) if call.arguments.len() == 1 => {
                let arg = call.arguments[0]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                Ok(format!("tsn_print_string({})", self.expression(arg)?))
            }
            Some(Builtin::PrintString) => Err("printString expects one string argument".into()),
            Some(Builtin::PrintU32) if call.arguments.len() == 1 => {
                let expression = call.arguments[0]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                Ok(format!(
                    "tsn_print_u32((uint32_t)({}))",
                    self.expression(expression)?
                ))
            }
            Some(Builtin::PrintU32) => Err("printU32 expects one argument".into()),
            Some(Builtin::PrintI32) if call.arguments.len() == 1 => {
                let expression = call.arguments[0]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                Ok(format!(
                    "tsn_print_i32((int32_t)({}))",
                    self.expression(expression)?
                ))
            }
            Some(Builtin::PrintI32) => Err("printI32 expects one argument".into()),
            Some(Builtin::PrintF64) if call.arguments.len() == 1 => {
                let expression = call.arguments[0]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                Ok(format!(
                    "tsn_print_f64((double)({}))",
                    self.expression(expression)?
                ))
            }
            Some(Builtin::PrintF64) => Err("printF64 expects one argument".into()),
            Some(Builtin::ReadStdin) if call.arguments.len() == 1 => {
                let buffer = call.arguments[0]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                let buffer_name = self.buffer_name(buffer)?;
                Ok(format!("tsn_read_stdin({buffer_name})"))
            }
            Some(Builtin::ReadStdin) => Err("readStdin expects one argument".into()),
            Some(Builtin::SliceUtf8) if call.arguments.len() == 3 => {
                let value = call.arguments[0]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                let start = call.arguments[1]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                let end = call.arguments[2]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                Ok(format!(
                    "tsn_string_slice_utf8({}, (uint32_t)({}), (uint32_t)({}))",
                    self.expression(value)?,
                    self.expression(start)?,
                    self.expression(end)?
                ))
            }
            Some(Builtin::SliceUtf8) => Err("sliceUtf8 expects three arguments".into()),
            Some(Builtin::StringByteAt) if call.arguments.len() == 2 => {
                let value = call.arguments[0]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                let index = call.arguments[1]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                Ok(format!(
                    "tsn_string_byte_at({}, (uint32_t)({}))",
                    self.expression(value)?,
                    self.expression(index)?
                ))
            }
            Some(Builtin::StringByteAt) => Err("stringByteAt expects two arguments".into()),
            Some(Builtin::ListenTcp) if call.arguments.len() == 2 => {
                let port = call.arguments[0]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                let loopback = call.arguments[1]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                Ok(format!(
                    "tsn_listen_tcp((uint16_t)({}), {})",
                    self.expression(port)?,
                    self.expression(loopback)?
                ))
            }
            Some(Builtin::ListenTcp) => Err("listenTcp expects two arguments".into()),
            Some(Builtin::AcceptTcp) if call.arguments.len() == 1 => {
                let listener = call.arguments[0]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                Ok(format!(
                    "tsn_accept_tcp((int32_t)({}))",
                    self.expression(listener)?
                ))
            }
            Some(Builtin::AcceptTcp) => Err("acceptTcp expects one argument".into()),
            Some(Builtin::ReadTcp) if call.arguments.len() == 3 => {
                let fd = call.arguments[0]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                let buffer = call.arguments[1]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                let offset = call.arguments[2]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                Ok(format!(
                    "tsn_read_tcp((int32_t)({}), {}, (uint32_t)({}))",
                    self.expression(fd)?,
                    self.buffer_name(buffer)?,
                    self.expression(offset)?
                ))
            }
            Some(Builtin::ReadTcp) => Err("readTcp expects three arguments".into()),
            Some(Builtin::WriteTcp) if call.arguments.len() == 3 => {
                let fd = call.arguments[0]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                let buffer = call.arguments[1]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                let offset = call.arguments[2]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                Ok(format!(
                    "tsn_write_tcp((int32_t)({}), {}, (uint32_t)({}))",
                    self.expression(fd)?,
                    self.buffer_name(buffer)?,
                    self.expression(offset)?
                ))
            }
            Some(Builtin::WriteTcp) => Err("writeTcp expects three arguments".into()),
            Some(Builtin::WriteTcpString) if call.arguments.len() == 3 => {
                let fd = call.arguments[0]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                let text = call.arguments[1]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                let offset = call.arguments[2]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                Ok(format!(
                    "tsn_write_tcp_string((int32_t)({}), {}, (uint32_t)({}))",
                    self.expression(fd)?,
                    self.expression(text)?,
                    self.expression(offset)?
                ))
            }
            Some(Builtin::WriteTcpString) => Err("writeTcpString expects three arguments".into()),
            Some(Builtin::CloseTcp) if call.arguments.len() == 1 => {
                let fd = call.arguments[0]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                Ok(format!(
                    "tsn_close_tcp((int32_t)({}))",
                    self.expression(fd)?
                ))
            }
            Some(Builtin::CloseTcp) => Err("closeTcp expects one argument".into()),
            Some(Builtin::ShutdownRequested) if call.arguments.is_empty() => {
                Ok("tsn_shutdown_requested()".into())
            }
            Some(Builtin::ShutdownRequested) => {
                Err("shutdownRequested expects no arguments".into())
            }
            Some(Builtin::SpawnThread) if call.arguments.len() == 2 => {
                let worker = call.arguments[0]
                    .as_expression()
                    .and_then(|expr| match expr {
                        Expression::Identifier(identifier) => Some(identifier.name.as_str()),
                        _ => None,
                    })
                    .ok_or("spawnThread expects a named worker function")?;
                let target = self
                    .call_targets
                    .get(worker)
                    .ok_or_else(|| format!("unknown worker function `{worker}`"))?;
                let argument = call.arguments[1]
                    .as_expression()
                    .ok_or("spread arguments are unsupported")?;
                Ok(format!(
                    "tsn_spawn_thread(tsn_{target}, (int32_t)({}))",
                    self.expression(argument)?
                ))
            }
            Some(Builtin::SpawnThread) => Err("spawnThread expects two arguments".into()),
            None => {
                let target = self
                    .call_targets
                    .get(name)
                    .ok_or_else(|| format!("unknown function or builtin `{name}`"))?;
                let Some(signature) = self.functions.get(target) else {
                    return Err(format!("unknown imported function `{target}`"));
                };
                if call.arguments.len() != signature.params.len() {
                    return Err(format!(
                        "function `{name}` expects {} argument(s), got {}",
                        signature.params.len(),
                        call.arguments.len()
                    ));
                }
                let args = call
                    .arguments
                    .iter()
                    .map(|arg| {
                        arg.as_expression()
                            .ok_or_else(|| "spread arguments are unsupported".to_string())
                            .and_then(|expr| self.expression(expr))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(format!("tsn_{target}({})", args.join(", ")))
            }
        }
    }

    fn condition(&self, expression: &Expression<'_>) -> Result<String, String> {
        if self.infer_expression_type(expression)? != CType::Bool {
            return Err("condition must have type bool".into());
        }
        match expression {
            Expression::BinaryExpression(binary) => {
                if matches!(
                    binary.operator,
                    BinaryOperator::StrictEquality | BinaryOperator::StrictInequality
                ) && self.infer_expression_type(&binary.left)? == CType::String
                {
                    let equal = format!(
                        "tsn_string_equal({}, {})",
                        self.expression(&binary.left)?,
                        self.expression(&binary.right)?
                    );
                    return Ok(if binary.operator == BinaryOperator::StrictEquality {
                        equal
                    } else {
                        format!("(!{equal})")
                    });
                }
                let op = match binary.operator {
                    BinaryOperator::LessThan => "<",
                    BinaryOperator::LessEqualThan => "<=",
                    BinaryOperator::GreaterThan => ">",
                    BinaryOperator::GreaterEqualThan => ">=",
                    BinaryOperator::StrictEquality | BinaryOperator::Equality => "==",
                    BinaryOperator::StrictInequality | BinaryOperator::Inequality => "!=",
                    _ => return Err("condition requires a comparison operator".into()),
                };
                Ok(format!(
                    "{} {op} {}",
                    self.expression(&binary.left)?,
                    self.expression(&binary.right)?
                ))
            }
            Expression::LogicalExpression(logical) => {
                let op = match logical.operator {
                    LogicalOperator::And => "&&",
                    LogicalOperator::Or => "||",
                    _ => return Err("nullish coalescing is not a boolean condition".into()),
                };
                Ok(format!(
                    "{} {op} {}",
                    self.condition(&logical.left)?,
                    self.condition(&logical.right)?
                ))
            }
            Expression::BooleanLiteral(literal) => Ok(if literal.value { "1" } else { "0" }.into()),
            Expression::Identifier(identifier) => {
                let name = identifier.name.as_str();
                match self.local(name) {
                    Some(local) if local.ty == CType::Bool => Ok(name.to_string()),
                    Some(_) => Err(format!("condition `{name}` must have type bool")),
                    None => Err(format!("unknown local `{name}`")),
                }
            }
            Expression::ParenthesizedExpression(paren) => self.condition(&paren.expression),
            Expression::UnaryExpression(unary) if unary.operator == UnaryOperator::LogicalNot => {
                Ok(format!("!{}", self.condition(&unary.argument)?))
            }
            // Boolean-returning calls and other already-typechecked boolean
            // expressions are valid conditions too; keep comparison lowering
            // above for string equality and direct relational operators.
            _ => self.expression(expression),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Diagnostic, emit_c_source};
    use oxc_allocator::Allocator;
    use oxc_parser::Parser;
    use oxc_span::{SourceType, Span};

    fn lower(source: &str) -> Result<String, String> {
        let allocator = Allocator::default();
        let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
        if !parsed.diagnostics.is_empty() {
            return Err("test fixture has invalid TypeScript syntax".into());
        }
        emit_c_source(source, &parsed.program)
    }

    #[test]
    fn lowers_standalone_byte_kernel_fixture() {
        let source = include_str!("../../../benchmarks/byte-kernel/kernel.ts");
        let generated = lower(source).expect("the benchmark fixture is in the supported subset");
        assert!(generated.contains("uint32_t tsn_kernel(uint32_t n)"));
        assert!(generated.contains("int32_t tsn_main(void)"));
        assert!(generated.contains("int main(void)"));
    }

    #[test]
    fn lowers_utf8_string_literal_and_byte_length() {
        let source = include_str!("../../../examples/string.ts");
        let generated = lower(source).expect("string literal/output subset should lower");
        assert!(generated.contains("TsnString greeting"));
        assert!(generated.contains("tsn_print_string(greeting)"));
        assert!(generated.contains("greeting.len"));
        assert!(generated.contains("\\xc3\\xa9"));
    }

    #[test]
    fn resolves_named_time_and_io_standard_imports() {
        let source = include_str!("../../../examples/stdlib.ts");
        let generated = lower(source).expect("supported standard packages should link");
        assert!(generated.contains("tsn_clock_ms()"));
        assert!(generated.contains("tsn_print_string"));
    }

    #[test]
    fn resolves_print_u32_standard_import() {
        let source = include_str!("../../../examples/print-u32.ts");
        let generated = lower(source).expect("printU32 is a typed tsn:io API");
        assert!(generated.contains("tsn_print_u32((uint32_t)(42))"));
    }

    #[test]
    fn resolves_bounded_stdin_read_into_owned_buffer() {
        let source = "import { readStdin } from \"tsn:io\"; function main(): i32 { const bytes: Uint8Array = new Uint8Array(16); const count: u32 = readStdin(bytes); return 0; }";
        let generated = lower(source).expect("bounded stdin read should lower");
        assert!(generated.contains("tsn_read_stdin(bytes)"));
    }

    #[test]
    fn resolves_tcp_platform_primitives() {
        let source = "import { listenTcp, acceptTcp, readTcp, writeTcpString, closeTcp } from \"tsn:net\"; import { shutdownRequested } from \"tsn:os\"; function main(): i32 { if (shutdownRequested()) return 1; const listener: i32 = listenTcp(8080, true); const connection: i32 = acceptTcp(listener); const buffer: Uint8Array = new Uint8Array(16); const count: u32 = readTcp(connection, buffer, 0); const written: u32 = writeTcpString(connection, \"ok\", 0); closeTcp(connection); closeTcp(listener); return 0; }";
        let generated =
            lower(source).expect("socket operations should be host-boundary primitives");
        assert!(generated.contains("tsn_listen_tcp((uint16_t)(8080), 1)"));
        assert!(generated.contains("tsn_accept_tcp((int32_t)(listener))"));
        assert!(generated.contains("tsn_read_tcp((int32_t)(connection), buffer, (uint32_t)(0))"));
        assert!(generated.contains("tsn_write_tcp_string"));
        assert!(generated.contains("tsn_close_tcp"));
        assert!(generated.contains("tsn_shutdown_requested()"));
    }

    #[test]
    fn lowers_bounded_thread_spawn_for_named_worker() {
        let source = "import { spawnThread } from \"tsn:os\"; function worker(value: i32): void { const ignored: i32 = value; } function main(): i32 { const status: i32 = spawnThread(worker, 7); return status; }";
        let generated = lower(source).expect("named i32 workers should be spawnable");
        assert!(generated.contains("void tsn_worker(int32_t value)"));
        assert!(generated.contains("tsn_spawn_thread(tsn_worker, (int32_t)(7))"));
        assert!(generated.contains("tsn_wait_workers()"));
    }

    #[test]
    fn resolves_typed_integer_and_float_io_imports() {
        let source = "import { printI32, printF64 } from \"tsn:io\"; function main(): i32 { printI32(-7); printF64(3.5); return 0; }";
        let generated = lower(source).expect("typed numeric output APIs should lower");
        assert!(generated.contains("tsn_print_i32"));
        assert!(generated.contains("tsn_print_f64"));
    }

    #[test]
    fn type_only_import_does_not_create_runtime_binding() {
        let source = "import type { monotonicMs } from \"tsn:time\"; function main(): i32 { const now: f64 = monotonicMs(); return 0; }";
        let error = lower(source).expect_err("type-only import must be erased at runtime");
        assert!(error.contains("unknown function or builtin"));
    }

    #[test]
    fn rejects_lone_surrogate_string_literals() {
        let source = r#"function main(): i32 { const text: string = "\uD800"; printString(text); return 0; }"#;
        let error = lower(source).expect_err("strings must be valid UTF-8");
        assert!(error.contains("lone UTF-16 surrogate"));
    }

    #[test]
    fn lowers_string_equality_by_contents() {
        let source = "function main(): i32 { const a: string = \"a\"; const b: string = \"a\"; if (a === b) return 0; return 1; }";
        let generated = lower(source).expect("strict string equality uses byte contents");
        assert!(generated.contains("tsn_string_equal(a, b)"));
    }

    #[test]
    fn lowers_string_concatenation_to_owned_utf8_runtime_value() {
        let source = include_str!("../../../examples/string-concat.ts");
        let generated = lower(source).expect("string concatenation should lower");
        assert!(generated.contains("tsn_string_concat("));
        assert!(generated.contains("tsn_live_strings"));
        assert!(generated.contains("tsn_cleanup_all()"));
    }

    #[test]
    fn rejects_implicit_string_concatenation_coercion() {
        let source = "function main(): i32 { const text: string = \"value=\" + 3; return 0; }";
        let error = lower(source).expect_err("string concatenation must not coerce numbers");
        assert!(error.contains("two string operands"));
    }

    #[test]
    fn lowers_explicit_integer_narrowing_cast() {
        let source = include_str!("../../../examples/byte-cast.ts");
        let generated = lower(source).expect("explicit integer cast should lower");
        assert!(generated.contains("((uint8_t)(wide))"));
    }

    #[test]
    fn rejects_assignment_to_const_binding() {
        let source = "function main(): i32 { const value: u32 = 1; value = 2; return 0; }";
        let error = lower(source).expect_err("const assignment must be rejected before C emission");
        assert!(
            error.contains("immutable local"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn rejects_increment_of_const_binding() {
        let source = "function main(): i32 { const value: u32 = 1; value++; return 0; }";
        let error = lower(source).expect_err("incrementing const must be rejected");
        assert!(
            error.contains("immutable local"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn rejects_program_without_standalone_main() {
        let source = "function kernel(n: u32): u32 { return n; }";
        let error = lower(source).expect_err("a standalone program requires main");
        assert!(error.contains("main"), "unexpected error: {error}");
    }

    #[test]
    fn lowers_if_else_and_while_with_lexical_scopes() {
        let source = r#"
            function main(): i32 {
                let n: u32 = 0;
                while (n < 3) { n++; }
                if (n === 3) { return 0; } else { return 1; }
            }
        "#;
        let generated = lower(source).expect("if/else and while should lower");
        assert!(generated.contains("while (n < 3)"));
        assert!(generated.contains("if (n == 3)"));
    }

    #[test]
    fn rejects_local_used_outside_its_block_scope() {
        let source = "function main(): i32 { { const value: u32 = 1; } return value; }";
        let error = lower(source).expect_err("block-local binding must not escape");
        assert!(error.contains("unknown local"), "unexpected error: {error}");
    }

    #[test]
    fn lowers_checked_mutable_byte_buffer_access() {
        let source = r#"
            function main(): i32 {
                const flags: Uint8Array = new Uint8Array(16);
                flags[3] = 1;
                if (flags[3] === 1) { return 0; }
                return 1;
            }
        "#;
        let generated = lower(source).expect("typed byte-buffer operations should lower");
        assert!(generated.contains("tsn_u8_array_new(16)"));
        assert!(generated.contains("if (i >= a->len) tsn_bounds_fail()"));
        assert!(generated.contains("tsn_cleanup_all()"));
        assert!(generated.contains("tsn_u8_set(flags, 3, 1)"));
        assert!(generated.contains("tsn_u8_get_known(flags, 3, 16)"));
        assert!(generated.contains("tsn_u8_array_free(flags)"));
    }

    #[test]
    fn lowers_owned_u32_array_with_checked_access_and_cleanup() {
        let source = include_str!("../../../examples/word-array.ts");
        let generated = lower(source).expect("Uint32Array allocation and access should lower");
        assert!(generated.contains("tsn_u32_array_new(2)"));
        assert!(generated.contains("tsn_u32_set(words, 0, 0x12345678)"));
        assert!(generated.contains("tsn_u32_get_known(words, 1, 2)"));
        assert!(generated.contains("tsn_u32_array_free(words)"));
    }

    #[test]
    fn preserves_static_buffer_length_for_optimizer_bounds_proofs() {
        let source = "function main(): i32 { const bytes: Uint8Array = new Uint8Array(16); let i: u32 = 0; let sum: u32 = 0; while (i < bytes.length) { sum = sum + bytes[i]; i++; } return 0; }";
        let generated = lower(source).expect("fixed-size buffer loop should lower");
        assert!(generated.contains("tsn_u8_get_known(bytes, i, 16)"));
        assert!(generated.contains("while (i < 16u)"));
    }

    #[test]
    fn lowers_general_sieve_source_without_pattern_special_cases() {
        let source = include_str!("../../../benchmarks/sieve/sieve.ts");
        let generated = lower(source).expect("ordinary sieve constructs should lower");
        assert!(generated.contains("tsn_u8_array_new"));
        assert!(generated.contains("tsn_u8_get"));
        assert!(generated.contains("tsn_u8_set"));
        assert!(generated.contains("tsn_u8_array_free(composite)"));
        assert!(generated.contains("tsn_countPrimes"));
    }

    #[test]
    fn frees_nested_buffer_at_block_exit() {
        let source = "function main(): i32 { if (true) { const a: Uint8Array = new Uint8Array(2); } return 0; }";
        let generated = lower(source).expect("block-local buffers should be cleaned up");
        let allocation = generated.find("tsn_u8_array_new(2)").unwrap();
        let cleanup = generated.find("tsn_u8_array_free(a)").unwrap();
        assert!(
            cleanup > allocation,
            "buffer cleanup must follow allocation"
        );
    }

    #[test]
    fn frees_loop_scoped_buffer_before_break() {
        let source = "function main(): i32 { while (true) { const bytes: Uint8Array = new Uint8Array(8); break; } return 0; }";
        let generated = lower(source).expect("break should lower with lexical cleanup");
        let cleanup = generated.find("tsn_u8_array_free(bytes)").unwrap();
        let break_stmt = generated.find("break;").unwrap();
        assert!(
            cleanup < break_stmt,
            "loop-local resource must be freed before break"
        );
    }

    #[test]
    fn rejects_owning_buffer_alias_without_lifetime_rules() {
        let source = "function main(): i32 { const a: Uint8Array = new Uint8Array(4); const b: Uint8Array = a; return 0; }";
        let error = lower(source).expect_err("a copied handle must not be freed twice");
        assert!(error.contains("aliasing is unsupported"));
    }

    #[test]
    fn rejects_rebinding_owned_buffer_handles() {
        let source = "function main(): i32 { let a: Uint8Array = new Uint8Array(1); let b: Uint8Array = new Uint8Array(1); a = b; return 0; }";
        let error = lower(source).expect_err("rebind would make two cleanup owners alias");
        assert!(error.contains("ownership transfer is defined"));
    }

    #[test]
    fn supports_borrowed_uint8array_parameters_without_transfer() {
        let source = "function first(bytes: Uint8Array): u8 { return bytes[0]; } function main(): i32 { const bytes: Uint8Array = new Uint8Array(1); bytes[0] = 7; if (first(bytes) === 7) return 0; return 1; }";
        let generated = lower(source).expect("byte buffers may be borrowed by helper functions");
        assert!(generated.contains("uint8_t tsn_first(TsnByteArray * bytes)"));
        let helper = generated
            .split("uint8_t tsn_first")
            .nth(1)
            .expect("helper body is emitted")
            .split("int32_t tsn_main")
            .next()
            .expect("main follows helper");
        assert!(!helper.contains("tsn_u8_array_free(bytes)"));
    }

    #[test]
    fn evaluates_return_expression_before_scope_cleanup() {
        let source = "function first(bytes: Uint8Array): u8 { return bytes[0]; } function outer(): u8 { const bytes: Uint8Array = new Uint8Array(1); bytes[0] = 7; return first(bytes); } function main(): i32 { if (outer() === 7) return 0; return 1; }";
        let generated = lower(source).expect("resource-owning return should lower");
        let call = generated
            .find("tsn_first(bytes)")
            .expect("return call emitted");
        let free = generated
            .find("tsn_u8_array_free(bytes)")
            .expect("buffer cleanup emitted");
        assert!(call < free, "return value must be evaluated before cleanup");
        assert!(generated.contains("tsn_return_tmp_0 = tsn_first(bytes)"));
    }

    #[test]
    fn rejects_break_outside_loop() {
        let source = "function main(): i32 { break; return 0; }";
        let error = lower(source).expect_err("break outside a loop is invalid");
        assert!(error.contains("inside a loop"));
    }

    #[test]
    fn rejects_incompatible_initializer_types() {
        let source =
            "function main(): i32 { const ratio: f64 = 1.5; const count: u32 = ratio; return 0; }";
        let error = lower(source).expect_err("f64 must not silently narrow to u32");
        assert!(error.contains("type mismatch"), "unexpected error: {error}");
    }

    #[test]
    fn rejects_incompatible_function_arguments() {
        let source = "function identity(value: u32): u32 { return value; } function main(): i32 { return identity(1.5); }";
        let error = lower(source).expect_err("float literal must not silently narrow to u32");
        assert!(error.contains("argument"), "unexpected error: {error}");
    }

    #[test]
    fn local_binding_shadows_function_call_target() {
        let source = "function compute(): i32 { return 1; } function main(): i32 { const compute: i32 = 0; return compute(); }";
        let error = lower(source).expect_err("local value must shadow global function name");
        assert!(error.contains("shadows the callable"));
    }

    #[test]
    fn local_binding_shadows_builtin_call_target() {
        let source = "import { printString } from \"tsn:io\"; function main(): i32 { const printString: i32 = 0; printString(\"hello\"); return 0; }";
        let error = lower(source).expect_err("local value must shadow imported builtin name");
        assert!(error.contains("shadows the callable"));
    }

    #[test]
    fn rejects_mixed_integer_float_arithmetic() {
        let source = "function main(): i32 { const count: u32 = 1; const ratio: f64 = count + 1.5; return 0; }";
        let error =
            lower(source).expect_err("mixed numeric arithmetic needs an explicit conversion");
        assert!(
            error.contains("incompatible operand types"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn integer_division_promotes_operands_and_result_to_f64() {
        let source = "function main(): i32 { const result: f64 = 7 / 2; return 0; }";
        let generated = lower(source).expect("numeric division is part of the v0 scalar contract");
        assert!(generated.contains("((double)(7) / (double)(2))"));

        let source = "function main(): i32 { const result: u32 = 7 / 2; return 0; }";
        let error = lower(source).expect_err("division result must not silently narrow to u32");
        assert!(
            error.contains("expected `U32`, got `F64`"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn conditional_expressions_require_boolean_tests_and_compatible_branches() {
        let source = "function main(): i32 { const choose: bool = true; const value: i32 = choose ? 7 : 9; return value; }";
        let generated = lower(source).expect("typed conditional expressions should lower");
        assert!(generated.contains("((choose) ? (7) : (9))"));

        let source = "function main(): i32 { const choose: i32 = 1; const value: i32 = choose ? 7 : 9; return value; }";
        let error = lower(source).expect_err("conditional test must be boolean");
        assert!(
            error.contains("test must be boolean"),
            "unexpected error: {error}"
        );

        let source = "function main(): i32 { const choose: bool = true; const value: i32 = choose ? 7 : \"nine\"; return value; }";
        let error = lower(source).expect_err("conditional branches must have compatible types");
        assert!(
            error.contains("incompatible operand types"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn lowers_narrow_signed_wrap_with_explicit_bit_conversion() {
        let source = "function main(): i32 { let value: i8 = 127; const old: i8 = value++; if (old === 127 && value === -128) return 0; return 1; }";
        let generated = lower(source).expect("i8 wrapping should use explicit bit conversion");
        assert!(generated.contains("tsn_i8_wrap"));
        assert!(generated.contains("tsn_i8_post_inc(&value)"));
    }

    #[test]
    fn lowers_narrow_signed_arithmetic_with_explicit_wrap() {
        let source = "function increment(value: i8): i8 { return value + 1; } function main(): i32 { const value: i8 = increment(127); if (value === -128) return 0; return 1; }";
        let generated = lower(source).expect("i8 addition should use explicit wrapping");
        assert!(generated.contains("tsn_i8_wrap((int32_t)"));
    }

    #[test]
    fn rejects_loose_equality_with_grammar_code() {
        let source =
            "function main(): i32 { const value: u32 = 1; if (value == 1) return 0; return 1; }";
        let error = lower(source).expect_err("loose equality is forbidden by GRAMMAR.md");
        let diagnostic = Diagnostic::from_lowering_error("loose-equality.ts", source, error);
        let rendered = diagnostic.render();
        assert!(rendered.contains("error[TSN1301]"));
        assert!(rendered.contains("use `===` or `!==`"));
    }

    #[test]
    fn rejects_integer_literal_outside_declared_width() {
        let source = "function main(): i32 { const byte: u8 = 256; return 0; }";
        let error = lower(source).expect_err("256 does not fit u8");
        assert!(
            error.contains("outside the range"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn rejects_integer_literal_not_exact_in_number() {
        let source = "function main(): i32 { const value: number = 9007199254740993; return 0; }";
        let error = lower(source).expect_err("integer exceeds exact f64 precision");
        assert!(
            error.contains("exactly representable"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn accepts_signed_minimum_literal() {
        let source = "function main(): i32 { const minimum: i8 = -128; if (minimum < 0) return 0; return 1; }";
        lower(source).expect("the signed i8 minimum is representable");
    }

    #[test]
    fn rejects_negative_literal_outside_signed_width() {
        let source = "function main(): i32 { const value: i8 = -129; return 0; }";
        let error = lower(source).expect_err("-129 does not fit i8");
        assert!(error.contains("outside the range"));
    }

    #[test]
    fn accepts_hex_integer_with_e_digit_as_integer() {
        let source = "function main(): i32 { const value: u32 = 0x1e; if (value === 30) return 0; return 1; }";
        lower(source).expect("hex `e` is a digit, not a decimal exponent");
    }

    #[test]
    fn rejects_negative_integer_in_unsigned_context() {
        let source = "function main(): i32 { const value: u8 = -1; return 0; }";
        let error = lower(source).expect_err("negative one is outside u8 range");
        assert!(error.contains("outside the range"));
    }

    #[test]
    fn rejects_negative_uint8array_length_at_typecheck_time() {
        let source =
            "function main(): i32 { const bytes: Uint8Array = new Uint8Array(-1); return 0; }";
        let error = lower(source).expect_err("negative allocation length must not wrap to u32");
        assert!(error.contains("outside the range"));
    }

    #[test]
    fn rejects_negative_uint8array_index_at_typecheck_time() {
        let source = "function main(): i32 { const bytes: Uint8Array = new Uint8Array(1); const value: u8 = bytes[-1]; return 0; }";
        let error = lower(source).expect_err("negative index must not silently convert to u32");
        assert!(error.contains("outside the range"));
    }

    #[test]
    fn bitwise_not_integer_literal_has_i32_semantics() {
        let source =
            "function main(): i32 { const value: i64 = ~1; if (value === -2) return 0; return 1; }";
        let generated =
            lower(source).expect("bitwise-not literal should have defined i32 semantics");
        assert!(generated.contains("const int64_t value = (~1)"));
    }

    #[test]
    fn accepts_exact_integer_float_boundaries() {
        let source = "function main(): i32 { const f: f32 = 16777216; const d: f64 = 9007199254740992; return 0; }";
        lower(source).expect("powers of two at precision boundaries are exactly representable");
    }

    #[test]
    fn rejects_inexact_integer_float_boundary() {
        let source = "function main(): i32 { const f: f32 = 16777217; return 0; }";
        let error = lower(source).expect_err("16777217 rounds when stored in f32");
        assert!(
            error.contains("exactly representable"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn renders_type_error_code_and_source_location() {
        let source =
            "function main(): i32 { const ratio: f64 = 1.5; const value: u32 = ratio; return 0; }";
        let lowering_error = lower(source).expect_err("f64-to-u32 narrowing must fail");
        let diagnostic = Diagnostic::from_lowering_error("sample.ts", source, lowering_error);
        let rendered = diagnostic.render();
        assert!(rendered.contains("error[TSN2001]"));
        assert!(rendered.contains("sample.ts:1:"));
        assert!(rendered.contains("const value: u32 = ratio"));
        assert!(rendered.contains('^'));
    }

    #[test]
    fn diagnostic_rendering_handles_crlf_and_unicode_line_separators() {
        for (source, marker) in [
            ("first\rsecond", "sample.ts:2:1"),
            ("first\r\nsecond", "sample.ts:2:1"),
            ("first\u{2028}second", "sample.ts:2:1"),
            ("first\u{2029}second", "sample.ts:2:1"),
        ] {
            let start = source.find("second").expect("fixture contains second") as u32;
            let diagnostic = Diagnostic::new(
                "TSN2001",
                "test diagnostic",
                "sample.ts",
                source,
                Some(Span::new(start, start + 6)),
            );
            let rendered = diagnostic.render();
            assert!(rendered.contains(marker), "{rendered}");
            assert!(rendered.contains("2 | second"), "{rendered}");
        }
    }

    #[test]
    fn missing_type_annotation_points_to_function() {
        let source = "function main() { return 0; }";
        let error = lower(source).expect_err("main must have an explicit return type");
        let diagnostic = Diagnostic::from_lowering_error("missing-type.ts", source, error);
        let rendered = diagnostic.render();
        assert!(rendered.contains("error[TSN2001]"));
        assert!(rendered.contains("missing-type.ts:1:1"));
        assert!(rendered.contains("needs an explicit type annotation"));
    }

    #[test]
    fn rejects_non_void_function_without_return_on_all_paths() {
        let source = "function maybe(flag: bool): u32 { if (flag) return 1; } function main(): i32 { return 0; }";
        let error =
            lower(source).expect_err("missing return path must be rejected in the type pass");
        assert!(error.contains("may complete without returning"));
    }

    #[test]
    fn rejects_nested_block_with_conditional_return_only() {
        let source = "function maybe(flag: bool): i32 { { if (flag) return 1; } } function main(): i32 { return maybe(false); }";
        let error = lower(source).expect_err("nested conditional return must not guarantee return");
        assert!(error.contains("may complete without returning"));
    }

    #[test]
    fn accepts_non_void_function_when_both_if_branches_return() {
        let source = "function choose(flag: bool): u32 { if (flag) return 1; else return 2; } function main(): i32 { const value: u32 = choose(true); return 0; }";
        lower(source).expect("both branches return a value");
    }

    #[test]
    fn accepts_boolean_returning_function_as_condition() {
        let source = "function ready(): bool { return true; } function main(): i32 { if (!ready()) return 1; return 0; }";
        let generated = lower(source).expect("boolean-returning calls are valid conditions");
        assert!(generated.contains("if (!tsn_ready())"));
    }

    #[test]
    fn lowers_boolean_comparison_return_expression() {
        let source = "function matches(value: u32): bool { return value === 7; } function main(): i32 { if (matches(7)) return 0; return 1; }";
        let generated = lower(source).expect("boolean comparison can be returned and called");
        assert!(generated.contains("_Bool tsn_return_tmp_0 = (value == 7)"));
    }
}
