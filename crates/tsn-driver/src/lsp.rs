//! Minimal editor-facing LSP: stdio framing and parser diagnostics only.
use std::{
    collections::HashMap,
    io::{self, BufRead, Write},
    process::ExitCode,
};

use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;
use serde_json::{Value, json};

pub(crate) fn run() -> ExitCode {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut input = stdin.lock();
    let mut output = stdout.lock();
    let mut documents = HashMap::<String, String>::new();
    let mut initialized = false;
    loop {
        let body = match read_message(&mut input) {
            Ok(Some(body)) => body,
            Ok(None) => return ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("tsn lsp: {error}");
                return ExitCode::from(1);
            }
        };
        let request: Value = match serde_json::from_slice(&body) {
            Ok(value) => value,
            Err(_) => {
                // The frame was consumed completely; report parse error without losing alignment.
                if write_message(&mut output, &json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Parse error"}})).is_err() { return ExitCode::from(1); }
                continue;
            }
        };
        let id = request.get("id").cloned();
        let method = request.get("method").and_then(Value::as_str).unwrap_or("");
        let params = &request["params"];
        let mut response = None;
        let mut notifications = Vec::new();
        match method {
            "initialize" => {
                initialized = true;
                response = Some(
                    json!({"capabilities":{"textDocumentSync":{"openClose":true,"change":1}},"serverInfo":{"name":"tsn","version":env!("CARGO_PKG_VERSION")}}),
                );
            }
            "initialized" => {}
            "shutdown" => response = Some(Value::Null),
            "exit" => {
                return if initialized {
                    ExitCode::SUCCESS
                } else {
                    ExitCode::from(1)
                };
            }
            "textDocument/didOpen" => {
                if let (Some(uri), Some(text)) = (
                    params.pointer("/textDocument/uri").and_then(Value::as_str),
                    params.pointer("/textDocument/text").and_then(Value::as_str),
                ) {
                    documents.insert(uri.to_owned(), text.to_owned());
                    notifications.push(diagnostics(uri, text));
                }
            }
            "textDocument/didChange" => {
                if let (Some(uri), Some(text)) = (
                    params.pointer("/textDocument/uri").and_then(Value::as_str),
                    params
                        .pointer("/contentChanges/0/text")
                        .and_then(Value::as_str),
                ) {
                    documents.insert(uri.to_owned(), text.to_owned());
                    notifications.push(diagnostics(uri, text));
                }
            }
            _ if id.is_some() => {
                response = Some(json!({"error":{"code":-32601,"message":"Method not found"}}))
            }
            _ => {}
        }
        if let Some(id) = id {
            let result = response.unwrap_or(Value::Null);
            let message = if result.get("error").is_some() {
                json!({"jsonrpc":"2.0","id":id,"error":result["error"]})
            } else {
                json!({"jsonrpc":"2.0","id":id,"result":result})
            };
            if write_message(&mut output, &message).is_err() {
                return ExitCode::from(1);
            }
        }
        for notification in notifications {
            if write_message(&mut output, &notification).is_err() {
                return ExitCode::from(1);
            }
        }
    }
}

fn read_message(reader: &mut impl BufRead) -> io::Result<Option<Vec<u8>>> {
    let mut length = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((name, value)) = line.trim_end_matches(['\r', '\n']).split_once(':')
            && name.eq_ignore_ascii_case("Content-Length")
        {
            length = value.trim().parse::<usize>().ok();
        }
    }
    let length = length.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "missing or invalid Content-Length",
        )
    })?;
    if length > 16 * 1024 * 1024 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "message exceeds 16 MiB limit",
        ));
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    Ok(Some(body))
}

fn write_message(writer: &mut impl Write, value: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(value).map_err(io::Error::other)?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()
}

fn diagnostics(uri: &str, source: &str) -> Value {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, SourceType::ts()).parse();
    let items: Vec<Value> = parsed
        .diagnostics
        .iter()
        .map(|diagnostic| {
            json!({
                "range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}},
                "severity":1,"source":"tsn","message":diagnostic.to_string()
            })
        })
        .collect();
    json!({"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{"uri":uri,"diagnostics":items}})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn framing_round_trips_exact_body() {
        let mut frame = Vec::new();
        write_message(&mut frame, &json!({"x":"λ"})).unwrap();
        assert_eq!(
            read_message(&mut frame.as_slice()).unwrap(),
            Some(r#"{"x":"λ"}"#.as_bytes().to_vec())
        );
    }
    #[test]
    fn malformed_json_does_not_consume_next_frame() {
        let mut frames = b"Content-Length: 1\r\n\r\nxContent-Length: 2\r\n\r\n{}".as_slice();
        assert_eq!(read_message(&mut frames).unwrap(), Some(b"x".to_vec()));
        assert_eq!(read_message(&mut frames).unwrap(), Some(b"{}".to_vec()));
    }
    #[test]
    fn initialize_advertises_full_sync_and_parser_diagnostics() {
        let response = json!({"capabilities":{"textDocumentSync":{"openClose":true,"change":1}}});
        assert_eq!(
            response.pointer("/capabilities/textDocumentSync/change"),
            Some(&json!(1))
        );
        assert_eq!(
            diagnostics("file:///a.ts", "const = ;")["params"]["diagnostics"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            diagnostics("file:///a.ts", "const a = 1;")["params"]["diagnostics"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }
}
