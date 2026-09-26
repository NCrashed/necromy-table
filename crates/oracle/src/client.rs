//! The smallest client for a local `llama-server`: one POST to
//! `/v1/chat/completions`, one JSON reply. Written on `std::net`, like the
//! first-person game's, because it talks to one server we run ourselves.
//!
//! Replies are not streamed: the gods answer in JSON bound by a schema, and
//! a half-read object is worth nothing. `llama-server` turns the schema into
//! a grammar, so even a small model cannot answer out of shape.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// A server that is not running refuses at once; a cold one may chew on the
/// prompt for a while before answering.
const CONNECT: Duration = Duration::from_secs(2);
const READ: Duration = Duration::from_secs(180);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Role {
    System,
    User,
}

#[derive(Clone, Debug)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Message {
            role: Role::System,
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Message {
            role: Role::User,
            content: content.into(),
        }
    }
}

/// Is a server answering at `addr`? A quick `GET /health`.
pub fn alive(addr: &str) -> bool {
    let Ok(sock) = addr.parse() else {
        return false;
    };
    let Ok(stream) = TcpStream::connect_timeout(&sock, CONNECT) else {
        return false;
    };
    stream.set_read_timeout(Some(CONNECT)).ok();
    let request = format!("GET /health HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
    if (&stream).write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut reader = BufReader::new(&stream);
    matches!(read_head(&mut reader), Ok((200, _, _)))
}

/// Ask for a reply. With a `schema`, the reply is JSON of that shape.
pub fn chat(
    addr: &str,
    messages: &[Message],
    schema: Option<&serde_json::Value>,
    max_tokens: u32,
    temperature: f32,
) -> Result<String, String> {
    let lines: Vec<serde_json::Value> = messages
        .iter()
        .map(|m| {
            let role = match m.role {
                Role::System => "system",
                Role::User => "user",
            };
            serde_json::json!({ "role": role, "content": m.content })
        })
        .collect();
    let mut body = serde_json::json!({
        "messages": lines,
        "stream": false,
        "max_tokens": max_tokens,
        "temperature": temperature,
    });
    if let Some(schema) = schema {
        body["response_format"] = serde_json::json!({
            "type": "json_schema",
            "json_schema": { "name": "answer", "schema": schema },
        });
    }
    let body = body.to_string();

    let stream = TcpStream::connect_timeout(
        &addr.parse().map_err(|_| format!("bad address {addr}"))?,
        CONNECT,
    )
    .map_err(|e| format!("no server at {addr}: {e}"))?;
    stream.set_read_timeout(Some(READ)).ok();
    let request = format!(
        "POST /v1/chat/completions HTTP/1.1\r\n\
         Host: {addr}\r\n\
         Content-Type: application/json\r\n\
         Connection: close\r\n\
         Content-Length: {}\r\n\r\n{body}",
        body.len(),
    );
    (&stream)
        .write_all(request.as_bytes())
        .map_err(|e| format!("cannot ask: {e}"))?;

    let mut reader = BufReader::new(&stream);
    let (status, chunked, length) = read_head(&mut reader)?;
    let raw = if chunked {
        read_chunked(&mut reader)?
    } else {
        let mut raw = Vec::new();
        match length {
            Some(n) => {
                raw.resize(n, 0);
                reader
                    .read_exact(&mut raw)
                    .map_err(|e| format!("cut off: {e}"))?;
            }
            None => {
                reader
                    .read_to_end(&mut raw)
                    .map_err(|e| format!("cut off: {e}"))?;
            }
        }
        raw
    };
    let text = String::from_utf8_lossy(&raw);
    if status != 200 {
        return Err(format!(
            "server said {status}: {}",
            text.chars().take(200).collect::<String>()
        ));
    }
    let reply: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| format!("not json: {text}"))?;
    reply["choices"][0]["message"]["content"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| format!("no content in {text}"))
}

fn read_head(reader: &mut impl BufRead) -> Result<(u32, bool, Option<usize>), String> {
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|e| format!("no answer: {e}"))?;
    let status: u32 = line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| format!("not http: {}", line.trim()))?;
    let (mut chunked, mut length) = (false, None);
    loop {
        let mut header = String::new();
        reader
            .read_line(&mut header)
            .map_err(|e| format!("cut off in headers: {e}"))?;
        let header = header.trim();
        if header.is_empty() {
            break;
        }
        let lower = header.to_ascii_lowercase();
        if let Some(value) = lower.strip_prefix("transfer-encoding:") {
            chunked = value.contains("chunked");
        }
        if let Some(value) = lower.strip_prefix("content-length:") {
            length = value.trim().parse().ok();
        }
    }
    Ok((status, chunked, length))
}

fn read_chunked(reader: &mut impl BufRead) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    loop {
        let mut size = String::new();
        reader
            .read_line(&mut size)
            .map_err(|e| format!("cut off: {e}"))?;
        let size = usize::from_str_radix(size.trim().split(';').next().unwrap_or("").trim(), 16)
            .map_err(|_| format!("bad chunk size {}", size.trim()))?;
        if size == 0 {
            return Ok(body);
        }
        let mut chunk = vec![0u8; size + 2];
        reader
            .read_exact(&mut chunk)
            .map_err(|e| format!("cut off: {e}"))?;
        body.extend_from_slice(&chunk[..size]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// A one-shot fake server that answers with `reply` and returns what it
    /// was asked.
    fn serve(reply: String) -> (String, std::thread::JoinHandle<String>) {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap().to_string();
        let handle = std::thread::spawn(move || {
            let (mut socket, _) = server.accept().unwrap();
            let mut request = vec![0u8; 65536];
            let read = socket.read(&mut request).unwrap();
            socket.write_all(reply.as_bytes()).unwrap();
            String::from_utf8_lossy(&request[..read]).to_string()
        });
        (addr, handle)
    }

    #[test]
    fn asks_with_a_schema_and_reads_the_content() {
        let body =
            serde_json::json!({"choices": [{"message": {"content": "{\"ok\":1}"}}]}).to_string();
        let reply = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let (addr, server) = serve(reply);
        let schema = serde_json::json!({"type": "object"});
        let got = chat(&addr, &[Message::user("hi")], Some(&schema), 50, 0.5).unwrap();
        assert_eq!(got, "{\"ok\":1}");
        let request = server.join().unwrap();
        assert!(request.starts_with("POST /v1/chat/completions"));
        assert!(request.contains("\"json_schema\""));
    }

    #[test]
    fn reads_chunked_replies() {
        let body = serde_json::json!({"choices": [{"message": {"content": "слово"}}]}).to_string();
        let reply = format!(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{body}\r\n0\r\n\r\n",
            body.len()
        );
        let (addr, _server) = serve(reply);
        assert_eq!(chat(&addr, &[], None, 10, 0.5).unwrap(), "слово");
    }

    #[test]
    fn nobody_home_is_an_error_not_a_hang() {
        assert!(chat("127.0.0.1:9", &[], None, 8, 0.5).is_err());
        assert!(!alive("127.0.0.1:9"));
    }
}
