//! T12 验收测试（plans/01-foundation.md §T12）：只读 platform probe 与 radio lease 模型。
//!
//! 三条硬规矩：
//! - probe 只读：命令白名单 + 固定参数 + 不经 shell；接口名以外的地址/MAC 一律不采集；
//! - **arbiter 自身永不改网络**：唯一出口是注入的 `RadioDriver`，且只在"用户批准"之后调用；
//! - hardware/api 不可用时 fail-closed（`hardware-unavailable`），不假造 stub 成功。

use interop_contract::error::ErrorCode;
use interop_platform::probe::{
    parse_interface_names, probe_local, ProbeValue, SupportLevel, PROBE_COMMANDS,
};
use interop_platform::radio::{
    Approval, FakeRadioDriver, LeaseMode, LeaseState, RadioArbiter, RadioCapabilities,
    RadioRequest, RadioResource, RollbackPlan,
};

fn macos_caps_p2p_unavailable() -> RadioCapabilities {
    RadioCapabilities {
        p2p: SupportLevel::ApiUnavailable,
        wifi_display: SupportLevel::ApiUnavailable,
        wifi_radio_present: true,
    }
}

fn rollback() -> RollbackPlan {
    RollbackPlan {
        summary: "恢复 Wi-Fi 到批准前的接口状态".into(),
        steps: vec!["重新启用 en0".into()],
    }
}

/// T12-01：WiFi 不支持 P2P → hardware-unavailable、无网络变更。
#[test]
fn t12_01_unsupported_p2p_fails_closed_without_network_change() {
    let driver = FakeRadioDriver::default();
    let mut arbiter = RadioArbiter::new(Box::new(driver), macos_caps_p2p_unavailable());
    let err = arbiter
        .request(
            RadioRequest {
                resource: RadioResource::WifiDirect,
                mode: LeaseMode::Exclusive,
                subject: "worker:wfd".into(),
                reason: "P2P 建立".into(),
            },
            1_000,
        )
        .expect_err("P2P 不可用必须拒绝");
    assert_eq!(err.code, ErrorCode::HardwareUnavailable);
    assert!(
        arbiter.driver_calls().is_empty(),
        "拒绝路径不得触碰任何驱动器（无网络变更）：{:?}",
        arbiter.driver_calls()
    );
    assert!(arbiter.leases().is_empty(), "失败请求不留下 lease");

    // Wi-Fi 广播本身存在：共享（只读）使用是允许的
    let id = arbiter
        .request(
            RadioRequest {
                resource: RadioResource::WifiRadio,
                mode: LeaseMode::Shared,
                subject: "probe".into(),
                reason: "只读探测".into(),
            },
            1_000,
        )
        .expect("共享只读不改变网络");
    assert_eq!(arbiter.leases().len(), 1);
    assert_eq!(arbiter.get(&id).unwrap().state, LeaseState::Granted);
    assert!(arbiter.driver_calls().is_empty(), "共享 lease 也不得写入");
}

/// T12-02：独占 lease 与当前连接冲突 → pending approval 或 busy。
#[test]
fn t12_02_exclusive_lease_conflict_needs_approval_or_busy() {
    let driver = FakeRadioDriver::default();
    let mut arbiter = RadioArbiter::new(Box::new(driver), macos_caps_p2p_unavailable());
    // 已有共享使用（例如正在下载）
    arbiter
        .request(
            RadioRequest {
                resource: RadioResource::WifiRadio,
                mode: LeaseMode::Shared,
                subject: "download".into(),
                reason: "既有连接".into(),
            },
            1_000,
        )
        .expect("shared");

    let exclusive = RadioRequest {
        resource: RadioResource::WifiRadio,
        mode: LeaseMode::Exclusive,
        subject: "worker:wfd".into(),
        reason: "切换信道做 P2P".into(),
    };
    // 直接索取 → busy（不静默抢占）
    let busy = arbiter.request(exclusive.clone(), 2_000).expect_err("冲突应 busy");
    assert_eq!(busy.code, ErrorCode::Busy);
    assert!(arbiter.driver_calls().is_empty());

    // 走审批：pending 期间不得动网络
    let id = arbiter
        .request_with_approval(exclusive.clone(), Approval::Pending, 2_000)
        .expect("登记 pending");
    assert_eq!(arbiter.get(&id).unwrap().state, LeaseState::Pending);
    assert!(arbiter.driver_calls().is_empty(), "pending 期间不得改网络");
    assert_eq!(arbiter.pending().len(), 1);

    // 用户拒绝 → denied，仍然零网络变更
    arbiter
        .deny(&id, 2_100)
        .expect("deny");
    assert_eq!(arbiter.get(&id).unwrap().state, LeaseState::Denied);
    assert!(arbiter.driver_calls().is_empty());

    // 再次申请并由用户批准（带 rollback 计划）→ 此时才允许一次写入
    let id2 = arbiter
        .request_with_approval(exclusive, Approval::Pending, 2_200)
        .expect("pending");
    arbiter.approve(&id2, rollback(), 2_300).expect("approve");
    assert_eq!(arbiter.get(&id2).unwrap().state, LeaseState::Granted);
    assert_eq!(arbiter.driver_calls(), vec!["apply:wifi_radio".to_string()]);
    assert_eq!(
        arbiter.get(&id2).unwrap().rollback.as_ref().unwrap().steps.len(),
        1,
        "批准的 rollback 必须随 lease 保存"
    );

    // 释放 → 执行已批准 rollback，且只执行一次
    arbiter.release(&id2, 2_400).expect("release");
    assert_eq!(arbiter.get(&id2).unwrap().state, LeaseState::Released);
    assert_eq!(
        arbiter.driver_calls(),
        vec!["apply:wifi_radio".to_string(), "rollback:wifi_radio".to_string()]
    );
}

/// T12-03：worker 退出 → 回收 radio lease 并运行已批准 rollback。
#[test]
fn t12_03_worker_exit_reclaims_lease_and_rolls_back() {
    let driver = FakeRadioDriver::default();
    let mut arbiter = RadioArbiter::new(Box::new(driver), macos_caps_p2p_unavailable());
    let granted = arbiter
        .request_with_approval(
            RadioRequest {
                resource: RadioResource::WifiRadio,
                mode: LeaseMode::Exclusive,
                subject: "worker:airplay".into(),
                reason: "镜像会话".into(),
            },
            Approval::UserGranted {
                rollback: rollback(),
            },
            5_000,
        )
        .expect("granted");
    assert_eq!(arbiter.driver_calls(), vec!["apply:wifi_radio".to_string()]);

    // 同 subject 另有一条 pending（尚未批准）
    let pending = arbiter
        .request_with_approval(
            RadioRequest {
                resource: RadioResource::WifiRadio,
                mode: LeaseMode::Exclusive,
                subject: "worker:airplay".into(),
                reason: "重连尝试".into(),
            },
            Approval::Pending,
            5_100,
        )
        .expect("pending");

    let reclaimed = arbiter.worker_exit("worker:airplay", 5_200);
    assert_eq!(reclaimed, vec![granted.clone()], "只回收该 worker 的 lease");
    assert_eq!(arbiter.get(&granted).unwrap().state, LeaseState::Reclaimed);
    assert_eq!(arbiter.get(&pending).unwrap().state, LeaseState::Denied, "未批准的 pending 直接作废");
    assert_eq!(
        arbiter.driver_calls(),
        vec!["apply:wifi_radio".to_string(), "rollback:wifi_radio".to_string()],
        "回收必须运行已批准 rollback，且只运行一次"
    );

    // 其他 worker 的 lease 不受影响
    let other = arbiter
        .request_with_approval(
            RadioRequest {
                resource: RadioResource::WifiRadio,
                mode: LeaseMode::Shared,
                subject: "worker:dlna".into(),
                reason: "upnp 探测".into(),
            },
            Approval::Pending,
            5_300,
        )
        .expect("other pending");
    arbiter.worker_exit("worker:gone", 5_400);
    assert_eq!(arbiter.get(&other).unwrap().state, LeaseState::Pending);
}

/// probe 只读白名单：固定程序 + 固定参数、不含写操作、不经 shell。
#[test]
fn t12_probe_allowlist_is_read_only() {
    assert!(!PROBE_COMMANDS.is_empty(), "白名单不得为空");
    let forbidden = [
        "sudo", "su", "sh", "bash", "zsh", "set", "down", "up", "off", "on", "create", "delete",
        "disable", "enable", "config", "join", "leave", "rm", "mv", "chmod",
    ];
    for cmd in PROBE_COMMANDS {
        for verb in forbidden {
            assert_ne!(
                cmd.program,
                verb,
                "白名单不得包含 {}（probe 禁止任何写/提权入口）",
                verb
            );
            for arg in cmd.args {
                assert!(
                    !arg.eq_ignore_ascii_case(verb),
                    "{} 的参数 {:?} 含写操作动词 {}",
                    cmd.program,
                    cmd.args,
                    verb
                );
            }
        }
        assert!(
            !cmd.why.is_empty(),
            "每条白名单命令必须记录只读用途（审计）"
        );
    }
}

/// 真实只读 probe：本机可观察到接口名，且报告里带命令证据。
#[test]
fn t12_probe_observes_interfaces_with_command_evidence() {
    let report = probe_local();
    assert!(
        matches!(report.os.as_str(), "macos" | "linux" | "windows"),
        "未知 OS 必须如实记录：{}",
        report.os
    );
    if report.os == "macos" {
        assert!(!report.commands.is_empty(), "macOS 上应至少执行一条白名单命令");
        let ifaces = report
            .network_interfaces
            .observed()
            .expect("macOS 上接口枚举应可得");
        assert!(!ifaces.is_empty(), "至少有一个接口");
        assert!(ifaces.iter().any(|n| n == "lo0" || n == "en0"), "包含 lo0/en0 之一：{ifaces:?}");
        for name in ifaces {
            assert!(
                !name.contains(':') && !name.contains('.'),
                "接口列表只保留名字（不采集地址/MAC）：{name}"
            );
        }
        assert_eq!(
            report.wifi_display.observed(),
            None,
            "macOS 无 WFD 实现：不得声称可用"
        );
    }
}

/// 解析必须只保留接口名（地址/MAC 不进入报告）。
#[test]
fn t12_probe_parse_redacts_addresses_and_macs() {
    let macos = "lo0 gif0 stf0 en0 en1 bridge0\n";
    assert_eq!(parse_interface_names("ifconfig", macos), vec!["lo0", "gif0", "stf0", "en0", "en1", "bridge0"]);

    let linux = "1: lo: <LOOPBACK,UP> mtu 65536 qdisc noqueue state UNKNOWN mode DEFAULT \\    link/loopback 00:00:00:00:00:00 brd 00:00:00:00:00:00\n\
                 2: en0: <BROADCAST,MULTICAST,UP> mtu 1500 qdisc fq_codel state UP mode DEFAULT link/ether aa:bb:cc:dd:ee:ff brd ff:ff:ff:ff:ff:ff\n";
    assert_eq!(parse_interface_names("ip", linux), vec!["lo", "en0"]);
    assert!(!parse_interface_names("ip", linux).iter().any(|n| n.contains("aa:bb")));

    let windows = "Admin State    State          Type             Interface Name\n\
                   Enabled        Connected      Dedicated        Wi-Fi\n\
                   Enabled        Disconnected   Dedicated        Ethernet 2\n";
    assert_eq!(parse_interface_names("netsh", windows), vec!["Wi-Fi", "Ethernet 2"]);
}

/// 未在本机验证的字段必须如实标注（not-run/unavailable），不猜测。
#[test]
fn t12_probe_unverified_fields_are_explicit() {
    let report = probe_local();
    for (field, value) in [
        ("wifi_p2p", &report.wifi_p2p),
        ("wifi_display", &report.wifi_display),
        ("permissions", &report.permissions),
    ] {
        match value {
            ProbeValue::Observed { .. } => {}
            ProbeValue::NotRun { why } | ProbeValue::Unavailable { why } => {
                assert!(!why.is_empty(), "{field}: 未观测必须写明原因")
            }
        }
    }
    assert!(
        matches!(report.media_outputs, ProbeValue::NotRun { .. } | ProbeValue::Unavailable { .. }),
        "native/file sink 尚未实现（T29），本轮不得声称媒体输出可用"
    );
}
