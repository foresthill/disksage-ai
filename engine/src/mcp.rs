//! Read-only MCP (Model Context Protocol) server over stdio.
//!
//! Exposes DiskSage's diagnosis as MCP tools so an MCP client (Claude Desktop,
//! Claude Code, …) can ask "what's eating my disk?" and get DiskSage's findings.
//!
//! READ-ONLY BY DESIGN. The server never deletes, moves, or changes anything — it
//! only measures and reports. Deletion stays in the app, behind explicit user
//! approval, to the Trash (never `rm`). DiskSage's rule is "never auto-delete", so
//! an autonomous LLM must not be able to delete the user's files through this
//! server. That is why no delete/trash tool is offered here.
//!
//! Privacy: tools return the same local metadata the app shows — sizes and paths,
//! never file contents. The stdio transport is a local subprocess, so nothing
//! leaves the machine.
//!
//! Transport: stdio, newline-delimited JSON-RPC 2.0 (the MCP stdio transport) —
//! one JSON message per line; stdout carries ONLY MCP messages, and any logging
//! goes to stderr. Spec: https://modelcontextprotocol.io/specification/2025-11-25

use std::io::{BufRead, Write};

use serde_json::{json, Value};

use crate::util::human;

/// Protocol version we advertise when the client doesn't state one. Otherwise we
/// echo the client's requested version: we implement only the stable
/// initialize/tools subset, which is unchanged across recent revisions.
const PROTOCOL_VERSION: &str = "2025-06-18";

/// A hint for the client's model: what DiskSage is and the read-only boundary.
const INSTRUCTIONS: &str = "DiskSage diagnoses what is using disk space. It is READ-ONLY: \
it never deletes or changes files. Use `disk_usage` for free space at a glance, `scan` to find \
large reclaimable items (caches, old device backups, build artifacts, VM disks) each with a \
safety rating and a suggested action, `largest_folders` to see the biggest folders, and \
`snapshots` to diagnose APFS / Time Machine local snapshots that pin disk space (why deleting \
files didn't return any). When you suggest removing something, tell the user to review and delete \
it themselves in the DiskSage app (which moves items to the Trash) — never instruct destructive \
shell commands.";

/// Run the stdio server loop until stdin closes (the client disconnected).
pub fn run() {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }
        if let Some(reply) = handle_line(&line) {
            let mut out = stdout.lock();
            // One compact JSON message per line. serde never emits a literal
            // newline inside the serialized value, so the newline framing holds
            // even when a tool's text contains line breaks (they become `\n`).
            if writeln!(out, "{reply}").is_err() {
                break;
            }
            let _ = out.flush();
        }
    }
}

/// Handle one incoming line. Returns the JSON reply to write, or `None` for a
/// notification (no `id` → no response per JSON-RPC).
fn handle_line(line: &str) -> Option<String> {
    let msg: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return Some(to_line(error(Value::Null, -32700, "parse error"))),
    };
    let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
    // No `id` → a notification (e.g. notifications/initialized). Nothing to do,
    // and we must not reply.
    let id = msg.get("id").cloned()?;
    let reply = match method {
        "initialize" => initialize_result(id, &msg),
        "ping" => ok(id, json!({})),
        "tools/list" => ok(id, json!({ "tools": tool_list() })),
        "tools/call" => tools_call(id, &msg),
        "" => error(id, -32600, "invalid request: missing method"),
        _ => error(id, -32601, &format!("method not found: {method}")),
    };
    Some(to_line(reply))
}

fn to_line(v: Value) -> String {
    serde_json::to_string(&v).unwrap_or_default()
}

fn ok(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn initialize_result(id: Value, msg: &Value) -> Value {
    let version = msg
        .get("params")
        .and_then(|p| p.get("protocolVersion"))
        .and_then(|v| v.as_str())
        .unwrap_or(PROTOCOL_VERSION)
        .to_string();
    ok(
        id,
        json!({
            "protocolVersion": version,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "disksage", "version": env!("CARGO_PKG_VERSION") },
            "instructions": INSTRUCTIONS,
        }),
    )
}

/// The read-only tool catalogue. No delete/trash tool is present by design.
fn tool_list() -> Value {
    json!([
        {
            "name": "disk_usage",
            "title": "Disk usage",
            "description": "Free vs. used space for each disk/container, plus the local snapshot count. Fast; no filesystem walk.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "scan",
            "title": "Scan for reclaimable space",
            "description": "Find large, commonly-reclaimable items (caches, old device backups, build artifacts, VM disks). Each result has a size, a safety rating (safe/low/medium/high/info) and a suggested action. Read-only; may take several seconds to a minute. Returns metadata (sizes, paths) only — never file contents.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "largest_folders",
            "title": "Largest folders",
            "description": "The biggest folders under the home directory, largest first (a 'where did it all go' view).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "limit": {
                        "type": "integer",
                        "description": "How many folders to return (default 15, max 50).",
                        "minimum": 1,
                        "maximum": 50
                    }
                }
            }
        },
        {
            "name": "snapshots",
            "title": "Snapshot diagnosis",
            "description": "List and classify local APFS / Time Machine snapshots and flag the 'pinned' ones that hold space freeing files won't return. Explains why deleting files didn't free space. Read-only; macOS only (n/a elsewhere).",
            "inputSchema": { "type": "object", "properties": {} }
        }
    ])
}

fn tools_call(id: Value, msg: &Value) -> Value {
    let params = msg.get("params");
    let name = params.and_then(|p| p.get("name")).and_then(|n| n.as_str());
    let args = params.and_then(|p| p.get("arguments"));
    match name {
        Some("disk_usage") => text_result(id, disk_usage_text()),
        Some("scan") => text_result(id, scan_text()),
        Some("largest_folders") => {
            let limit = args
                .and_then(|a| a.get("limit"))
                .and_then(|v| v.as_u64())
                .unwrap_or(15)
                .clamp(1, 50) as usize;
            text_result(id, largest_folders_text(limit))
        }
        Some("snapshots") => text_result(id, crate::snapshots::report_text()),
        // Per the spec, failing to *find* the tool is an MCP protocol error (not a
        // tool result with isError) so the client knows the call itself was wrong.
        Some(other) => error(id, -32602, &format!("unknown tool: {other}")),
        None => error(id, -32602, "missing tool name"),
    }
}

/// A successful tool result: one text content block.
fn text_result(id: Value, text: String) -> Value {
    ok(
        id,
        json!({ "content": [ { "type": "text", "text": text } ], "isError": false }),
    )
}

fn disk_usage_text() -> String {
    let mut out = String::new();
    for (total, members) in crate::df::containers().iter().rev() {
        let free = members.iter().map(|m| m.avail).max().unwrap_or(0);
        let used = total.saturating_sub(free);
        let pct = if *total > 0 { used * 100 / total } else { 0 };
        out.push_str(&format!(
            "{}: used {} / {} ({pct}%) · free {}\n",
            crate::df::label_for(members),
            human(used),
            human(*total),
            human(free),
        ));
    }
    match crate::df::snapshot_count() {
        Some(0) => out.push_str("Local snapshots: none"),
        Some(n) => out.push_str(&format!(
            "Local snapshots: {n} — each pins recently-deleted blocks, so free space can shrink on its own."
        )),
        None => out.push_str("Local snapshots: n/a on this platform"),
    }
    out
}

fn scan_text() -> String {
    let mut findings = crate::scan::collect();
    if findings.is_empty() {
        return "✅ All clear — nothing is over the size threshold.".to_string();
    }
    // Biggest first so the top reclaim candidates lead.
    findings.sort_by_key(|f| std::cmp::Reverse(f.size));
    let mut out = format!("{} findings (biggest first):\n\n", findings.len());
    for f in &findings {
        out.push_str(&format!(
            "[{}] {}\n  path: {}\n  action: {}\n\n",
            f.severity, f.description, f.path, f.action,
        ));
    }
    out.push_str(
        "Deletion is not available through this server by design. Review and remove items in the \
         DiskSage app, which moves them to the Trash (recoverable) — never delete them outright.",
    );
    out
}

fn largest_folders_text(limit: usize) -> String {
    let rows = crate::top::biggest_folders(limit);
    if rows.is_empty() {
        return "No folders found.".to_string();
    }
    let mut out = String::new();
    for (path, bytes) in rows {
        out.push_str(&format!("{:>10}  {}\n", human(bytes), path));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parse the reply to a single request line back into JSON.
    fn call(line: &str) -> Value {
        let s = handle_line(line).expect("a reply");
        serde_json::from_str(&s).expect("valid json reply")
    }

    #[test]
    fn initialize_echoes_version_and_advertises_tools() {
        let v = call(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"c","version":"1"}}}"#,
        );
        assert_eq!(v["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(v["result"]["serverInfo"]["name"], "disksage");
        assert!(v["result"]["capabilities"]["tools"].is_object());
    }

    #[test]
    fn tools_list_is_read_only() {
        let v = call(r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#);
        let names: Vec<String> = v["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        for want in ["disk_usage", "scan", "largest_folders", "snapshots"] {
            assert!(names.iter().any(|n| n == want), "missing {want}");
        }
        // The safety invariant: nothing that could delete or move files.
        assert!(
            !names
                .iter()
                .any(|n| n.contains("delete") || n.contains("trash") || n.contains("remove")),
            "a mutating tool leaked into the MCP surface: {names:?}"
        );
    }

    #[test]
    fn notification_without_id_gets_no_reply() {
        assert!(handle_line(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none());
    }

    #[test]
    fn unknown_method_is_method_not_found() {
        let v = call(r#"{"jsonrpc":"2.0","id":3,"method":"resources/list"}"#);
        assert_eq!(v["error"]["code"], -32601);
    }

    #[test]
    fn unknown_tool_is_invalid_params() {
        let v = call(
            r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"rm_rf","arguments":{}}}"#,
        );
        assert_eq!(v["error"]["code"], -32602);
    }

    #[test]
    fn bad_json_is_parse_error() {
        let v = call("not json at all");
        assert_eq!(v["error"]["code"], -32700);
    }
}
