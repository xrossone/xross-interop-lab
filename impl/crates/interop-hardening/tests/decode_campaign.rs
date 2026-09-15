//! 不可信输入的确定性扫描（目标表见 `tests/common/mod.rs`）。
//!
//! 断言（细节见 `interop_testkit::adversarial` 模块文档）：
//! 1. 任意字节都不 panic——dev profile 下算术溢出检查是开的，"长度 + 偏移"回绕会直接炸；
//! 2. `Ok` 时消费长度落在 `(0, 输入长度]`；
//! 3. 基线帧必须被自己的解码器接受（编码器/解码器脱节会先在这里炸）。
//!
//! 分配量与"声明长度不得驱动分配"另见 `allocation_budget.rs`。

mod common;

use common::{targets, target_corpus, Target, ALWAYS_REJECTS};
use interop_testkit::adversarial::{scan, Outcome, ScanReport};

/// 收集器：跑完所有目标再一起断言（避免第一个坏目标挡住后面的报告）。
#[derive(Default)]
struct Campaign {
    reports: Vec<ScanReport>,
}

impl Campaign {
    fn run(&mut self, target: &Target) {
        let c = target_corpus(target);
        let report = scan(&c, |b| (target.probe)(b));
        if let Some((_, reason)) = ALWAYS_REJECTS.iter().find(|(l, _)| *l == target.label) {
            assert_eq!(
                report.accepted, 0,
                "{}: 只允许整体拒绝（{reason}），却有 {} 个输入被接受",
                target.label, report.accepted
            );
        }
        self.reports.push(report);
    }

    fn finish(self) {
        let mut total = 0usize;
        for r in &self.reports {
            total += r.cases;
            println!("{}", r.summary());
        }
        println!(
            "总计 {total} 个对抗性用例，覆盖 {} 个解码入口",
            self.reports.len()
        );
        // 先打印全部摘要（含 digest），再断言：失败时报告里已经带齐复现信息。
        for r in &self.reports {
            r.assert_clean();
        }
    }
}

/// 目标表的标签必须与 `SCAN_TARGETS`（lib.rs）逐字一致——gate 用那份清单做完整性检查，
/// 两份漂移会让"覆盖了哪些入口"变成一句无法核对的话。
#[test]
fn scan_targets_match_table() {
    let table: Vec<&str> = targets().iter().map(|t| t.label).collect();
    let declared = interop_hardening::SCAN_TARGETS;
    let mut missing: Vec<&str> = table
        .iter()
        .copied()
        .filter(|l| !declared.contains(l))
        .collect();
    let mut extra: Vec<&str> = declared
        .iter()
        .copied()
        .filter(|l| !table.contains(l))
        .collect();
    missing.sort_unstable();
    extra.sort_unstable();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "SCAN_TARGETS 与目标表不一致：表里有而清单没有 {missing:?}；清单有而表里没有 {extra:?}"
    );
    assert_eq!(table.len(), declared.len(), "标签数不一致");
    // 标签唯一性（否则 digest 的 label 前缀会撞在一起）
    let mut sorted = table.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), table.len(), "目标表里有重复标签");
}

/// 每个目标先跑一遍"基线必须被接受"，再跑扫描。
#[test]
fn baselines_are_accepted_by_their_own_decoders() {
    for target in targets() {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            (target.probe)(&target.base)
        }));
        if let Some((_, reason)) = ALWAYS_REJECTS.iter().find(|(l, _)| *l == target.label) {
            // 这类入口的基线**必须**被拒绝：能接受才是缺陷。
            match outcome {
                Ok(Outcome::Rejected) => {}
                Ok(Outcome::Accepted) => panic!(
                    "{}: 竟然接受了基线输入——{reason}",
                    target.label
                ),
                Err(_) => panic!("{}: 基线帧让解码器 panic", target.label),
            }
            continue;
        }
        match outcome {
            Ok(Outcome::Accepted) => {}
            Ok(Outcome::Rejected) => panic!(
                "{}: 基线帧被自己的解码器拒绝——编码器/解码器脱节，或基线不是合法帧",
                target.label
            ),
            Err(_) => panic!("{}: 基线帧让解码器 panic", target.label),
        }
    }
}

fn campaign_for(prefix: &str) {
    let mut c = Campaign::default();
    let mut seen = 0usize;
    for target in targets() {
        if target.label.starts_with(prefix) {
            c.run(&target);
            seen += 1;
        }
    }
    assert!(seen > 0, "前缀 {prefix} 没有匹配任何目标");
    c.finish();
}

#[test]
fn quickshare_wire_campaign() {
    campaign_for("quickshare/wire/");
}

#[test]
fn quickshare_control_campaign() {
    campaign_for("quickshare/control/");
}

#[test]
fn quickshare_payload_and_framing_campaign() {
    campaign_for("quickshare/payload/");
    campaign_for("quickshare/framing/");
}

#[test]
fn airplay_campaign() {
    campaign_for("airplay/");
}

#[test]
fn upnp_campaign() {
    campaign_for("upnp/");
}

#[test]
fn cast_campaign() {
    campaign_for("cast/");
}

#[test]
fn wfd_campaign() {
    campaign_for("wfd/");
}

#[test]
fn ipc_campaign() {
    campaign_for("ipc/");
}

#[test]
fn contract_campaign() {
    campaign_for("contract/");
}
