//! Worker supervisor（plans/01 T13；ADR-003 的隔离证据面）。
//!
//! 外部协议引擎（provider）一律以**独立低权限进程**运行，由本模块记账：
//! - **binary hash 固定**：profile 里 pin 了 sha256 就必须匹配（不匹配 → `integrity-failed`，
//!   不生成进程）；未 pin（dev profile）则在首次启动时计算并 pin 住。
//! - **参数白名单**：调用方只能选 profile 与 mode，不能追加任意参数（协议在 `args` 里）。
//! - **bootstrap 走 parent pipe**：stdin/stdout 两条管道，stderr 丢弃；**环境清空**后只注入
//!   白名单变量与 `INTEROP_WORKER_ID`。
//! - **有限重启**：窗口内崩溃次数超过 `max_restarts` → 停止自动重启并记录事件。
//! - **关闭无孤儿**：关闭 stdin（EOF 指令）→ 等待退出 → 超时才 kill；listener 文件必须消失。
//! - **隔离等级如实标注**：canary 探针给出证据；只有 OS 沙箱真的拒绝读取时才报
//!   `platform-sandbox`，否则报 `process-only` 并限制可发布 profile（不把进程边界说成 sandbox）。

use interop_contract::error::{Error, ErrorCode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};

/// macOS 平台沙箱载体（弃用 API，但仍是真实的 OS 沙箱；不可用时降级为 process-only）。
const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

/// 实际生效的隔离等级（以证据为准，不是意图）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum IsolationLevel {
    /// 仅进程边界：worker 与 host 同用户、同文件权限——**不是** sandbox
    ProcessOnly,
    /// OS 沙箱生效（本机 = macOS sandbox-exec），canary 读取被内核拒绝
    PlatformSandbox,
}

/// worker profile（`impl/policies/worker-profiles.json` 的条目）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerProfile {
    pub profile_id: String,
    /// 二进制路径（相对 impl/ 根或绝对）
    pub binary: String,
    /// pin 的 sha256；None = dev profile，首次启动时计算并 pin
    #[serde(default)]
    pub sha256: Option<String>,
    /// 固定参数（调用方不可追加）
    #[serde(default)]
    pub args: Vec<String>,
    /// 允许从父进程继承的环境变量（其余一律清空）
    #[serde(default)]
    pub env_allow: Vec<String>,
    #[serde(default = "default_max_restarts")]
    pub max_restarts: u32,
    #[serde(default = "default_window_ms")]
    pub restart_window_ms: u64,
    /// 是否用平台沙箱运行（不可用时如实降级并记录）
    #[serde(default)]
    pub sandbox: bool,
}

fn default_max_restarts() -> u32 {
    4
}
fn default_window_ms() -> u64 {
    300_000
}

/// supervisor 事件（审计）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "event", rename_all = "kebab-case")]
pub enum SupervisorEvent {
    Spawned {
        profile_id: String,
        pid: u32,
        mode: String,
        sandboxed: bool,
    },
    ExecDenied {
        profile_id: String,
        request: String,
        code: String,
    },
    Exited {
        profile_id: String,
        code: Option<i32>,
    },
    RestartLimitReached {
        profile_id: String,
        restarts: u32,
        window_ms: u64,
    },
    Shutdown {
        workers: usize,
        killed: usize,
        orphan_listeners: usize,
    },
}

/// canary 探针结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "kebab-case")]
pub enum CanaryOutcome {
    /// OS 拒绝读取（真实隔离）
    Denied { detail: String },
    /// 读到了：说明当前等级下没有隔离——必须如实上报
    Readable { detail: String },
}

/// 隔离报告（`process-only` 时必须列出受限 profile）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IsolationReport {
    pub profile_id: String,
    pub requested_sandbox: bool,
    pub actual: IsolationLevel,
    pub canary: CanaryOutcome,
    pub note: String,
    /// 在 `process-only` 等级下不得发布/放行的 profile
    pub restricted_profiles: Vec<String>,
}

/// 启动参数（调用方可控的部分，全部受限）。
#[derive(Debug, Clone, Default)]
pub struct StartOptions {
    pub canary_path: Option<PathBuf>,
    pub listener_path: Option<PathBuf>,
    pub env_vars: Vec<String>,
    /// 检查白名单传递的环境变量（仅 dump_env 模式用）
    pub force_sandbox: bool,
}

struct RunningWorker {
    profile_id: String,
    pid: u32,
    mode: String,
    sandboxed: bool,
    listener_path: Option<PathBuf>,
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    buffered: Vec<String>,
}

impl RunningWorker {
    fn drain(&mut self) -> Vec<String> {
        while let Ok(line) = self.lines.try_recv() {
            self.buffered.push(line);
        }
        std::mem::take(&mut self.buffered)
    }
}

/// worker supervisor（唯一记账方）。
#[derive(Default)]
pub struct WorkerSupervisor {
    profiles: HashMap<String, WorkerProfile>,
    pinned: HashMap<String, String>,
    running: Vec<RunningWorker>,
    log: Vec<SupervisorEvent>,
    spawns: HashMap<String, u32>,
    crashes: HashMap<String, Vec<u64>>,
    stopped: Vec<String>,
    last_opts: HashMap<String, (String, StartOptions)>,
    sandbox_available: bool,
    shutting_down: bool,
}

impl WorkerSupervisor {
    pub fn new() -> Self {
        Self {
            sandbox_available: Path::new(SANDBOX_EXEC).is_file(),
            ..Self::default()
        }
    }

    /// 平台沙箱载体是否可用（macOS sandbox-exec）。
    pub fn sandbox_available(&self) -> bool {
        self.sandbox_available
    }

    /// 从 JSON 文件加载 profiles（数组）。
    pub fn load_profiles(&mut self, path: &Path) -> Result<usize, Error> {
        let text = std::fs::read_to_string(path).map_err(|e| {
            Error::new(
                ErrorCode::PlatformUnavailable,
                format!("profile 文件不可读 {}：{e}", path.display()),
            )
        })?;
        let list: Vec<WorkerProfile> = serde_json::from_str(&text).map_err(|e| {
            Error::new(
                ErrorCode::InvalidFrame,
                format!("profile JSON 非法 {}：{e}", path.display()),
            )
        })?;
        let n = list.len();
        for p in list {
            self.profiles.insert(p.profile_id.clone(), p);
        }
        Ok(n)
    }

    pub fn add_profile(&mut self, profile: WorkerProfile) {
        self.profiles.insert(profile.profile_id.clone(), profile);
    }

    pub fn pinned_hash(&self, profile_id: &str) -> Option<&str> {
        self.pinned.get(profile_id).map(String::as_str)
    }

    pub fn log(&self) -> &[SupervisorEvent] {
        &self.log
    }

    pub fn worker_count(&self) -> usize {
        self.running.len()
    }

    /// 当前运行的 worker：`(profile_id, pid, mode)`（运维/诊断用）。
    pub fn running(&self) -> Vec<(String, u32, String)> {
        self.running
            .iter()
            .map(|w| (w.profile_id.clone(), w.pid, w.mode.clone()))
            .collect()
    }

    pub fn spawn_count(&self, profile_id: &str) -> u32 {
        *self.spawns.get(profile_id).unwrap_or(&0)
    }

    pub fn is_stopped(&self, profile_id: &str) -> bool {
        self.stopped.iter().any(|p| p == profile_id)
    }

    /// 启动一个 worker。hash 校验失败 / profile 未知 → 显式错误，不生成进程。
    pub fn start(
        &mut self,
        profile_id: &str,
        mode: &str,
        opts: StartOptions,
        now_ms: u64,
    ) -> Result<u32, Error> {
        let profile = self
            .profiles
            .get(profile_id)
            .cloned()
            .ok_or_else(|| unknown_profile(profile_id))?;
        if self.is_stopped(profile_id) {
            return Err(Error::new(
                ErrorCode::Busy,
                format!("profile {profile_id} 已达到重启上限，需人工介入"),
            ));
        }
        let binary = self.resolve_binary(&profile)?;
        let hash = self.verify_or_pin(&profile, &binary)?;
        let _ = hash;

        let sandboxed = profile.sandbox && self.sandbox_available;
        let mut cmd = if sandboxed {
            let profile_file = self.write_sandbox_profile(&opts)?;
            let mut c = Command::new(SANDBOX_EXEC);
            c.arg("-f").arg(profile_file);
            c.arg(&binary);
            c
        } else {
            Command::new(&binary)
        };
        cmd.args(&profile.args)
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        for var in &profile.env_allow {
            if let Some(v) = std::env::var_os(var) {
                cmd.env(var, v);
            }
        }
        cmd.env("INTEROP_WORKER_ID", profile_id)
            .env("INTEROP_WORKER_MODE", mode);

        let mut child = cmd.spawn().map_err(|e| {
            Error::new(
                ErrorCode::ProviderCrashed,
                format!("worker 启动失败 {profile_id}：{e}"),
            )
        })?;
        let pid = child.id();
        let stdout = child.stdout.take().expect("piped stdout");
        let mut stdin = child.stdin.take().expect("piped stdin");

        // bootstrap：一行配置（然后保持管道打开；关闭 = EOF 指令）
        let mut cfg_line = format!("mode={mode}");
        if let Some(p) = &opts.canary_path {
            cfg_line.push_str(&format!(" canary_path={}", p.display()));
        }
        if let Some(p) = &opts.listener_path {
            cfg_line.push_str(&format!(" listener_path={}", p.display()));
        }
        if !opts.env_vars.is_empty() {
            cfg_line.push_str(&format!(" env_vars={}", opts.env_vars.join(",")));
        }
        use std::io::Write as _;
        writeln!(stdin, "{cfg_line}").map_err(|e| {
            Error::new(
                ErrorCode::ProviderCrashed,
                format!("worker bootstrap 写入失败：{e}"),
            )
        })?;

        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(l) => {
                        if tx.send(l).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        *self.spawns.entry(profile_id.to_string()).or_insert(0) += 1;
        self.running.push(RunningWorker {
            profile_id: profile_id.to_string(),
            pid,
            mode: mode.to_string(),
            sandboxed,
            listener_path: opts.listener_path.clone(),
            child,
            stdin: Some(stdin),
            lines: rx,
            buffered: Vec::new(),
        });
        self.last_opts
            .insert(profile_id.to_string(), (mode.to_string(), opts));
        self.log.push(SupervisorEvent::Spawned {
            profile_id: profile_id.to_string(),
            pid,
            mode: mode.to_string(),
            sandboxed,
        });
        let _ = now_ms;
        Ok(pid)
    }

    /// 处理 worker 输出：`exec-request:` 一律拒绝（arbitrary shell 是禁止形状），
    /// 并检测退出与重启上限。
    pub fn poll(&mut self, now_ms: u64) -> Vec<SupervisorEvent> {
        let mut out = Vec::new();
        let mut replies: Vec<(usize, String)> = Vec::new();
        for (idx, worker) in self.running.iter_mut().enumerate() {
            for line in worker.drain() {
                if let Some(req) = line.strip_prefix("exec-request:") {
                    let _ = &req;
                    worker.buffered.push(line.clone());
                    replies.push((idx, "denied:auth-denied\n".to_string()));
                    self.log.push(SupervisorEvent::ExecDenied {
                        profile_id: worker.profile_id.clone(),
                        request: req.to_string(),
                        code: "auth-denied".into(),
                    });
                } else {
                    worker.buffered.push(line);
                }
            }
        }
        for (idx, reply) in replies {
            if let Some(stdin) = self.running[idx].stdin.as_mut() {
                use std::io::Write as _;
                let _ = stdin.write_all(reply.as_bytes());
                let _ = stdin.flush();
            }
        }

        // 退出检测 + 自动重启
        let mut i = 0;
        while i < self.running.len() {
            let status = self.running[i]
                .child
                .try_wait()
                .ok()
                .flatten();
            if let Some(status) = status {
                let worker = self.running.remove(i);
                self.log.push(SupervisorEvent::Exited {
                    profile_id: worker.profile_id.clone(),
                    code: status.code(),
                });
                out.push(SupervisorEvent::Exited {
                    profile_id: worker.profile_id.clone(),
                    code: status.code(),
                });
                if !self.shutting_down {
                    self.maybe_restart(&worker.profile_id, now_ms, &mut out);
                }
            } else {
                i += 1;
            }
        }
        out
    }

    fn maybe_restart(&mut self, profile_id: &str, now_ms: u64, out: &mut Vec<SupervisorEvent>) {
        let profile = match self.profiles.get(profile_id) {
            Some(p) => p.clone(),
            None => return,
        };
        let window_start = now_ms.saturating_sub(profile.restart_window_ms);
        let crashes = self.crashes.entry(profile_id.to_string()).or_default();
        crashes.retain(|t| *t >= window_start);
        crashes.push(now_ms);
        if crashes.len() as u32 > profile.max_restarts {
            self.stopped.push(profile_id.to_string());
            let ev = SupervisorEvent::RestartLimitReached {
                profile_id: profile_id.to_string(),
                restarts: profile.max_restarts,
                window_ms: profile.restart_window_ms,
            };
            self.log.push(ev.clone());
            out.push(ev);
            return;
        }
        if let Some((mode, opts)) = self.last_opts.get(profile_id).cloned() {
            if let Ok(pid) = self.start(profile_id, &mode, opts, now_ms) {
                let _ = pid;
            }
        }
    }

    /// 读取某 worker 目前收到的全部输出行（不清空运行记录）。
    pub fn worker_lines(&self, profile_id: &str) -> Vec<String> {
        self.running
            .iter()
            .filter(|w| w.profile_id == profile_id)
            .flat_map(|w| w.buffered.clone())
            .collect()
    }

    /// 关闭：EOF → 等待退出（超时 kill）；报告孤儿 listener。
    pub fn shutdown(&mut self) -> (usize, usize, Vec<PathBuf>) {
        self.shutting_down = true;
        let workers = self.running.len();
        let mut killed = 0usize;
        let mut orphans: Vec<PathBuf> = Vec::new();
        for worker in self.running.iter_mut() {
            worker.stdin.take(); // EOF 指令
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        for worker in self.running.iter_mut() {
            loop {
                match worker.child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) => {
                        if std::time::Instant::now() > deadline {
                            let _ = worker.child.kill();
                            let _ = worker.child.wait();
                            killed += 1;
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        }
        for worker in self.running.drain(..) {
            if let Some(p) = worker.listener_path {
                if p.exists() {
                    orphans.push(p);
                }
            }
        }
        self.log.push(SupervisorEvent::Shutdown {
            workers,
            killed,
            orphan_listeners: orphans.len(),
        });
        (workers, killed, orphans)
    }

    /// canary 隔离探针：以 read_canary 模式真实跑一次，报告实际等级。
    pub fn isolation_report(
        &mut self,
        profile_id: &str,
        canary_path: &Path,
        now_ms: u64,
    ) -> Result<IsolationReport, Error> {
        let canonical = std::fs::canonicalize(canary_path).unwrap_or_else(|_| canary_path.into());
        let requested_sandbox = self
            .profiles
            .get(profile_id)
            .map(|p| p.sandbox)
            .unwrap_or(false);
        let opts = StartOptions {
            canary_path: Some(canonical.clone()),
            force_sandbox: requested_sandbox,
            ..StartOptions::default()
        };
        self.start(profile_id, "read_canary", opts, now_ms)?;
        // 等待 canary 结论（真实进程，给足时间）
        let mut outcome: Option<CanaryOutcome> = None;
        for _ in 0..200 {
            self.poll(now_ms);
            for line in self.worker_lines(profile_id) {
                if let Some(rest) = line.strip_prefix("canary:read_ok:") {
                    outcome = Some(CanaryOutcome::Readable {
                        detail: format!("worker 以当前等级成功读取 canary（{rest} 字节）"),
                    });
                } else if let Some(rest) = line.strip_prefix("canary:denied:") {
                    outcome = Some(CanaryOutcome::Denied {
                        detail: format!("OS 拒绝读取：{rest}"),
                    });
                }
            }
            if outcome.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let sandboxed = self
            .running
            .iter()
            .find(|w| w.profile_id == profile_id)
            .map(|w| w.sandboxed)
            .unwrap_or(false);
        self.shutdown();
        let outcome = outcome.unwrap_or_else(|| CanaryOutcome::Readable {
            detail: "worker 未给出 canary 结论（视为隔离未证明）".into(),
        });
        let actual = match (&outcome, sandboxed) {
            (CanaryOutcome::Denied { .. }, true) => IsolationLevel::PlatformSandbox,
            _ => IsolationLevel::ProcessOnly,
        };
        let note = match (&outcome, sandboxed, self.sandbox_available) {
            (CanaryOutcome::Denied { .. }, true, _) => {
                "OS 沙箱（macOS sandbox-exec）生效：canary 读取被内核拒绝；发布级 profile 仍需收紧读/写与网络白名单".to_string()
            }
            (CanaryOutcome::Readable { .. }, true, _) => {
                "沙箱已请求但未生效（canary 可读）：按 isolation 失败处理，等级降为 process-only".to_string()
            }
            (CanaryOutcome::Readable { .. }, false, false) => {
                "本机无可用 OS 沙箱载体：等级为 process-only（进程边界 ≠ sandbox），相关 profile 不得进入发布".to_string()
            }
            (CanaryOutcome::Readable { .. }, false, true) => {
                "profile 未启用沙箱：等级为 process-only；如该 profile 要发布必须启用平台沙箱".to_string()
            }
            (CanaryOutcome::Denied { .. }, false, _) => {
                "未用沙箱但读取被拒（外部权限原因）：不足以证明隔离，仍报 process-only".to_string()
            }
        };
        let restricted_profiles = if actual == IsolationLevel::ProcessOnly {
            vec![profile_id.to_string()]
        } else {
            Vec::new()
        };
        Ok(IsolationReport {
            profile_id: profile_id.to_string(),
            requested_sandbox,
            actual,
            canary: outcome,
            note,
            restricted_profiles,
        })
    }

    fn resolve_binary(&self, profile: &WorkerProfile) -> Result<PathBuf, Error> {
        let p = PathBuf::from(&profile.binary);
        let p = if p.is_absolute() {
            p
        } else {
            // 相对 impl/ 根解析（tests 从 crates/interop-runtime 起算）
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join(&profile.binary)
        };
        let canonical = std::fs::canonicalize(&p).map_err(|e| {
            Error::new(
                ErrorCode::PlatformUnavailable,
                format!("worker 二进制不可用 {}：{e}", p.display()),
            )
        })?;
        Ok(canonical)
    }

    fn verify_or_pin(&mut self, profile: &WorkerProfile, binary: &Path) -> Result<String, Error> {
        let bytes = std::fs::read(binary).map_err(|e| {
            Error::new(
                ErrorCode::PlatformUnavailable,
                format!("worker 二进制不可读 {}：{e}", binary.display()),
            )
        })?;
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let hash = format!("{:x}", hasher.finalize());
        match &profile.sha256 {
            Some(expected) => {
                if expected != &hash {
                    return Err(Error::new(
                        ErrorCode::IntegrityFailed,
                        format!(
                            "worker 二进制 hash 不匹配（profile={} expected={} actual={}）",
                            profile.profile_id,
                            &expected[..12.min(expected.len())],
                            &hash[..12]
                        ),
                    )
                    .with_phase("discovered"));
                }
                self.pinned
                    .insert(profile.profile_id.clone(), hash.clone());
                Ok(hash)
            }
            None => {
                self.pinned
                    .insert(profile.profile_id.clone(), hash.clone());
                Ok(hash)
            }
        }
    }

    /// macOS sandbox profile：证明机制用的**窄**策略（只拒绝 canary 子树读取）。
    /// 发布级 profile 必须收紧到读/写白名单与网络策略（见 IsolationReport.note）。
    fn write_sandbox_profile(&self, opts: &StartOptions) -> Result<PathBuf, Error> {
        let dir = std::env::temp_dir().join(format!("interop-sandbox-{}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|e| {
            Error::new(
                ErrorCode::PlatformUnavailable,
                format!("sandbox 目录不可建：{e}"),
            )
        })?;
        let file = dir.join("worker.sb");
        let deny = opts
            .canary_path
            .as_ref()
            .map(|c| {
                let parent = c.parent().unwrap_or(Path::new("/"));
                format!("(deny file-read* (subpath \"{}\"))", parent.display())
            })
            .unwrap_or_default();
        let content = format!("(version 1)\n(allow default)\n{deny}\n");
        std::fs::write(&file, content).map_err(|e| {
            Error::new(
                ErrorCode::PlatformUnavailable,
                format!("sandbox profile 不可写：{e}"),
            )
        })?;
        Ok(file)
    }
}

fn unknown_profile(id: &str) -> Error {
    Error::new(
        ErrorCode::UnsupportedProfile,
        format!("未知 worker profile {id}"),
    )
    .with_profile(id.to_string())
    .with_phase("discovered")
}
