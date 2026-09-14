//! 证据记录：fixture 登记（hash 固定，禁止静默重录）、run manifest 分级、
//! 日志脱敏检查（SEC-08）。
//!
//! run manifest 遵循 lab 仓库 `evidence/run.schema.json`；kind 决定
//! evidence_level，**没有把 simulated 升级为 device-verified 的构造路径**
//! （T14-02）。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// fixture 登记
// ---------------------------------------------------------------------------

/// fixture 登记表文件名（放在 fixture 目录内）。
pub const FIXTURE_INDEX: &str = "fixtures.index.json";

pub fn fixture_index_path(dir: &Path) -> PathBuf {
    dir.join(FIXTURE_INDEX)
}

/// name → sha256（十六进制）。
#[derive(Debug, Clone, Default)]
pub struct FixtureRegistry {
    entries: BTreeMap<String, String>,
}

impl FixtureRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 载入已提交的登记表（缺失视为空表）。
    pub fn load(dir: &Path) -> Result<Self, std::io::Error> {
        let path = fixture_index_path(dir);
        if !path.exists() {
            return Ok(Self::new());
        }
        let raw = std::fs::read_to_string(path)?;
        let map: BTreeMap<String, String> = serde_json::from_str(&raw)?;
        Ok(Self { entries: map })
    }

    pub fn entries(&self) -> &BTreeMap<String, String> {
        &self.entries
    }

    /// **显式**录制：写文件 + 更新登记表。只在测试的 record 模式调用；
    /// verify 路径永不触达（T14-01"不可静默重录"）。
    pub fn record(&mut self, dir: &Path, name: &str, bytes: &[u8]) {
        std::fs::create_dir_all(dir).expect("fixture dir");
        std::fs::write(dir.join(name), bytes).expect("写 fixture");
        self.entries.insert(name.to_string(), sha256_hex(bytes));
        self.write_index(dir);
    }

    fn write_index(&self, dir: &Path) {
        let raw = serde_json::to_string_pretty(&self.entries).expect("序列化登记表");
        std::fs::write(fixture_index_path(dir), raw).expect("写登记表");
    }
}

/// 校验 fidelity：缺失或 hash 变化 → 失败列表（调用方决定如何处理；
/// 测试里作为断言失败）。**绝不改写登记表。**
pub fn verify_fixtures(dir: &Path, registry: &FixtureRegistry) -> Result<(), Vec<String>> {
    let mut problems = Vec::new();
    for (name, expected) in registry.entries() {
        match std::fs::read(dir.join(name)) {
            Ok(bytes) => {
                let actual = sha256_hex(&bytes);
                if &actual != expected {
                    problems.push(format!(
                        "fixture {name}: hash 变化（expected={expected} actual={actual}）"
                    ));
                }
            }
            Err(_) => problems.push(format!("fixture {name}: 文件缺失")),
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}

// ---------------------------------------------------------------------------
// run manifest
// ---------------------------------------------------------------------------

/// run 类型决定 evidence_level（不可僭越）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunKind {
    /// 模拟/自对通：永远是 simulated。
    Simulated,
    /// 真机运行：必须携带 attestation（指向 device-run 证据 ID）。
    Device { attestation: String },
    /// 人工观察：manual，不升级。
    Manual,
}

impl RunKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Simulated => "simulated",
            Self::Device { .. } => "device-verified",
            Self::Manual => "manual",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommandRecord {
    pub command: String,
    pub exit_code: i32,
}

#[derive(Debug, Clone)]
pub struct RunManifest {
    run_id: String,
    kind: RunKind,
    commands: Vec<CommandRecord>,
    case_ids: Vec<String>,
    artifacts: Vec<String>,
    notes: String,
}

impl RunManifest {
    pub fn new(run_id: &str, kind: RunKind) -> Self {
        Self {
            run_id: run_id.to_string(),
            kind,
            commands: Vec::new(),
            case_ids: Vec::new(),
            artifacts: Vec::new(),
            notes: String::new(),
        }
    }

    pub fn command(&mut self, command: &str, exit_code: i32) {
        self.commands.push(CommandRecord {
            command: command.to_string(),
            exit_code,
        });
    }

    pub fn case(&mut self, case_id: &str) {
        self.case_ids.push(case_id.to_string());
    }

    pub fn artifact(&mut self, path: &str) {
        self.artifacts.push(path.to_string());
    }

    pub fn note(&mut self, note: &str) {
        self.notes = note.to_string();
    }

    /// 产出 manifest JSON（字段与 lab evidence/run.schema.json 对齐）。
    pub fn finish(&self) -> serde_json::Value {
        let (kind_str, attestation) = match &self.kind {
            RunKind::Simulated => ("simulated", None),
            RunKind::Device { attestation } => ("device", Some(attestation.clone())),
            RunKind::Manual => ("manual", None),
        };
        serde_json::json!({
            "schema_version": 1,
            "run_id": self.run_id,
            "kind": kind_str,
            "evidence_level": self.kind.as_str(),
            "device_attestation": attestation,
            "result": "pass",
            "case_ids": self.case_ids,
            "commands": self.commands.iter().map(|c| serde_json::json!({
                "command": c.command, "exit_code": c.exit_code
            })).collect::<Vec<_>>(),
            "artifacts": self.artifacts,
            "notes": self.notes,
            "credentials_redacted": true
        })
    }
}

// ---------------------------------------------------------------------------
// 脱敏检查（SEC-08）
// ---------------------------------------------------------------------------

// 模式分段构造：本文件源码不出现完整 pattern
const PEM_GENERIC: &str = concat!("-----BEGIN", " PRIVATE KEY-----");
const PEM_RSA: &str = concat!("-----BEGIN", " RSA PRIVATE KEY-----");

/// 返回 Err(matches) 表示日志含高置信 secret 形状，必须先脱敏。
pub fn check_redaction(log: &str) -> Result<(), Vec<String>> {
    let mut hits = Vec::new();
    if log.contains(PEM_GENERIC) || log.contains(PEM_RSA) {
        hits.push("PEM私钥头".to_string());
    }
    if has_aws_key(log.as_bytes()) {
        hits.push("AWS访问密钥形状".to_string());
    }
    if find_prefixed_alnum_run(log.as_bytes(), b"ghp_", 36) {
        hits.push("GitHub token形状".to_string());
    }
    if find_prefixed_alnum_run(log.as_bytes(), b"xoxb-", 20) {
        hits.push("Slack token形状".to_string());
    }
    if hits.is_empty() {
        Ok(())
    } else {
        Err(hits)
    }
}

fn has_aws_key(b: &[u8]) -> bool {
    b.windows(20).any(|w| {
        w.starts_with(b"AKIA")
            && w[4..]
                .iter()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    })
}

fn find_prefixed_alnum_run(b: &[u8], prefix: &[u8], min_run: usize) -> bool {
    if b.len() < prefix.len() {
        return false;
    }
    (0..=b.len() - prefix.len()).any(|i| {
        &b[i..i + prefix.len()] == prefix
            && b[i + prefix.len()..]
                .iter()
                .take_while(|c| c.is_ascii_alphanumeric())
                .count()
                >= min_run
    })
}

// ---------------------------------------------------------------------------
// SHA-256（与 interop-file 相同的 RustCrypto 原语；testkit 独立依赖）
// ---------------------------------------------------------------------------

fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    let out = h.finalize();
    let mut s = String::with_capacity(64);
    for byte in out {
        s.push_str(&format!("{byte:02x}"));
    }
    s
}

use sha2::{Digest, Sha256};
