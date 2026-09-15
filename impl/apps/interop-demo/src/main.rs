//! headless demo CLI（docs/goal-phase-3.md T5）。
//!
//! 用法：
//! ```text
//! xinterop-demo [run|discovery|session|sink|media|qshare|mirror|dlna|wfd|cast] [--json]
//! ```
//! 退出码：0 = 全部场景断言通过；1 = 场景断言未通过；2 = 用法错误。

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (section, json) = match interop_demo::parse_args(&args) {
        Ok(v) => v,
        Err(why) => {
            if why == "help" {
                usage();
                std::process::exit(0);
            }
            eprintln!("{why}");
            usage();
            std::process::exit(2);
        }
    };

    let report = interop_demo::compute_report();
    if json {
        match serde_json::to_string_pretty(&report) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("序列化失败：{e}");
                std::process::exit(1);
            }
        }
    } else {
        print!("{}", interop_demo::render_human(&report, section));
    }

    if !report.ok {
        eprintln!("demo 场景存在未通过断言（ok=false）");
        std::process::exit(1);
    }
}

fn usage() {
    eprintln!("用法：xinterop-demo [run|discovery|session|sink|media|qshare|mirror|dlna|wfd|cast] [--json]");
    eprintln!("  run（默认）：全部场景 + blocked + 待用户手动清单");
    eprintln!("  --json     ：输出结构化报告（同一份数据）");
}
