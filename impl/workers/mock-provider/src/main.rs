//! `mock-provider`：T13 的测试 worker（真实子进程；不实现任何协议、不联网）。
//!
//! 协议（**最小行协议**，不是产品契约）：
//! - 父进程在 stdin 写一行配置：`mode=<mode> [canary_path=<p>] [listener_path=<p>] [env_vars=A,B]`
//! - 之后保持 stdin 打开；**父进程关闭 stdin（EOF）= 关闭指令**：清理 listener 文件并退出 0。
//! - 子进程在 stdout 逐行报告：`ready` / `canary:read_ok:<n>` / `canary:denied:<err>` /
//!   `env:<VAR>:present|absent` / `exec-request:<cmd>` / `listener:<path>` / `exec-denied:<code>`。
//!
//! 存在的意义是让 T13 的验收（canary 拒绝、shell 拒绝、重启上限、无孤儿）跑在**真实进程、
//! 真实管道、真实文件**上，而不是内存 mock。

use std::io::{BufRead, Write};
use std::os::unix::net::UnixListener;

fn main() {
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    let cfg_line = match lines.next() {
        Some(Ok(l)) => l,
        _ => std::process::exit(64),
    };
    let cfg = parse_cfg(&cfg_line);
    let mode = cfg.get("mode").cloned().unwrap_or_else(|| "idle".into());

    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    // listener 必须在整个生命周期内存活（否则 bind 后立刻关闭）
    let mut listener_guard: Option<UnixListener> = None;

    match mode.as_str() {
        "idle" => {
            let _ = writeln!(out, "ready");
        }
        "crash" => {
            let _ = writeln!(out, "ready");
            let _ = out.flush();
            std::process::exit(3);
        }
        "read_canary" => {
            let path = cfg.get("canary_path").cloned().unwrap_or_default();
            match std::fs::read(&path) {
                Ok(bytes) => {
                    let _ = writeln!(out, "canary:read_ok:{}", bytes.len());
                }
                Err(e) => {
                    let _ = writeln!(out, "canary:denied:{}", e.kind());
                }
            }
            let _ = writeln!(out, "ready");
        }
        "dump_env" => {
            let vars: Vec<String> = cfg
                .get("env_vars")
                .map(|s| s.split(',').map(str::to_string).collect())
                .unwrap_or_default();
            for var in vars {
                let present = if std::env::var_os(&var).is_some() {
                    "present"
                } else {
                    "absent"
                };
                let _ = writeln!(out, "env:{var}:{present}");
            }
            let _ = writeln!(out, "ready");
        }
        "request_shell" => {
            let _ = writeln!(out, "exec-request:/bin/sh -c id");
            let _ = out.flush();
            match lines.next() {
                Some(Ok(reply)) if reply.starts_with("denied") => {
                    let code = reply.split(':').nth(1).unwrap_or("unknown");
                    let _ = writeln!(out, "exec-denied:{code}");
                }
                Some(Ok(_)) => {
                    let _ = writeln!(out, "exec-allowed");
                }
                _ => {
                    let _ = writeln!(out, "exec-no-reply");
                }
            }
        }
        "open_listener" => {
            let path = cfg.get("listener_path").cloned().unwrap_or_default();
            match UnixListener::bind(&path) {
                Ok(l) => {
                    listener_guard = Some(l);
                    let _ = writeln!(out, "listener:{path}");
                }
                Err(e) => {
                    let _ = writeln!(out, "listener-error:{e}");
                }
            }
        }
        other => {
            let _ = writeln!(out, "unknown-mode:{other}");
        }
    }
    let _ = out.flush();

    // 等待关闭指令（stdin EOF）——期间不做任何其他事
    for line in lines {
        if line.is_err() {
            break;
        }
    }

    // 关闭路径：清理 listener 文件（孤儿 listener 检查在这里成立）
    drop(listener_guard);
    if let Some(path) = cfg.get("listener_path") {
        let _ = std::fs::remove_file(path);
    }
    let _ = writeln!(out, "shutdown");
    let _ = out.flush();
}

fn parse_cfg(line: &str) -> std::collections::HashMap<String, String> {
    line.split_whitespace()
        .filter_map(|tok| tok.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}
