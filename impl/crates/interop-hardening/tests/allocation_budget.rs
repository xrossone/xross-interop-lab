//! 分配预算：**声明长度不得驱动分配**。
//!
//! 这是本仓"长度检查先于分配"这句话的机器版本。做法是给这个测试二进制装一个记账
//! `GlobalAlloc`，逐个输入测量解码期间的**峰值存活字节数**，再断言它落在
//! `max(64 KiB, 8 × 输入长度, 流式入口的声明上限 + 余量)` 之内。
//!
//! 为什么记"峰值存活"而不是"累计分配"：累计分配会把任何**几何增长的缓冲**（`String`/`Vec`
//! 按 2 倍扩容：1+2+4+…≈ 最终大小的 2 倍）算成放大，而那不是缺陷；峰值存活才对应
//! "输入声明了 16 MiB，于是真的占了 16 MiB"这件事。
//!
//! 为什么必须独占一个进程：`GlobalAlloc` 是进程级的，而 `cargo test` 默认为每个测试开线程；
//! 只要断言依赖"这段时间里只有我在分配"，这个二进制里就**只能有一个测试**（下面那个）。
//! 也因此本文件不跑任何其他用例——契约扫描在 `decode_campaign.rs`。
//!
//! 长度炸弹的上限刻意停在 16 MiB：如果某个解码器真的按声明长度预分配，4 GiB 的失败模式是
//! 进程 abort（不可 catch，还可能把整个测试跑拖成 OOM），而 16 MiB 足够越过 64 KiB 的门槛，
//! 拿到同样的结论而不会把测试套件变成危险的东西。

mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, Ordering};
use std::time::{Duration, Instant};

use common::targets;

/// 记账分配器：只统计**存活**字节（alloc 加、dealloc 减、realloc 按差值）与峰值。
///
/// 从进程启动就记（不设开关）：否则"窗口前分配、窗口内释放"会让存活计数漂移。
struct CountingAlloc;

static LIVE: AtomicIsize = AtomicIsize::new(0);
static PEAK: AtomicIsize = AtomicIsize::new(0);

// SAFETY: 只做转发 + 原子加减；计数溢出/漂移不影响内存正确性，指针与布局原样来自 System。
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = System.alloc(layout);
        if !ptr.is_null() {
            let live =
                LIVE.fetch_add(layout.size() as isize, Ordering::SeqCst) + layout.size() as isize;
            PEAK.fetch_max(live, Ordering::SeqCst);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size() as isize, Ordering::SeqCst);
        System.dealloc(ptr, layout);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let out = System.realloc(ptr, layout, new_size);
        if !out.is_null() {
            let delta = new_size as isize - layout.size() as isize;
            let live = LIVE.fetch_add(delta, Ordering::SeqCst) + delta;
            PEAK.fetch_max(live, Ordering::SeqCst);
        }
        out
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

/// 测量一次调用的**峰值存活增量**（调用期间相对调用前的最大增长）。
fn measure<T>(f: impl FnOnce() -> T) -> (T, usize) {
    let base = LIVE.load(Ordering::SeqCst);
    PEAK.store(base, Ordering::SeqCst);
    let out = f();
    let peak = PEAK.load(Ordering::SeqCst);
    (out, (peak - base).max(0) as usize)
}

/// 一个输入的分配上限。
fn budget(input_len: usize, stream_cap: usize) -> usize {
    const FLOOR: usize = 64 * 1024;
    input_len
        .saturating_mul(8)
        .max(FLOOR)
        .max(stream_cap + 4 * 1024)
}

#[test]
fn declared_lengths_never_drive_allocations() {
    let table = targets();
    // 先建好全部输入（基线帧与形状化大输入的分配不计入任何用例的窗口）。
    let shaped = common::big_shaped_inputs();
    let mut worst_cases: Vec<(usize, String, usize)> = Vec::new();
    let mut bombs = 0usize;

    // 1) 长度炸弹 + 全部扫描语料：逐个测量。
    for target in &table {
        let corpus = common::target_corpus(target);
        let mut worst = 0usize;
        let mut worst_input = 0usize;
        for input in &corpus.inputs {
            let (_out, peak) = measure(|| (target.probe)(input));
            if peak > worst {
                worst = peak;
                worst_input = input.len();
            }
            assert!(
                peak <= budget(input.len(), target.policy_cap_bytes),
                "{}: {} 字节输入导致 {} 字节峰值分配（上限 {}，流式声明上限 {}）——声明长度驱动了分配",
                target.label,
                input.len(),
                peak,
                budget(input.len(), target.policy_cap_bytes),
                target.policy_cap_bytes
            );
        }
        bombs += corpus.inputs.len();
        worst_cases.push((worst, target.label.to_string(), worst_input));
    }

    // 2) 形状化大输入：分配与耗时都必须跟输入成比例（粗界，防"先收集后校验/平方级"实现混进来）。
    const BIG_BOUND: Duration = Duration::from_secs(5);
    let mut big_worst: Vec<(usize, String)> = Vec::new();
    for target in &table {
        for (shape, big_full) in &shaped {
            // 截到该入口在真实调用链里能拿到的最长输入：给一个调用方早就按 64 KiB 拒掉的
            // 解析器喂 1 MiB，测的是不存在的场景。
            let big: &[u8] = if big_full.len() > target.max_input_bytes {
                &big_full[..target.max_input_bytes]
            } else {
                big_full
            };
            // 测试脚手架的成本要扣掉：str 型入口在本仓测试里先经一次
            // `from_utf8_lossy(...).into_owned()`（非 UTF-8 输入最坏膨胀 3 倍），
            // 那是测试侧转换，不是被测实现的开销。
            let harness = measure(|| {
                let s = String::from_utf8_lossy(big);
                std::hint::black_box(s.into_owned().len())
            })
            .1;
            let (_, peak) = measure(|| (target.probe)(big));
            let own = peak.saturating_sub(harness);
            // 大输入上限：输入长度 × 4 + 256 KiB（流式入口另加自己的声明上限）。
            let limit = big
                .len()
                .saturating_mul(4)
                .saturating_add(256 * 1024)
                .saturating_add(target.structural_budget_bytes)
                .saturating_add(target.policy_cap_bytes);
            assert!(
                own <= limit,
                "{}: {} 字节「{shape}」输入导致 {} 字节峰值分配（扣除测试侧转换 {harness} 字节后 {own}，上限 {limit}）",
                target.label,
                big.len(),
                peak
            );
            let (elapsed, _) = measure(|| {
                let start = Instant::now();
                (target.probe)(big);
                start.elapsed()
            });
            assert!(
                elapsed <= BIG_BOUND,
                "{}: {} 字节「{shape}」输入耗时 {:?}（上限 {BIG_BOUND:?}）——疑似超线性",
                target.label,
                big.len(),
                elapsed
            );
            big_worst.push((
                peak,
                format!("{} ← {shape}（{} B）", target.label, big.len()),
            ));
        }
    }

    worst_cases.sort_unstable_by_key(|(bytes, _, _)| std::cmp::Reverse(*bytes));
    big_worst.sort_unstable_by_key(|(bytes, _)| std::cmp::Reverse(*bytes));
    println!(
        "对抗性输入 {} 个（含长度炸弹），覆盖 {} 个解码入口；全部满足峰值分配上限",
        bombs,
        table.len()
    );
    println!("对抗性输入下峰值分配最多的三个入口：");
    for (bytes, label, input_len) in worst_cases.iter().take(3) {
        println!("  {label}: {bytes} 字节（输入 {input_len} 字节）");
    }
    println!(
        "形状化大输入（{} 种，按各入口的调用方上限截断）下峰值分配最多的三个入口：",
        shaped.len()
    );
    for (bytes, label) in big_worst.iter().take(3) {
        println!("  {label}: {bytes} 字节");
    }
}
