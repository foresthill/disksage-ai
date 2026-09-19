// DiskSage engine CLI — a thin front-end over the `disksage_engine` library.
//
//   disksage-engine df   [--json]
//   disksage-engine scan [--json]
//
// The real logic lives in the library (df / scan / util / walk) so the desktop
// app can call it in-process too.

use disksage_engine::df::{self, Volume};
use disksage_engine::util::human;
use disksage_engine::{scan, serve};

fn cmd_df_human() {
    println!("## Current Disk Usage\n");
    for (total, members) in df::containers().iter().rev() {
        let free = members.iter().map(|m| m.avail).max().unwrap_or(0);
        let used = total.saturating_sub(free);
        let pct = if *total > 0 { used * 100 / total } else { 0 };
        let bar_len = 24u64;
        let fill = ((pct as f64) / 100.0 * bar_len as f64).round() as u64;
        let bar: String = "█".repeat(fill as usize) + &"░".repeat((bar_len - fill) as usize);
        println!("### {}", df::label_for(members));
        println!(
            "`{}`  used {} / {} ({}%) · free {}",
            bar,
            human(used),
            human(*total),
            pct,
            human(free)
        );
        if members.len() > 1 {
            println!("\n_Volumes below share this container's free space:_");
            let mut sorted: Vec<&Volume> = members.iter().collect();
            sorted.sort_by(|a, b| b.used.cmp(&a.used));
            for m in sorted {
                println!("- {} — {}", m.mount, human(m.used));
            }
        }
        println!();
    }
    match df::snapshot_count() {
        Some(0) => println!("Local snapshots: none"),
        Some(n) => println!(
            "Local snapshots: {n} — each one pins recently-deleted blocks, so free space can shrink on its own."
        ),
        None => println!("Local snapshots: n/a on this platform"),
    }
}

fn cmd_df_json() {
    // Deliberately dependency-free JSON so the PoC stays tiny; enough to diff.
    let mut items = String::new();
    for (total, members) in df::containers().iter().rev() {
        let free = members.iter().map(|m| m.avail).max().unwrap_or(0);
        let used = total.saturating_sub(free);
        let pct = if *total > 0 { used * 100 / total } else { 0 };
        if !items.is_empty() {
            items.push(',');
        }
        items.push_str(&format!(
            "{{\"label\":\"{}\",\"total\":{},\"used\":{},\"free\":{},\"pct\":{}}}",
            df::label_for(members).replace('"', "\\\""),
            total,
            used,
            free,
            pct
        ));
    }
    let snaps = match df::snapshot_count() {
        Some(n) => n.to_string(),
        None => "null".to_string(),
    };
    println!("{{\"containers\":[{items}],\"snapshots\":{snaps}}}");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let json = args.iter().any(|a| a == "--json");
    match args.first().map(String::as_str) {
        Some("df") => {
            if json {
                cmd_df_json();
            } else {
                cmd_df_human();
            }
        }
        Some("scan") => scan::run(json),
        Some("serve") => {
            let port = args
                .iter()
                .position(|a| a == "--port")
                .and_then(|i| args.get(i + 1))
                .and_then(|s| s.parse::<u16>().ok())
                .unwrap_or(8765);
            if let Err(e) = serve::run(port) {
                eprintln!("serve: {e}");
                std::process::exit(1);
            }
        }
        Some("--version") | Some("-v") => {
            println!("disksage-engine {}", env!("CARGO_PKG_VERSION"));
        }
        _ => {
            eprintln!(
                "disksage-engine (PoC)\n\nUsage:\n  disksage-engine df [--json]\n  disksage-engine scan [--json]\n  disksage-engine serve [--port N]\n  disksage-engine --version"
            );
            std::process::exit(2);
        }
    }
}
