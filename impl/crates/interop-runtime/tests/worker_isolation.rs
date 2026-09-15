//! T13 验收测试（plans/01-foundation.md §T13）：worker supervisor 与真实隔离 probe。
//!
//! 这些用例跑在**真实子进程、真实管道、真实文件**上（`impl/workers/mock-provider`），不是内存 mock：
//! - T13-01 canary：沙箱生效时 OS 必须拒绝读取；否则按 isolation 失败如实上报 process-only；
//! - T13-02 任意 shell：worker 的 exec 请求一律 `auth-denied`；
//! - T13-03 重启上限：窗口内崩溃超过 4 次停止自动重启；
//! - T13-04 关闭：无孤儿 worker / 无孤儿 listener 文件。
//!
//! 依赖 `target/debug/mock-provider`：`cargo test --workspace` 会构建全部成员；
//! 单独跑本测试前先 `cargo build --workspace`。

use interop_contract::error::ErrorCode;
use interop_runtime::workers::{
    CanaryOutcome, IsolationLevel, StartOptions, WorkerProfile, WorkerSupervisor,
};
use std::path::{Path, PathBuf};

fn mock_binary_path() -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/mock-provider");
    assert!(
        p.is_file(),
        "缺少 mock-provider 二进制：先 `cargo build --workspace`（或直接 `cargo test --workspace`）；路径 {}",
        p.display()
    );
    std::fs::canonicalize(p).expect("canonicalize")
}

fn profile(id: &str, sha256: Option<String>, sandbox: bool) -> WorkerProfile {
    WorkerProfile {
        profile_id: id.into(),
        binary: mock_binary_path().to_string_lossy().to_string(),
        sha256,
        args: vec![],
        env_allow: vec!["INTEROP_WORKER_ID".into()],
        max_restarts: 4,
        restart_window_ms: 300_000,
        sandbox,
    }
}

fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("interop-t13-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("temp dir");
    d
}

fn write_canary(dir: &Path) -> PathBuf {
    let p = dir.join("canary.secret");
    std::fs::write(&p, b"CANARY-fake-secret-value").expect("write canary");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).expect("chmod 600");
    }
    p
}

fn wait_for<F: FnMut() -> bool>(mut f: F, timeout_ms: u64) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    loop {
        if f() {
            return true;
        }
        if std::time::Instant::now() > deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// 二进制 hash 校验：pin 后必须匹配；不匹配时不得生成进程；profile 文件可加载。
#[test]
fn t13_00_binary_hash_pin_and_profile_file() {
    let profiles_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../policies/worker-profiles.json");
    let mut sup = WorkerSupervisor::new();
    let n = sup
        .load_profiles(&profiles_path)
        .expect("profile 文件可加载");
    assert!(n >= 1, "至少一个 profile");
    sup.add_profile(profile("mock-provider", None, false));
    let pid = sup
        .start("mock-provider", "idle", StartOptions::default(), 0)
        .expect("启动");
    assert!(pid > 0);
    let pinned = sup.pinned_hash("mock-provider").expect("启动后必须有 pin").to_string();
    assert_eq!(pinned.len(), 64, "sha256 十六进制长度");
    println!("mock-provider pinned sha256 = {pinned}");
    sup.shutdown();

    // 错 hash → integrity-failed，且没有生成进程
    let mut bad = WorkerSupervisor::new();
    bad.add_profile(profile(
        "mock-provider",
        Some("0".repeat(64)),
        false,
    ));
    let err = bad
        .start("mock-provider", "idle", StartOptions::default(), 0)
        .expect_err("hash 不匹配必须拒绝");
    assert_eq!(err.code, ErrorCode::IntegrityFailed);
    assert_eq!(bad.worker_count(), 0, "拒绝路径不得生成进程");
    assert!(
        bad.log().is_empty(),
        "拒绝路径不得留下 Spawned 事件：{:?}",
        bad.log()
    );
}

/// T13-01：worker 读无授权 canary → OS 拒绝或标明 isolation 失败。
#[test]
fn t13_01_canary_isolation_is_evidenced() {
    let dir = temp_dir("canary");
    let canary = write_canary(&dir);

    let mut sup = WorkerSupervisor::new();
    sup.add_profile(profile("mock-provider", None, true));
    let sandbox_available = sup.sandbox_available();
    let report = sup
        .isolation_report("mock-provider", &canary, 1_000)
        .expect("探针运行");
    assert!(!report.note.is_empty(), "结论必须带说明");
    assert_eq!(sup.worker_count(), 0, "探针结束后不得留下 worker");
    println!(
        "isolation evidence: sandbox_available={sandbox_available} actual={:?} canary={:?} note={}",
        report.actual, report.canary, report.note
    );

    match (&report.canary, report.actual, sandbox_available) {
        (CanaryOutcome::Denied { detail }, IsolationLevel::PlatformSandbox, true) => {
            assert!(!detail.is_empty(), "拒绝必须带证据：{detail}");
            assert!(
                report.restricted_profiles.is_empty(),
                "沙箱生效时不需要限制 profile"
            );
        }
        (CanaryOutcome::Readable { .. }, IsolationLevel::ProcessOnly, false) => {
            assert!(
                !report.restricted_profiles.is_empty(),
                "process-only 必须限制可发布 profile"
            );
            assert!(
                report.note.contains("process-only"),
                "说明必须点名等级：{}",
                report.note
            );
        }
        (other, level, available) => panic!(
            "隔离等级与 canary 证据不一致（sandbox_available={available}）：{other:?} / {level:?}"
        ),
    }
}

/// T13-02：worker 命令要求任意 shell → auth-denied。
#[test]
fn t13_02_arbitrary_shell_request_is_denied() {
    let mut sup = WorkerSupervisor::new();
    sup.add_profile(profile("mock-provider", None, false));
    sup.start("mock-provider", "request_shell", StartOptions::default(), 0)
        .expect("start");

    let saw_denied = wait_for(
        || {
            sup.poll(1);
            sup.worker_lines("mock-provider")
                .iter()
                .any(|l| l == "exec-denied:auth-denied")
        },
        3_000,
    );
    let lines = sup.worker_lines("mock-provider");
    assert!(saw_denied, "worker 应收到 auth-denied 答复，实际：{lines:?}");
    assert!(
        !lines.iter().any(|l| l == "exec-allowed"),
        "任意 shell 绝不能放行：{lines:?}"
    );
    let denied = sup
        .log()
        .iter()
        .find(|e| matches!(e, interop_runtime::workers::SupervisorEvent::ExecDenied { .. }))
        .expect("必须留下 ExecDenied 审计事件");
    match denied {
        interop_runtime::workers::SupervisorEvent::ExecDenied {
            request, code, ..
        } => {
            assert!(request.contains("/bin/sh"), "记录原始请求：{request}");
            assert_eq!(code, "auth-denied");
        }
        _ => unreachable!(),
    }
    sup.shutdown();
}

/// T13-03：连续 4 次崩溃/5 分钟 → 停止自动重启。
#[test]
fn t13_03_restart_limit_stops_auto_restart() {
    let mut sup = WorkerSupervisor::new();
    sup.add_profile(profile("mock-provider", None, false));
    sup.start("mock-provider", "crash", StartOptions::default(), 0)
        .expect("start");

    let stopped = wait_for(
        || {
            sup.poll(10);
            sup.is_stopped("mock-provider")
        },
        10_000,
    );
    assert!(stopped, "到重启上限必须停下");
    assert_eq!(
        sup.spawn_count("mock-provider"),
        5,
        "1 次初始 + 4 次重启（窗口内第 5 次仍允许，第 6 次不再启动）"
    );
    assert!(sup.log().iter().any(|e| matches!(
        e,
        interop_runtime::workers::SupervisorEvent::RestartLimitReached { restarts: 4, .. }
    )));
    assert_eq!(sup.worker_count(), 0, "上限后不再自动重启");
    let err = sup
        .start("mock-provider", "idle", StartOptions::default(), 20_000)
        .expect_err("停止后需人工介入");
    assert_eq!(err.code, ErrorCode::Busy);
}

/// T13-04：关闭 supervisor → 无孤儿 listener/worker。
#[test]
fn t13_04_shutdown_leaves_no_orphans() {
    let dir = temp_dir("listener");
    let listener = dir.join("worker.sock");
    let mut sup = WorkerSupervisor::new();
    sup.add_profile(profile("mock-provider", None, false));
    sup.start(
        "mock-provider",
        "open_listener",
        StartOptions {
            listener_path: Some(listener.clone()),
            ..StartOptions::default()
        },
        0,
    )
    .expect("start");

    assert!(
        wait_for(|| listener.exists(), 3_000),
        "worker 应创建 listener：{}",
        listener.display()
    );
    let (workers, killed, orphans) = sup.shutdown();
    assert_eq!(workers, 1);
    assert_eq!(killed, 0, "正常关闭不应需要 kill");
    assert!(orphans.is_empty(), "不得留下孤儿 listener：{orphans:?}");
    assert!(!listener.exists(), "listener 文件必须被清理");
    assert_eq!(sup.worker_count(), 0, "不得留下孤儿 worker");
}

/// 环境清洗：除白名单外一律不继承（含假 canary secret 变量）。
#[test]
fn t13_05_env_is_cleared_except_allowlist() {
    std::env::set_var("INTEROP_CANARY_SECRET", "fake-secret-xyz");
    let mut sup = WorkerSupervisor::new();
    sup.add_profile(profile("mock-provider", None, false));
    sup.start(
        "mock-provider",
        "dump_env",
        StartOptions {
            env_vars: vec![
                "INTEROP_CANARY_SECRET".into(),
                "HOME".into(),
                "INTEROP_WORKER_ID".into(),
            ],
            ..StartOptions::default()
        },
        0,
    )
    .expect("start");

    let seen = wait_for(
        || {
            sup.poll(1);
            sup.worker_lines("mock-provider")
                .iter()
                .any(|l| l.starts_with("env:INTEROP_WORKER_ID:"))
        },
        3_000,
    );
    let lines = sup.worker_lines("mock-provider");
    assert!(seen, "应收到 env dump：{lines:?}");
    assert!(
        lines.contains(&"env:INTEROP_CANARY_SECRET:absent".to_string()),
        "父进程的 secret 不得被 worker 看到：{lines:?}"
    );
    assert!(
        lines.contains(&"env:HOME:absent".to_string()),
        "非白名单变量必须清空：{lines:?}"
    );
    assert!(
        lines.contains(&"env:INTEROP_WORKER_ID:present".to_string()),
        "白名单变量必须注入：{lines:?}"
    );
    sup.shutdown();
}
