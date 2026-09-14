//! 不可信名称 → 逐 component 安全路径（docs/07 §3）。
//!
//! 拒绝（不是改名）：空段、`.`、`..`、绝对/UNC/drive 形状、NUL/控制字符、
//! 路径分隔符注入、Windows 保留名。拒绝发生在形状层，先于任何文件系统操作。

use interop_contract::error::{Error, ErrorCode};

/// 安全化的相对路径（components 已验证）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentPath(pub Vec<String>);

impl ComponentPath {
    pub fn components(&self) -> &[String] {
        &self.0
    }
}

/// 由可选目录 components + display_name 构造安全相对路径。
pub fn sanitize(
    components: Option<&[String]>,
    display_name: &str,
) -> Result<ComponentPath, Error> {
    let mut out = Vec::new();
    if let Some(cs) = components {
        for c in cs {
            out.push(check_component(c)?);
        }
    }
    out.push(check_component(display_name)?);
    Ok(ComponentPath(out))
}

fn check_component(c: &str) -> Result<String, Error> {
    let reject = |why: &str| {
        Err(Error::new(
            ErrorCode::InvalidFrame,
            format!("路径组件 {c:?} 被拒绝：{why}"),
        )
        .with_phase("offered"))
    };
    if c.is_empty() || c == "." || c == ".." {
        return reject("空段/当前目录/父目录引用");
    }
    if c.len() > 255 {
        return reject("超过常见文件系统 255 字节名称上限");
    }
    if c.bytes().any(|b| b == 0 || b < 0x20) {
        return reject("含 NUL/控制字符");
    }
    if c.contains('/') || c.contains('\\') {
        return reject("含路径分隔符");
    }
    if c.contains(':') {
        // 覆盖 drive（C:）、Alternate Data Stream（name:stream）形状
        return reject("含冒号（drive/ADS 形状）");
    }
    if c.starts_with('/') {
        return reject("绝对路径段");
    }
    // Windows 保留名（大小写不敏感；含带扩展名形状 CON.txt）
    let stem = c.split('.').next().unwrap_or("").to_ascii_uppercase();
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
        "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if RESERVED.contains(&stem.as_str()) {
        return reject("Windows 保留设备名");
    }
    Ok(c.to_string())
}
