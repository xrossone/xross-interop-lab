//! T14 验收 case（plans/01-foundation.md §T14）+ testkit 行为测试。
//!
//! - T14-01：fixture 不存在或 hash 变化 → 验收失败，不可静默重录。
//! - T14-02：模拟 peer 自对通 → 仅 simulated，不可升级 device-verified。
//! - T14-03：日志中出现假 token → 脱敏检查失败。
//! - T14-04：随机分片 seed 固定 → 可重复结果。
//!
//! 映射说明：plans/01 的 `tests/fixtures/` →
//! `impl/crates/interop-testkit/tests/fixtures/`；
//! `lab/evidence/run.schema.json` → lab 仓库 `evidence/run.schema.json`。
#![forbid(unsafe_code)]

use interop_testkit::clock::FakeClock;
use interop_testkit::evidence::{
    check_redaction, fixture_index_path, verify_fixtures, FixtureRegistry, RunKind, RunManifest,
};
use interop_testkit::peer::{Fragmenter, XorShift64Star};

// ---------------------------------------------------------------------------
// T14-01 fixture 完整性
// ---------------------------------------------------------------------------

#[test]
fn t14_01_missing_or_tampered_fixture_fails() {
    let dir = std::env::temp_dir().join(format!(
        "interop-testkit-t14-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();

    // 制造两个 fixture（record 模式）+ 登记表
    let mut reg = FixtureRegistry::new();
    reg.record(&dir, "sample.bin", b"deterministic-content");
    reg.record(&dir, "other.txt", b"other");

    // 正常：verify 通过
    assert!(verify_fixtures(&dir, &reg).is_ok());

    // ① 篡改内容 → 失败，不可静默重录
    std::fs::write(dir.join("sample.bin"), b"tampered-content").unwrap();
    let err = verify_fixtures(&dir, &reg).unwrap_err();
    assert!(err.iter().any(|e| e.contains("sample.bin")), "{err:?}");

    // ② 文件缺失 → 失败
    std::fs::remove_file(dir.join("other.txt")).unwrap();
    let err = verify_fixtures(&dir, &reg).unwrap_err();
    assert!(err.iter().any(|e| e.contains("other.txt")), "{err:?}");

    // ③ 登记表里没有的额外文件不算失败（登记表是允许集的下限），但
    //    verify 永远不会自动改写登记表（无静默重录路径）
    let before = fixture_index_path(&dir).exists();
    let _ = verify_fixtures(&dir, &reg);
    assert_eq!(fixture_index_path(&dir).exists(), before, "verify 不得写登记表");

    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// T14-02 模拟自对通：simulated 不可升级
// ---------------------------------------------------------------------------

#[test]
fn t14_02_simulated_run_cannot_claim_device_verified() {
    let mut manifest = RunManifest::new("2026-09-15-t14-self-loop", RunKind::Simulated);
    manifest.command("cargo test -p interop-ipc --test ipc", 0);
    manifest.case("T09-01");
    let json = manifest.finish();

    assert_eq!(json["kind"], "simulated");
    assert_eq!(
        json["evidence_level"], "simulated",
        "模拟 run 的证据等级固定为 simulated（T14-02）"
    );

    // 类型层面：RunKind 没有任何路径能构造 Device 变体而不提供真机 attestation
    assert_ne!(RunKind::Simulated.as_str(), "device-verified");
    // RunManifest 只接受 kind 决定的 evidence_level
    let device_manifest = RunManifest::new(
        "2026-09-15-t14-device",
        RunKind::Device {
            attestation: "device-run-2026-09-15-airplay-poc".into(),
        },
    );
    assert_eq!(device_manifest.finish()["evidence_level"], "device-verified");
    // Manual 是人工观察，也不是 device-verified
    assert_eq!(
        RunManifest::new("r", RunKind::Manual).finish()["evidence_level"],
        "manual"
    );
}

// ---------------------------------------------------------------------------
// T14-03 日志脱敏检查
// ---------------------------------------------------------------------------

#[test]
fn t14_03_fake_token_in_log_fails_redaction() {
    // 人工构造的假 secret（分段拼装避免本测试源码自命中）
    let fake_aws = concat!("AKIA", "IOSFODNN7EXAMPLE");
    let log = format!("connect attempt used key {fake_aws} and failed");

    let errs = check_redaction(&log).unwrap_err();
    assert!(
        errs.iter().any(|e| e.contains("AWS")),
        "假 token 必须被脱敏检查发现：{errs:?}"
    );

    // 干净日志通过
    assert!(check_redaction("plain log line, no secrets").is_ok());
    // PEM 形状（分段构造）
    let pem = concat!("-----BEGIN", " RSA PRIVATE KEY-----");
    assert!(check_redaction(pem).is_err());
}

// ---------------------------------------------------------------------------
// T14-04 固定 seed 的分片/重排可重复
// ---------------------------------------------------------------------------

#[test]
fn t14_04_fixed_seed_is_reproducible() {
    let payload: Vec<u8> = (0..4096u32).map(|i| (i * 31 + 7) as u8).collect();

    // 同 seed 两次：chunk 划分与重排完全一致
    let mut f1 = Fragmenter::new(42);
    let mut f2 = Fragmenter::new(42);
    let run1 = f1.fragment_reorder(&payload);
    let run2 = f2.fragment_reorder(&payload);
    assert_eq!(run1, run2, "固定 seed 必须产生完全相同的分片/重排（T14-04）");

    // 重排后的流重组回原 payload（完整性不受分片重排破坏）
    let mut reassembled = run1.clone();
    reassembled.sort_by_key(|c| c.offset);
    let joined: Vec<u8> = reassembled.into_iter().flat_map(|c| c.bytes).collect();
    assert_eq!(joined, payload);

    // 不同 seed：chunk 边界不同（几乎必然），但重组结果相同
    let mut f3 = Fragmenter::new(43);
    let run3 = f3.fragment_reorder(&payload);
    let chunk_bounds_1: Vec<u64> = run1.iter().map(|c| c.offset).collect();
    let chunk_bounds_3: Vec<u64> = run3.iter().map(|c| c.offset).collect();
    assert_ne!(
        chunk_bounds_1, chunk_bounds_3,
        "不同 seed 应产生不同 chunk 边界（分片注入有意义）"
    );
    let mut re3 = run3;
    re3.sort_by_key(|c| c.offset);
    assert_eq!(re3.into_iter().flat_map(|c| c.bytes).collect::<Vec<u8>>(), payload);

    // xorshift 本身可重复
    let mut a = XorShift64Star::new(7);
    let mut b = XorShift64Star::new(7);
    assert_eq!(a.next_u64(), b.next_u64());
}

// ---------------------------------------------------------------------------
// FakeClock + fixture 生成算法记录
// ---------------------------------------------------------------------------

#[test]
fn fake_clock_advances_monotonically() {
    let mut clock = FakeClock::new(1000);
    assert_eq!(clock.now_ms(), 1000);
    clock.advance(250);
    assert_eq!(clock.now_ms(), 1250);
    clock.advance(0);
    assert_eq!(clock.now_ms(), 1250);
}

#[test]
fn committed_fixture_matches_recorded_algorithm_and_hash() {
    // tests/fixtures/sample.bin 的生成算法：i-th byte = (i*31+7) % 256，长度 2048
    let generated: Vec<u8> = (0..2048u32).map(|i| (i.wrapping_mul(31).wrapping_add(7)) as u8).collect();
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    // 该 fixture 必须已在仓库内且与算法一致（不通过 record 路径重新生成）
    let on_disk = std::fs::read(dir.join("sample.bin")).expect("tests/fixtures/sample.bin 已提交");
    assert_eq!(on_disk, generated, "提交的 fixture 与记录的生成算法不一致");
    assert!(verify_fixtures(&dir, &FixtureRegistry::load(&dir).unwrap()).is_ok());
}
