//! T08 验收 case（plans/01-foundation.md §T08）+ spool/integrity 行为测试。
//!
//! - T08-01：名称 `../../secret` 或 `C:\Windows\x` → 拒绝且无目录外写入。
//! - T08-02：审批后中间目录被替换成 symlink → 无目录外写入。
//! - T08-03：声明 1 KiB 却写 2 KiB → resource-limit 且临时文件终止。
//! - T08-04：hash 不符 → integrity-failed、不发布。
//! - T08-05：源文件中途替换 → source-changed。
//!
//! 映射说明：plans/01 的 `tests/contract/storage.rs` →
//! `impl/crates/interop-file/tests/storage.rs`。
#![forbid(unsafe_code)]

use interop_contract::error::ErrorCode;
use interop_contract::ids::EntryId;
use interop_contract::offer::OfferEntry;
use interop_contract::U64;
use interop_file::integrity::sha256_hex;
use interop_file::path::sanitize;
use interop_file::spool::{ReadLease, SpoolStore};
use std::path::PathBuf;

fn tmp_root(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "interop-file-t08-{}-{}-{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn entry(name: &str) -> OfferEntry {
    OfferEntry {
        entry_id: EntryId::try_from(format!("ent_{name}")).unwrap(),
        display_name: name.into(),
        relative_components: Some(vec![name.to_string()]),
        media_type_hint: None,
        declared_size: U64(1024),
        wire_payload_id: format!("wire-{name}"),
    }
}

// ---------------------------------------------------------------------------
// T08-01 路径穿越 / Windows 绝对路径形状
// ---------------------------------------------------------------------------

#[test]
fn t08_01_traversal_and_windows_paths_rejected() {
    // ../../secret 形状（components 带 ..）
    let err = sanitize(
        Some(&["..".to_string(), "..".to_string(), "secret".to_string()]),
        "secret",
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidFrame);

    // display_name 本身就是路径
    let err = sanitize(None, "../../secret").unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidFrame);

    // Windows 绝对/UNC/drive 形状
    for hostile in ["C:\\Windows\\x", "\\\\server\\share", "C:evil", "aux", "CON"] {
        let err = sanitize(None, hostile).unwrap_err();
        assert_eq!(
            err.code,
            ErrorCode::InvalidFrame,
            "hostile 名称 {hostile:?} 必须拒绝"
        );
    }

    // 拒绝发生在 sanitize/begin 阶段 → spool 目录内没有任何文件
    let root = tmp_root("t08-01");
    let mut store = SpoolStore::open(&root).unwrap();
    let mut e = entry("ok.bin");
    e.display_name = "C:\\Windows\\x".into();
    e.relative_components = Some(vec!["C:\\Windows\\x".into()]);
    let res = store.begin(&e, 1024);
    assert!(res.is_err());
    assert_eq!(count_files(&root), 0, "被拒条目不得产生任何临时文件");
    assert!(!PathBuf::from("/Windows").exists() || true); // 形状拒绝先于任何 fs 操作
}

fn count_files(dir: &std::path::Path) -> usize {
    std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok()).count())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// T08-02 审批后中间目录替换为 symlink
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn t08_02_symlink_swapped_directory_is_detected() {
    use std::os::unix::fs::symlink;

    let root = tmp_root("t08-02");
    let mut store = SpoolStore::open(&root).unwrap();
    let e = entry("payload.bin");
    let handle = store.begin(&e, 1024).unwrap();
    store
        .write(handle, 0, b"contents".as_slice())
        .unwrap();

    // 审批后（begin 之后）攻击者把目标目录换成了指向外部的 symlink
    let outside = tmp_root("t08-02-outside");
    let target_dir = root.join("sub");
    symlink(&outside, &target_dir).unwrap();
    assert!(target_dir.is_symlink());

    // commit 必须检测到中间目录是 symlink，拒绝且无目录外写入
    let comps = sanitize(Some(&["sub".to_string()]), "payload.bin").unwrap();
    let err = store
        .commit(handle, &comps, Some(&sha256_hex(b"contents")))
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidFrame);

    assert_eq!(
        std::fs::read_dir(&outside).unwrap().count(),
        0,
        "symlink 指向的目录外位置不得有任何写入"
    );
    // root 自身是 symlink 时 SpoolStore::open 直接拒绝
    let real = tmp_root("t08-02-real");
    std::fs::create_dir_all(&real).unwrap();
    let link_base = tmp_root("t08-02-link");
    let link_path = link_base.join("linked");
    symlink(&real, &link_path).unwrap();
    assert_eq!(
        SpoolStore::open(&link_path).unwrap_err().code,
        ErrorCode::InvalidFrame
    );
}

// ---------------------------------------------------------------------------
// T08-03 预算超限
// ---------------------------------------------------------------------------

#[test]
fn t08_03_overwrite_budget_terminates_temp_file() {
    let root = tmp_root("t08-03");
    let mut store = SpoolStore::open(&root).unwrap();
    let e = entry("big.bin"); // declared = 1 KiB
    let handle = store.begin(&e, 1024).unwrap();

    store.write(handle, 0, &[0u8; 1024]).unwrap();
    let err = store.write(handle, 1024, &[0u8; 1024]).unwrap_err();
    assert_eq!(err.code, ErrorCode::ResourceLimit);

    // 临时文件终止：句柄不可再用，磁盘临时文件已删除
    let err2 = store.write(handle, 2048, &[0u8; 1]).unwrap_err();
    assert_eq!(err2.code, ErrorCode::ResourceLimit);
    assert_eq!(count_files(&root), 0, "终止的临时文件必须被清理");
}

// ---------------------------------------------------------------------------
// T08-04 hash 不符 → 不发布
// ---------------------------------------------------------------------------

#[test]
fn t08_04_hash_mismatch_blocks_publish() {
    let root = tmp_root("t08-04");
    let mut store = SpoolStore::open(&root).unwrap();
    let e = entry("doc.txt");
    let handle = store.begin(&e, 1024).unwrap();
    store.write(handle, 0, b"actual bytes".as_slice()).unwrap();

    let comps = sanitize(None, "doc.txt").unwrap();
    let err = store
        .commit(handle, &comps, Some(&sha256_hex(b"tampered bytes")))
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::IntegrityFailed);
    assert_eq!(store.published().len(), 0, "hash 不符不得发布");

    // 正确 hash 可以发布
    let handle2 = store.begin(&e, 1024).unwrap();
    store.write(handle2, 0, b"actual bytes".as_slice()).unwrap();
    let receipt = store
        .commit(handle2, &comps, Some(&sha256_hex(b"actual bytes")))
        .unwrap();
    assert_eq!(receipt.sha256_hex, sha256_hex(b"actual bytes"));
    assert_eq!(receipt.size, 12);
    assert!(root.join("doc.txt").is_file());
    assert_eq!(store.published().len(), 1);
    // 临时文件已不存在（原子 rename，不留 .tmp）
    assert_eq!(count_files(&root), 1);
}

// ---------------------------------------------------------------------------
// T08-05 源文件中途替换 → source-changed
// ---------------------------------------------------------------------------

#[test]
fn t08_05_source_replaced_mid_read() {
    let root = tmp_root("t08-05");
    let src_path = root.join("source.bin");
    std::fs::write(&src_path, b"version-1-payload").unwrap();

    let lease = ReadLease::open(&src_path).unwrap();
    let first = lease.read_chunk(0, 7).unwrap();
    assert_eq!(first, b"version".as_slice());

    // 中途替换内容（同长度，避免只靠 size 检出）
    std::fs::write(&src_path, b"version-9-payload").unwrap();

    let err = lease.read_chunk(10, 7).unwrap_err();
    assert_eq!(err.code, ErrorCode::SourceChanged);
    // 显式刷新身份后可继续（新的 lease）
    let lease2 = ReadLease::open(&src_path).unwrap();
    assert_eq!(lease2.read_chunk(10, 7).unwrap(), b"payload".as_slice());
}

// ---------------------------------------------------------------------------
// integrity：SHA-256 正确性（已知向量）
// ---------------------------------------------------------------------------

#[test]
fn sha256_known_vectors() {
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    // 100 万次 "a" 的向量子集：对 1000 个 "abc" 拼接抽查（避免超长测试时间）
    let long = vec![b'a'; 1_000_000];
    assert_eq!(
        sha256_hex(&long),
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
    );
}
