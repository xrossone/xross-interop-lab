//! 对抗性输入扫描（自撰语料；确定性、零依赖）。
//!
//! 这**不是**模糊测试器：没有覆盖率反馈、没有外部工具、不引入第三方语料或依赖。
//! 它是一组**固定 seed 的确定性输入**，把三条断言变成机器检查：
//!
//! 1. 任意字节都不 panic——`cargo test` 走 dev profile，算术溢出检查是开的，
//!    因此"长度字段 + 偏移"这类会静默回绕的算术在这里会直接炸出来；
//! 2. `Ok` 时消费长度必须落在 `(0, 输入长度]`——返回 0 会让"按帧循环"的实现原地死循环
//!    （[`expect_consumed`]）；
//! 3. 分配量与输入**成比例**，声明长度不得驱动分配（计数分配器见 `interop-hardening`
//!    的 `allocation_budget` 用例）。
//!
//! 语料算法与 digest 记入 evidence（fixture 登记纪律：记录生成算法 + hash，不静默重录）。
//!
//! **这些不变量证明的是"没有崩、没有越界记账、没有被长度字段骗着分配"，不是"实现正确"。**
//! 协议语义仍由各 profile 的 gate 字段行与语料用例负责；两者不可互相替代。
//!
//! 极值取 **16 MiB 一族**（而非 `u32::MAX`）：如果某个解码器真的按声明长度预分配，
//! 4 GiB 的失败模式是**进程 abort**（`catch_unwind` 抓不到，测试套件可能被 OOM 杀掉），
//! 而 16 MiB 已经足够让任何"长度先于检查"的实现越过 64 KiB 的分配阈值——
//! 用可存活的失败模式拿到同样的结论。

use crate::peer::XorShift64Star;
use sha2::{Digest, Sha256};

/// 结构化字节的偏置字母表：协议分隔符 + varint 高位 + 文本极值 + 数字/字母。
/// 目的是让随机输入落在**解析器的分支上**，而不是均匀噪声里。
const STRUCTURED_ALPHABET: &[u8] = b"\x00\x01\x02\x03\x7f\x80\x81\xff\r\n\t :/.\"=<>&{}[](),;09aAzZ-_%+\\*?";

/// 单字节极值。
const EXTREME_BYTES: &[u8] = &[0x00, 0x01, 0x7f, 0x80, 0x81, 0xfe, 0xff];

/// 4 字节长度窗口的极值（大端/小端 × 巨值/零/一）。见模块文档：上限刻意停在 16 MiB。
const EXTREME_LENS: [[u8; 4]; 5] = [
    [0x01, 0x00, 0x00, 0x00], // 16 MiB 大端
    [0x00, 0x00, 0x00, 0x01], // 16 MiB 小端
    [0x00, 0x00, 0x00, 0x00], // 零：最容易触发"长度 0 + 减一"的下溢
    [0x00, 0x00, 0x00, 0x01], // 一
    [0xff, 0xff, 0xff, 0xff], // 全 1：无符号回绕 / 有符号 -1
];

/// 边界填充的重复单元。
const BOUNDARY_UNITS: &[&[u8]] = &[
    b"\x00",
    b"\xff",
    b"\xaa\x55",
    b"\x80",                       // varint 续位串：长度解码器的最坏输入
    b"\x7f",                       // varint 单字节最大值
    b"\r\n\r\n",                   // 头部终止
    b"<a>",                        // XML 元素
    b"\x00\x00\x00\x01",           // 大端长
    b"\x00\x10\x00\x00",           // 16 MiB（见模块文档）
    b"Content-Length: 16777216\r\n",
    b"\x08\x80\x80\x80\x80\x08",   // protobuf：超长 varint 字段号
];

/// 一个用例的判定结果（由 probe 返回）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// 解码器接受了这份字节。
    Accepted,
    /// 解码器拒绝了这份字节（含所有"形状不对"的拒绝）。
    Rejected,
}

/// 确定性输入生成器（seed 固定 ⇒ 字节集固定）。
#[derive(Debug, Clone)]
pub struct InputGen {
    rng: XorShift64Star,
}

impl InputGen {
    pub fn new(seed: u64) -> Self {
        Self {
            rng: XorShift64Star::new(seed),
        }
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.rng.next_u64() % n as u64) as usize
        }
    }

    pub fn byte(&mut self) -> u8 {
        (self.rng.next_u64() & 0xff) as u8
    }

    /// 均匀随机字节。
    pub fn uniform(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.byte()).collect()
    }

    /// 偏置字母表字节——更容易命中分隔符/varint/引号等分支。
    pub fn structured(&mut self, len: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(len);
        while out.len() < len {
            if self.below(8) == 0 {
                // 偶尔塞一段 varint 续位串或分隔符重复
                let unit = BOUNDARY_UNITS[self.below(BOUNDARY_UNITS.len())];
                out.extend_from_slice(unit);
                continue;
            }
            out.push(STRUCTURED_ALPHABET[self.below(STRUCTURED_ALPHABET.len())]);
        }
        out.truncate(len);
        out
    }

    /// 单一重复单元的填充（全 0、全 0xff、varint 串……）。
    pub fn boundary(&mut self, len: usize) -> Vec<u8> {
        let unit = BOUNDARY_UNITS[self.below(BOUNDARY_UNITS.len())];
        let mut out = Vec::with_capacity(len);
        while out.len() < len {
            let need = len - out.len();
            if need >= unit.len() {
                out.extend_from_slice(unit);
            } else {
                out.extend_from_slice(&unit[..need]);
            }
        }
        out
    }

    /// 对一个**合法基线帧**做 1..=6 次变异（含长度窗口极值）。
    pub fn mutate(&mut self, base: &[u8]) -> Vec<u8> {
        let mut out = base.to_vec();
        let ops = 1 + self.below(6);
        for _ in 0..ops {
            if out.is_empty() {
                out.push(self.byte());
                continue;
            }
            match self.below(9) {
                0 => {
                    let at = self.below(out.len());
                    out[at] = self.byte();
                }
                1 => {
                    let at = self.below(out.len());
                    out[at] ^= 1 << self.below(8);
                }
                2 => {
                    let at = self.below(out.len());
                    out[at] = EXTREME_BYTES[self.below(EXTREME_BYTES.len())];
                }
                3 => {
                    let at = self.below(out.len());
                    out.truncate(at + 1);
                }
                4 => {
                    let at = self.below(out.len() + 1);
                    let n = 1 + self.below(8);
                    let ins = self.uniform(n);
                    out.splice(at..at, ins);
                }
                5 => {
                    let at = self.below(out.len());
                    let end = (at + 1 + self.below(16)).min(out.len());
                    out.drain(at..end);
                }
                6 => {
                    let at = self.below(out.len());
                    let end = (at + 1 + self.below(16)).min(out.len());
                    let dup: Vec<u8> = out[at..end].to_vec();
                    out.splice(at..at, dup);
                }
                7 => {
                    if out.len() >= 4 {
                        let at = self.below(out.len() - 3);
                        let v = EXTREME_LENS[self.below(EXTREME_LENS.len())];
                        out[at..at + 4].copy_from_slice(&v);
                    }
                }
                _ => {
                    let at = self.below(out.len());
                    let end = (at + 1 + self.below(8)).min(out.len());
                    for b in &mut out[at..end] {
                        *b = 0x80;
                    }
                }
            }
        }
        out
    }
}

/// 一次扫描的语料：固定 seed + 固定基线 ⇒ 固定字节集。
#[derive(Debug, Clone)]
pub struct Corpus {
    pub label: String,
    pub seed: u64,
    pub inputs: Vec<Vec<u8>>,
}

impl Corpus {
    /// 语料 digest：对 (label, 每个输入的长度, 字节) 取 SHA-256。
    /// 记入 evidence 后，同一 digest 可复现同一批输入。
    pub fn digest(&self) -> String {
        let mut h = Sha256::new();
        h.update(self.label.as_bytes());
        h.update([0u8]);
        for input in &self.inputs {
            h.update((input.len() as u64).to_be_bytes());
            h.update(input);
        }
        to_hex(&h.finalize())
    }

    pub fn len(&self) -> usize {
        self.inputs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inputs.is_empty()
    }
}

/// 围绕一个基线帧造语料：空/单字节极值 + 四类形状各 `per_shape` 份 + 基线本身与它的截断。
pub fn corpus(seed: u64, label: &str, base: &[u8], per_shape: usize) -> Corpus {
    let mut gen = InputGen::new(seed);
    let mut inputs: Vec<Vec<u8>> = vec![
        Vec::new(),
        vec![0x00],
        vec![0xff],
        vec![0x7f],
        vec![0x80],
        base.to_vec(),
        base[..base.len() / 2].to_vec(), // 合法帧的截断（长度字段与正文不一致）
    ];
    for _ in 0..per_shape {
        let len = 1 + gen.below(96);
        inputs.push(gen.uniform(len));
    }
    for _ in 0..per_shape {
        let len = 1 + gen.below(160);
        inputs.push(gen.structured(len));
    }
    for _ in 0..per_shape {
        let len = 1 + gen.below(160);
        inputs.push(gen.boundary(len));
    }
    for _ in 0..per_shape {
        inputs.push(gen.mutate(base));
    }
    Corpus {
        label: label.to_string(),
        seed,
        inputs,
    }
}

/// 一次违规：探针里 `panic!`/`assert!` 的现场（含复现所需的 seed 与输入）。
#[derive(Debug, Clone)]
pub struct Violation {
    pub index: usize,
    pub input_hex: String,
    pub message: String,
}

/// 扫描报告。
#[derive(Debug, Clone)]
pub struct ScanReport {
    pub label: String,
    pub seed: u64,
    pub cases: usize,
    pub accepted: usize,
    pub rejected: usize,
    pub digest: String,
    pub violations: Vec<Violation>,
}

impl ScanReport {
    /// 断言零违规。失败信息带 seed + 用例下标 + 输入 hex（可原地复现），而不是只说"失败了"。
    pub fn assert_clean(&self) {
        if self.violations.is_empty() {
            return;
        }
        let mut msg = format!(
            "{}：{} 个用例中 {} 个触发违规（seed={}, digest={}）\n",
            self.label,
            self.cases,
            self.violations.len(),
            self.seed,
            self.digest
        );
        for v in self.violations.iter().take(5) {
            msg.push_str(&format!(
                "  [case {}] {}\n    input(hex)={}\n",
                v.index, v.message, v.input_hex
            ));
        }
        if self.violations.len() > 5 {
            msg.push_str(&format!("  …另有 {} 个\n", self.violations.len() - 5));
        }
        panic!("{msg}");
    }

    /// 一行摘要（用于证据记录）。
    pub fn summary(&self) -> String {
        format!(
            "{}: cases={} accepted={} rejected={} violations={} seed={} digest={}",
            self.label,
            self.cases,
            self.accepted,
            self.rejected,
            self.violations.len(),
            self.seed,
            self.digest
        )
    }
}

/// 跑一次扫描：探针返回 [`Outcome`]；探针内的 `panic!` 被捕获并记为违规（不让第一个坏用例
/// 中断整批）。扫描期间临时静音 panic hook，便于在报告里集中列出复现信息。
///
/// 注意：panic hook 是进程级全局的，`cargo test` 并行跑其他测试时它们的 panic 输出可能被
/// 这一小段窗口吞掉（失败本身仍由测试框架报告）。
pub fn scan<F>(corpus: &Corpus, mut probe: F) -> ScanReport
where
    F: FnMut(&[u8]) -> Outcome,
{
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let mut accepted = 0usize;
    let mut rejected = 0usize;
    let mut violations = Vec::new();
    for (index, input) in corpus.inputs.iter().enumerate() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| probe(input)));
        match result {
            Ok(Outcome::Accepted) => accepted += 1,
            Ok(Outcome::Rejected) => rejected += 1,
            Err(payload) => violations.push(Violation {
                index,
                input_hex: to_hex(&truncate_hex(input)),
                message: panic_message(&*payload),
            }),
        }
    }
    std::panic::set_hook(previous);
    ScanReport {
        label: corpus.label.clone(),
        seed: corpus.seed,
        cases: corpus.inputs.len(),
        accepted,
        rejected,
        digest: corpus.digest(),
        violations,
    }
}

/// 断言消费长度合法：`0 < consumed <= input_len`。
///
/// 返回 [`Outcome::Accepted`]，以便直接用作探针的返回值——违规时 `assert!` 会 panic，
/// 由 [`scan`] 记成带输入 hex 的违规。
pub fn expect_consumed(input_len: usize, consumed: usize) -> Outcome {
    assert!(
        consumed > 0,
        "consumed == 0：按帧循环会原地打转（输入长度 {input_len}）"
    );
    assert!(
        consumed <= input_len,
        "consumed {consumed} > 输入长度 {input_len}：消费记账越界"
    );
    Outcome::Accepted
}

fn truncate_hex(input: &[u8]) -> Vec<u8> {
    const MAX: usize = 96;
    if input.len() <= MAX {
        input.to_vec()
    } else {
        let mut out = input[..MAX].to_vec();
        out.extend_from_slice(format!("…(+{}B)", input.len() - MAX).as_bytes());
        out
    }
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "panic（无字符串载荷）".to_string()
    }
}

fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}
