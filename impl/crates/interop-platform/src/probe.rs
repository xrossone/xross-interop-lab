//! 只读平台 probe（plans/01 T12；CORE-09：报告 remedy、不自动改系统配置）。
//!
//! 规矩：
//! - 只执行**白名单内的固定命令**（见 [`PROBE_COMMANDS`]）：不经 shell、不 sudo、无写操作；
//!   命令缺席时该字段如实记 `NotRun`，不猜测。
//! - 采集最小化：网络部分**只保留接口名**，地址/MAC 一律不进入报告。
//! - 未验证的能力（P2P/WFD/权限/媒体输出）必须显式标 `unavailable`/`not-run`，
//!   不得显示成"支持"（docs/00 的 fail-closed 立场）。

use serde::Serialize;
use std::io::IsTerminal;
use std::process::Command;

/// 能力/实现状态（用于 probe 的每个能力字段，避免一个 `supported: bool` 掩盖差异）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SupportLevel {
    /// 本机实测/明确规定可用
    Supported,
    /// 平台没有公开 API（例如 macOS 的 Wi-Fi Direct / WFD）
    ApiUnavailable,
    /// 有 API 但当前硬件不具备
    HardwareUnavailable,
    /// 未实现（本 lab 尚未做）
    NotImplemented,
    /// 尚未在目标环境验证
    Unknown,
}

/// 带状态的探测值：`not-run`/`unavailable` 必须携带原因。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum ProbeValue<T> {
    Observed { value: T },
    NotRun { why: String },
    Unavailable { why: String },
}

impl<T> ProbeValue<T> {
    pub fn observed(&self) -> Option<&T> {
        match self {
            Self::Observed { value } => Some(value),
            _ => None,
        }
    }
}

/// 一条白名单探测命令（审计用：程序、参数、只读用途）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbeCommand {
    pub program: &'static str,
    pub args: &'static [&'static str],
    pub os: &'static str,
    pub why: &'static str,
}

/// 探测白名单。**新增条目必须只读、固定参数、非 root、不经 shell。**
pub const PROBE_COMMANDS: &[ProbeCommand] = &[
    ProbeCommand {
        program: "/sbin/ifconfig",
        args: &["-l"],
        os: "macos",
        why: "枚举网络接口名（只读，仅名字）",
    },
    ProbeCommand {
        program: "ip",
        args: &["-o", "link", "show"],
        os: "linux",
        why: "枚举网络接口名（只读，仅名字）",
    },
    ProbeCommand {
        program: "netsh",
        args: &["interface", "show", "interface"],
        os: "windows",
        why: "枚举网络接口名（只读，仅名字）",
    },
];

/// 一次实际执行的命令证据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommandOutcome {
    pub program: String,
    pub args: Vec<String>,
    pub exit_code: Option<i32>,
    pub lines: usize,
}

/// 平台只读报告（`xinterop doctor --json` 的输出体）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlatformReport {
    pub os: String,
    pub arch: String,
    /// 实际执行过的白名单命令及退出码（证据，不是声明）
    pub commands: Vec<CommandOutcome>,
    pub network_interfaces: ProbeValue<Vec<String>>,
    pub wifi_p2p: ProbeValue<SupportLevel>,
    pub wifi_display: ProbeValue<SupportLevel>,
    pub media_outputs: ProbeValue<Vec<String>>,
    /// 能否向用户发起交互审批（stdin 是否连着终端）
    pub interactive_session: ProbeValue<bool>,
    pub permissions: ProbeValue<SupportLevel>,
}

fn basename(program: &str) -> &str {
    program.rsplit('/').next().unwrap_or(program)
}

/// 从白名单命令的 stdout 解析接口名（**只取名字**；地址/MAC/状态一律丢弃）。
pub fn parse_interface_names(program: &str, stdout: &str) -> Vec<String> {
    match basename(program) {
        "ifconfig" => stdout
            .split_whitespace()
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect(),
        "ip" => stdout
            .lines()
            .filter_map(|line| {
                // "1: lo: <LOOPBACK,UP> ... " —— 首段必须是数字索引
                let (idx, rest) = line.split_once(':')?;
                if idx.trim().parse::<u32>().is_err() {
                    return None;
                }
                let name = rest.trim_start().split(':').next()?.trim();
                if name.is_empty() || name.len() > 64 || name.contains('/') {
                    return None;
                }
                Some(name.to_string())
            })
            .collect(),
        "netsh" => stdout
            .lines()
            .skip(1) // 表头
            .filter_map(|line| {
                let t = line.trim();
                if t.is_empty() {
                    return None;
                }
                let mut it = t.split_whitespace();
                let (_admin, _state, _ty) = (it.next()?, it.next()?, it.next()?);
                let name = it.collect::<Vec<_>>().join(" ");
                (!name.is_empty()).then_some(name)
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// 本机只读探测。无网络变更、无提权操作。
pub fn probe_local() -> PlatformReport {
    let os = std::env::consts::OS.to_string();
    let arch = std::env::consts::ARCH.to_string();
    let mut commands: Vec<CommandOutcome> = Vec::new();
    let mut interfaces = ProbeValue::NotRun {
        why: format!("{os}: 白名单中没有匹配的接口枚举命令"),
    };

    for cmd in PROBE_COMMANDS.iter().filter(|c| c.os == os) {
        let out = Command::new(cmd.program).args(cmd.args).output();
        match out {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                commands.push(CommandOutcome {
                    program: cmd.program.to_string(),
                    args: cmd.args.iter().map(|s| s.to_string()).collect(),
                    exit_code: out.status.code(),
                    lines: stdout.lines().count(),
                });
                if out.status.success() {
                    let names = parse_interface_names(cmd.program, &stdout);
                    if !names.is_empty() {
                        interfaces = ProbeValue::Observed { value: names };
                    } else {
                        interfaces = ProbeValue::NotRun {
                            why: format!("{} 输出无法解析出接口名", cmd.program),
                        };
                    }
                }
            }
            Err(e) => {
                commands.push(CommandOutcome {
                    program: cmd.program.to_string(),
                    args: cmd.args.iter().map(|s| s.to_string()).collect(),
                    exit_code: None,
                    lines: 0,
                });
                interfaces = ProbeValue::NotRun {
                    why: format!("{} 不可执行：{e}", cmd.program),
                };
            }
        }
    }

    let (wifi_p2p, wifi_display) = match os.as_str() {
        "macos" => (
            ProbeValue::Unavailable {
                why: "macOS 无公开 Wi-Fi Direct (P2P) API；AWDL 为 Apple 私有，不探测、不假设".into(),
            },
            ProbeValue::Unavailable {
                why: "macOS 无公开 WFD/Sink API——plans/01 T12：没有实现的 Mac WFD 明确不可用".into(),
            },
        ),
        "linux" | "windows" => (
            ProbeValue::NotRun {
                why: format!("{os}: 本阶段未在目标机运行 probe（需对应环境）"),
            },
            ProbeValue::NotRun {
                why: format!("{os}: 本阶段未在目标机运行 probe（需对应环境）"),
            },
        ),
        other => (
            ProbeValue::NotRun {
                why: format!("未知平台 {other}"),
            },
            ProbeValue::NotRun {
                why: format!("未知平台 {other}"),
            },
        ),
    };

    PlatformReport {
        os,
        arch,
        commands,
        network_interfaces: interfaces,
        wifi_p2p,
        wifi_display,
        media_outputs: ProbeValue::NotRun {
            why: "native/file sink 属阶段 3（T29）；T07 mock host 对媒体显式拒绝——不得声称可用"
                .into(),
        },
        interactive_session: ProbeValue::Observed {
            value: std::io::stdin().is_terminal(),
        },
        permissions: ProbeValue::Unavailable {
            why: "无公开 API 读取 TCC/权限状态；授权路径见 docs/07，不猜测".into(),
        },
    }
}
