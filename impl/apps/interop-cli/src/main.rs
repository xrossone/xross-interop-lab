//! `xinterop` 诊断 CLI（plans/01 T12 起）。
//!
//! 用法：
//! ```text
//! xinterop doctor [--json]
//! ```
//! `doctor` 只做只读平台探测（probe 白名单命令、无提权、无网络变更），
//! 输出 `not-run`/`unavailable` 字段时如实带原因——不把未验证的东西显示成可用。

use interop_platform::probe::{probe_local, PlatformReport, ProbeValue};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("doctor") => {
            let report = probe_local();
            if args.iter().any(|a| a == "--json") {
                match serde_json::to_string_pretty(&report) {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        eprintln!("序列化失败：{e}");
                        std::process::exit(1);
                    }
                }
            } else {
                print_human(&report);
            }
        }
        Some(other) => {
            eprintln!("未知子命令 {other:?}");
            usage();
            std::process::exit(2);
        }
        None => {
            usage();
            std::process::exit(2);
        }
    }
}

fn usage() {
    eprintln!("用法：xinterop doctor [--json]");
}

fn print_human(r: &PlatformReport) {
    println!("xinterop doctor — 只读平台报告");
    println!("os/arch           : {}/{}", r.os, r.arch);
    println!(
        "network_interfaces: {}",
        match &r.network_interfaces {
            ProbeValue::Observed { value } => value.join(", "),
            ProbeValue::NotRun { why } => format!("not-run（{why}）"),
            ProbeValue::Unavailable { why } => format!("unavailable（{why}）"),
        }
    );
    println!("wifi_p2p          : {}", level(&r.wifi_p2p));
    println!("wifi_display      : {}", level(&r.wifi_display));
    println!(
        "media_outputs     : {}",
        match &r.media_outputs {
            ProbeValue::Observed { value } => value.join(", "),
            ProbeValue::NotRun { why } | ProbeValue::Unavailable { why } => {
                format!("not-run（{why}）")
            }
        }
    );
    println!(
        "interactive       : {}",
        match &r.interactive_session {
            ProbeValue::Observed { value } => value.to_string(),
            ProbeValue::NotRun { why } | ProbeValue::Unavailable { why } => {
                format!("not-run（{why}）")
            }
        }
    );
    println!("permissions       : {}", level(&r.permissions));
    println!("commands          :");
    for c in &r.commands {
        println!(
            "  - {} {} (exit {:?}, {} lines)",
            c.program,
            c.args.join(" "),
            c.exit_code,
            c.lines
        );
    }
}

fn level(v: &ProbeValue<interop_platform::probe::SupportLevel>) -> String {
    match v {
        ProbeValue::Observed { value } => format!("{value:?}"),
        ProbeValue::NotRun { why } => format!("not-run（{why}）"),
        ProbeValue::Unavailable { why } => format!("unavailable（{why}）"),
    }
}
