//! T01 验收 case 的可运行测试（plans/01-foundation.md §T01）。
//!
//! - **T01-01 实现workspace依赖图**：无 Xross 账号、网络栈、GUI 或 external 路径依赖。
//! - **T01-02 提交前扫描lab索引**：不含 token、原始私有抓包、第三方 vendor 树。
//!
//! 仓库卫生 tripwire：零外部依赖，手工解析 manifest 文本与 git 索引。
//! 自检用例用人工构造的假凭据证明扫描器真的能发现违规，而不是恒真断言；
//! 模式常量一律用 `concat!` 分段构造，避免本测试源码入库后被自己扫描命中。
//!
//! 复制 `impl/` 子集到未来实现仓时，lab 相关断言会因找不到 lab 标记文件而跳过。
#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// 定位
// ---------------------------------------------------------------------------

/// `impl/` 根（本测试位于 impl/crates/interop-contract/tests/）。
fn impl_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("impl/ 根必须存在")
}

/// lab 仓库根：仅当存在 lab 标记文件时返回（否则视为 impl/ 已被复制出去）。
fn lab_root() -> Option<PathBuf> {
    let root = impl_root().join("..").canonicalize().ok()?;
    let is_lab = root.join("AGENTS.md").is_file()
        && root.join("references/repositories.json").is_file();
    is_lab.then_some(root)
}

fn read_text(path: &Path) -> String {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("读取 {} 失败: {e}", path.display()))
}

// ---------------------------------------------------------------------------
// manifest 解析（无 toml 依赖的足够精确子集）
// ---------------------------------------------------------------------------

/// 提取 `members = [ ... ]` 中的引号字符串。
fn extract_members(manifest: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_members = false;
    for line in manifest.lines() {
        let l = line.trim();
        if in_members {
            if l.starts_with(']') {
                break;
            }
            push_quoted(l, &mut out);
        } else if let Some((k, v)) = l.split_once('=') {
            if k.trim() == "members" && v.trim_start().starts_with('[') {
                in_members = !v.contains(']');
                push_quoted(v, &mut out);
            }
        }
    }
    out
}

fn push_quoted(s: &str, out: &mut Vec<String>) {
    let mut rest = s;
    while let Some(start) = rest.find('"') {
        let after = &rest[start + 1..];
        match after.find('"') {
            Some(end) => {
                out.push(after[..end].to_string());
                rest = &after[end + 1..];
            }
            None => break,
        }
    }
}

/// `[package] name = "..."`；workspace 根没有 package 时返回 None。
fn package_name(manifest: &str) -> Option<String> {
    let mut in_package = false;
    for line in manifest.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_package = l == "[package]";
            continue;
        }
        if in_package {
            if let Some((k, v)) = l.split_once('=') {
                if k.trim() == "name" {
                    let names = extract_quoted_first(v);
                    if let Some(n) = names {
                        return Some(n);
                    }
                }
            }
        }
    }
    None
}

fn extract_quoted_first(s: &str) -> Option<String> {
    let start = s.find('"')?;
    let after = &s[start + 1..];
    let end = after.find('"')?;
    Some(after[..end].to_string())
}

/// 依赖项： (section, crate名, 原始行)。
/// 支持 [dependencies]/[dev-dependencies]/[build-dependencies]、
/// [dependencies.foo] 头、[target.'cfg(..)'.dependencies] 与 [workspace.dependencies]。
fn extract_deps(manifest: &str) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let mut section = String::new();
    for raw in manifest.lines() {
        let line = raw.trim();
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].to_string();
            continue;
        }
        if section.is_empty() || line.is_empty() || line.starts_with('#') {
            continue;
        }
        let seg: Vec<&str> = section.split('.').collect();
        let last = *seg.last().unwrap_or(&"");
        let is_dep_section = matches!(
            last,
            "dependencies" | "dev-dependencies" | "build-dependencies"
        );
        if !is_dep_section {
            continue;
        }
        // [dependencies.foo] 头本身就是一个依赖声明
        if seg.len() == 2 && seg[0] != "target" {
            out.push((section.clone(), seg[1].to_string(), line.to_string()));
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            // `serde.workspace = true` 的依赖名是首段
            let name = k.trim().split('.').next().unwrap_or("").to_string();
            if name.is_empty() {
                continue;
            }
            out.push((section.clone(), name, v.trim().to_string()));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// T01-01：依赖图规则
// ---------------------------------------------------------------------------

/// 任何层都禁止：GUI 与 Xross 账号/Fabric 栈。
const FORBIDDEN_EVERYWHERE: &[&str] = &[
    "tauri", "wry", "winit", "gtk", "gdk", "glib", "qt", "slint", "iced", "egui", "eframe",
    "druid", // GUI
    "iroh", "workos", // Xross 账号 / Fabric / 身份栈
];

/// core 层（contract/policy/file）额外禁止：网络栈与异步运行时。
const FORBIDDEN_IN_CORE: &[&str] = &[
    "tokio", "async-std", "smol", "hyper", "axum", "warp", "actix", "reqwest", "quinn",
    "libp2p", "socket2", "hickory", "trust-dns", "futures",
];

/// 分层：契约/策略/存储是 core；runtime/ipc/testkit/apps/adapters 允许受控网络栈。
fn tier_of(crate_name: &str) -> &'static str {
    match crate_name {
        "interop-contract" | "interop-policy" | "interop-file" => "core",
        _ => "orchestrator",
    }
}

fn is_forbidden_dep(tier: &str, dep: &str) -> bool {
    let hit = |list: &[&str]| {
        list.iter()
            .any(|f| dep == *f || dep.starts_with(&format!("{f}-")) || dep.contains("xross"))
    };
    hit(FORBIDDEN_EVERYWHERE) || (tier == "core" && hit(FORBIDDEN_IN_CORE))
}

/// 词法规范化绝对路径（不要求目标存在），用于检查路径依赖不逃逸 impl/。
fn lexical_join(base: &Path, rel: &str) -> PathBuf {
    let mut p = base.to_path_buf();
    for comp in Path::new(rel).components() {
        match comp {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                p.pop();
            }
            std::path::Component::Normal(c) => p.push(c),
            _ => {}
        }
    }
    p
}

fn path_dep_escapes(manifest_dir: &Path, impl_root: &Path, dep_value: &str) -> bool {
    let Some(start) = dep_value.find("path") else {
        return false;
    };
    let Some(q1) = dep_value[start..].find('"') else {
        return false;
    };
    let after = &dep_value[start + q1 + 1..];
    let Some(q2) = after.find('"') else {
        return false;
    };
    let p = &after[..q2];
    if p.starts_with('/') {
        return true; // 绝对路径依赖一律禁止
    }
    !lexical_join(manifest_dir, p).starts_with(impl_root)
}

#[test]
fn t01_01_dependency_graph_is_clean() {
    let root = impl_root();

    // 工具链锁：stable 具体版本（改版本必须显式改本断言）
    let toolchain = read_text(&root.join("rust-toolchain.toml"));
    let channel = toolchain
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            if l.starts_with("channel") {
                extract_quoted_first(l)
            } else {
                None
            }
        })
        .next();
    assert_eq!(
        channel.as_deref(),
        Some("1.98.0"),
        "impl/rust-toolchain.toml 必须锁定具体 stable 版本 1.98.0"
    );

    // workspace README
    let readme_path = root.join("README.md");
    assert!(readme_path.is_file(), "impl/README.md 必须存在");
    assert!(
        std::fs::metadata(&readme_path).map(|m| m.len() > 0).unwrap_or(false),
        "impl/README.md 不能为空"
    );

    // 成员集合
    let ws = read_text(&root.join("Cargo.toml"));
    assert!(ws.contains("[workspace]"), "impl/Cargo.toml 必须是 workspace 根");
    let members = extract_members(&ws);
    assert!(!members.is_empty(), "workspace 至少声明一个 member");
    for m in &members {
        assert!(
            lexical_join(&root, m).starts_with(&root),
            "member {m} 逃逸 impl/ 根"
        );
    }

    // 待扫描 manifest：workspace 根 + 各成员
    let mut manifests: Vec<(PathBuf, String)> = vec![(root.clone(), ws)];
    for m in &members {
        let dir = root.join(m);
        let mp = dir.join("Cargo.toml");
        assert!(mp.is_file(), "member {m} 缺少 Cargo.toml");
        manifests.push((dir, read_text(&mp)));
    }

    for (dir, text) in &manifests {
        let tier = match package_name(text) {
            Some(name) => tier_of(&name).to_string(),
            None => "workspace-root".to_string(), // [workspace.dependencies] 只查全局禁项
        };
        for (section, dep, line) in extract_deps(text) {
            assert!(
                tier == "workspace-root" || !is_forbidden_dep(&tier, &dep),
                "{section} 中发现禁止依赖 `{dep}`（tier={tier}）；\
                 如确需引入，必须先修订 tests/t01_workspace_hygiene.rs 的分层规则并记录来源决议"
            );
            assert!(
                !line.contains("git =") && !line.contains("git="),
                "{section} 中 `{dep}` 使用 git 来源依赖；浮动依赖禁止，须走来源 gate"
            );
            assert!(
                !path_dep_escapes(dir, &root, &line),
                "{section} 中 `{dep}` 的路径依赖逃逸 impl/（禁止引用 lab/external 或绝对路径）"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// T01-02：lab 索引卫生
// ---------------------------------------------------------------------------

fn path_forbidden(p: &str) -> bool {
    let pl = p.to_ascii_lowercase();
    let name = pl.rsplit('/').next().unwrap_or(&pl);
    let ext_forbidden = ["pcap", "pcapng", "har", "pem", "p12", "mobileconfig"]
        .iter()
        .any(|e| name.ends_with(&format!(".{e}")));
    let env_forbidden = name == ".env" || name.starts_with(".env.");
    let vendor_tree = pl.starts_with("vendor/")
        || pl.contains("/vendor/")
        || pl.starts_with("captures/")
        || pl.contains("/captures/")
        || pl.starts_with("references/external/");
    ext_forbidden || env_forbidden || vendor_tree
}

// 秘密 tripwire 模式：concat! 分段构造，源码中不含完整模式。
const PEM_GENERIC: &str = concat!("-----BEGIN", " PRIVATE KEY-----");
const PEM_RSA: &str = concat!("-----BEGIN", " RSA PRIVATE KEY-----");
const PEM_OPENSSH: &str = concat!("-----BEGIN", " OPENSSH PRIVATE KEY-----");
const PEM_EC: &str = concat!("-----BEGIN", " EC PRIVATE KEY-----");

const MAX_SCAN_BYTES: u64 = 4 * 1024 * 1024;

/// 高置信 token 模式扫描。返回命中的模式描述；None 表示干净。
/// 这是 tripwire：漏报可接受（由人工审查兜底），误报需人工核实。
fn scan_secret_patterns(b: &[u8]) -> Option<&'static str> {
    for (pat, desc) in [
        (PEM_GENERIC, "PEM私钥头"),
        (PEM_RSA, "PEM RSA私钥头"),
        (PEM_OPENSSH, "PEM OpenSSH私钥头"),
        (PEM_EC, "PEM EC私钥头"),
    ] {
        if b.windows(pat.len()).any(|w| w == pat.as_bytes()) {
            return Some(desc);
        }
    }
    if b.windows(20).any(|w| {
        w.starts_with(b"AKIA")
            && w[4..].iter().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    }) {
        return Some("AWS访问密钥形状");
    }
    // gh{o,p,r,s,u}_ + 36 alnum
    if b.windows(40).any(|w| {
        w[0] == b'g'
            && w[1] == b'h'
            && matches!(w[2], b'o' | b'p' | b'r' | b's' | b'u')
            && w[3] == b'_'
            && w[4..].iter().all(|c| c.is_ascii_alphanumeric())
    }) {
        return Some("GitHub token形状");
    }
    if find_prefixed_alnum_run(b, b"github_pat_", 30) {
        return Some("GitHub fine-grained token形状");
    }
    if find_prefixed_alnum_run(b, b"xox", 20) {
        return Some("Slack token形状");
    }
    // sk- + 长alnum（阈值 30 避开普通英文词组如 risk-assessment）
    if find_prefixed_alnum_run(b, b"sk-", 30) {
        return Some("sk- API key形状");
    }
    None
}

/// 是否存在 `prefix` 后紧跟 ≥`min_run` 个连续 alnum 的位置。
fn find_prefixed_alnum_run(b: &[u8], prefix: &[u8], min_run: usize) -> bool {
    if b.len() < prefix.len() {
        return false;
    }
    (0..=b.len() - prefix.len()).any(|i| {
        &b[i..i + prefix.len()] == prefix && {
            b[i + prefix.len()..]
                .iter()
                .take_while(|c| c.is_ascii_alphanumeric())
                .count()
                >= min_run
        }
    })
}

#[test]
fn t01_02_lab_index_is_clean() {
    let Some(lab) = lab_root() else {
        eprintln!("SKIP: 未检测到 lab 仓库标记（AGENTS.md + references/repositories.json），impl/ 可能已被复制为独立仓库");
        return;
    };
    let out = std::process::Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(&lab)
        .output()
        .expect("运行 git ls-files");
    assert!(out.status.success(), "git ls-files 失败");
    let paths: Vec<String> = out
        .stdout
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect();
    assert!(!paths.is_empty(), "lab 索引为空，扫描无意义");

    let path_violations: Vec<&String> =
        paths.iter().filter(|p| path_forbidden(p)).collect();
    assert!(
        path_violations.is_empty(),
        "T01-02 索引中发现违禁路径（抓包/凭据/vendor树/外部源码）: {path_violations:?}"
    );

    let mut content_violations: Vec<(String, &'static str)> = Vec::new();
    for p in &paths {
        let full = lab.join(p);
        let Ok(meta) = std::fs::metadata(&full) else {
            continue;
        };
        if !meta.is_file() || meta.len() > MAX_SCAN_BYTES {
            continue;
        }
        let Ok(bytes) = std::fs::read(&full) else {
            continue;
        };
        if let Some(desc) = scan_secret_patterns(&bytes) {
            content_violations.push((p.clone(), desc));
        }
    }
    assert!(
        content_violations.is_empty(),
        "T01-02 索引内容命中高置信 secret 模式（请人工核实后处理）: {content_violations:?}"
    );
}

// ---------------------------------------------------------------------------
// 扫描器自检：证明规则真的能发现违规（fixture 为人工构造的假凭据）
// ---------------------------------------------------------------------------

#[test]
fn scanners_detect_seeded_violations() {
    // 假 AWS 形状（AWS 官方文档公开示例 key，经 concat! 分段避免源码自命中）
    let fake_aws = concat!("AKIA", "IOSFODNN7EXAMPLE");
    assert_eq!(
        scan_secret_patterns(fake_aws.as_bytes()),
        Some("AWS访问密钥形状")
    );
    let fake_pem = concat!("-----BEGIN", " RSA PRIVATE KEY-----");
    assert_eq!(scan_secret_patterns(fake_pem.as_bytes()), Some("PEM RSA私钥头"));
    assert_eq!(scan_secret_patterns(b"clean text, no secrets"), None);

    // 依赖规则自检
    assert!(is_forbidden_dep("core", "tokio"));
    assert!(is_forbidden_dep("core", "tokio-util"));
    assert!(is_forbidden_dep("orchestrator", "tauri"));
    assert!(!is_forbidden_dep("orchestrator", "tokio")); // runtime/ipc 层受控允许
    assert!(!is_forbidden_dep("core", "serde"));

    let bad_manifest = "[dependencies]\ntokio = \"1\"\n";
    let deps = extract_deps(bad_manifest);
    assert_eq!(deps.len(), 1);
    assert!(is_forbidden_dep("core", &deps[0].1));

    // 路径逃逸自检（从 impl/crates/interop-contract 上跳 3 级出 impl/）
    let esc = path_dep_escapes(
        Path::new("/x/impl/crates/interop-contract"),
        Path::new("/x/impl"),
        r#"{ path = "../../../references/external/r01-uxplay" }"#,
    );
    assert!(esc);
    let ok = !path_dep_escapes(
        Path::new("/x/impl/crates/interop-contract"),
        Path::new("/x/impl"),
        r#"{ path = "../interop-policy" }"#,
    );
    assert!(ok);
}
