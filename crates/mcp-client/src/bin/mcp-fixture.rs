//! Deterministic stdio test fixture; never launched by the product.
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::args().nth(1).unwrap_or_default();
    // A descendant intentionally outlives stdin EOF; the manager's Job Object must own it.
    let descendant = if mode == "tree" {
        Some(
            std::process::Command::new(std::env::current_exe()?)
                .arg("hang")
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()?,
        )
    } else {
        None
    };
    let stdout = io::stdout();
    let mut out = stdout.lock();
    for line in io::stdin().lock().lines() {
        let value: Value = serde_json::from_str(&line?)?;
        let method = value["method"].as_str().unwrap_or_default();
        if mode == "call-hang"
            && method == "notifications/cancelled"
            && let Some(address) = std::env::args().nth(2)
        {
            let mut stream = std::net::TcpStream::connect(address)?;
            stream.write_all(b"cancelled")?;
        }
        let Some(id) = value.get("id") else {
            continue;
        };
        if mode == "crash" {
            std::process::exit(7);
        }
        if mode == "hang" {
            continue;
        }
        if mode == "oversize" {
            writeln!(out, "{}", "x".repeat(1024 * 1024 + 1))?;
            out.flush()?;
            continue;
        }
        let result = match method {
            "initialize" => {
                json!({"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":"deskaide-fixture","version":"1"}})
            }
            "tools/list" => {
                if mode == "paginate" && value["params"]["cursor"].is_null() {
                    json!({"tools":[{"name":"first","description":"first","inputSchema":{"type":"object"}}],"nextCursor":"next"})
                } else {
                    json!({"tools":[{"name":"echo","description":"echo fixture","inputSchema":{"type":"object"}}]})
                }
            }
            "tools/call" => {
                if mode == "call-hang" {
                    if let Some(address) = std::env::args().nth(2) {
                        let mut stream = std::net::TcpStream::connect(address)?;
                        stream.write_all(b"started")?;
                    }
                    continue;
                }
                if mode == "call-crash" {
                    std::process::exit(9);
                }
                eprintln!("fixture-secret-must-not-be-logged");
                json!({"content":[{"type":"text","text":value["params"]["arguments"].to_string()}],"structuredContent":{"pid":std::process::id(),"childPid":descendant.as_ref().map(std::process::Child::id),"arguments":value["params"]["arguments"]},"isError":false})
            }
            _ => json!({}),
        };
        writeln!(out, "{}", json!({"jsonrpc":"2.0","id":id,"result":result}))?;
        out.flush()?;
    }
    Ok(())
}
