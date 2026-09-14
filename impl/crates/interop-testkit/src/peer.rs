//! 模拟 peer：确定性分片与乱序注入。
//!
//! 伪随机源是 xorshift64*（零依赖、可重复）；**固定 seed ⇒ 固定结果**
//! （T14-04）。分片/重排只改变到达顺序与边界，不改变字节内容——重组后必须
//! 与原始 payload 完全一致。

/// xorshift64* PRNG（可重复；seed 0 自动替换为非零常数）。
#[derive(Debug, Clone)]
pub struct XorShift64Star {
    state: u64,
}

impl XorShift64Star {
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0x9E37_79B9_7F4A_7C15 } else { seed },
        }
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.state = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            0
        } else {
            self.next_u64() % n
        }
    }
}

/// 一个分片：offset（原流位置）+ bytes。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    pub offset: u64,
    pub bytes: Vec<u8>,
}

/// 分片器：chunk 大小 1..=64 字节；可选相邻交换模拟重排。
#[derive(Debug, Clone)]
pub struct Fragmenter {
    rng: XorShift64Star,
}

impl Fragmenter {
    pub fn new(seed: u64) -> Self {
        Self {
            rng: XorShift64Star::new(seed),
        }
    }

    /// 顺序分片（不重排）。
    pub fn fragment(&mut self, payload: &[u8]) -> Vec<Chunk> {
        let mut out = Vec::new();
        let mut offset = 0usize;
        while offset < payload.len() {
            let size = (self.rng.below(64) + 1) as usize;
            let end = (offset + size).min(payload.len());
            out.push(Chunk {
                offset: offset as u64,
                bytes: payload[offset..end].to_vec(),
            });
            offset = end;
        }
        out
    }

    /// 分片 + 相邻交换重排（确定性：同 seed 同结果）。
    pub fn fragment_reorder(&mut self, payload: &[u8]) -> Vec<Chunk> {
        let mut chunks = self.fragment(payload);
        // 相邻交换：i 与 i+1 以 ~50% 概率互换（rng 决定，可重复）
        let mut i = 0;
        while i + 1 < chunks.len() {
            if self.rng.next_u64() & 1 == 1 {
                chunks.swap(i, i + 1);
                i += 2;
            } else {
                i += 1;
            }
        }
        chunks
    }
}
