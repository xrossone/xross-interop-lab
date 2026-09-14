//! 受限 spool：begin/write/commit/abort 语义、独占临时文件、可恢复、hash receipt。
//!
//! 安全模型：
//! - 临时文件以 `create_new` 独占创建（不跟随/不覆盖既有 link）。
//! - 写入顺序化：offset 必须 == 当前长度（checked offsets），总字节 ≤ 预算；
//!   超限 → `resource-limit` 且临时文件立即终止并删除（T08-03）。
//! - commit：hash 校验（不符 → `integrity-failed` 不发布，T08-04）→
//!   no-follow 检查 root 与全部中间目录（lstat，防"审批后目录被换成
//!   symlink"，T08-02）→ 同一文件系统内原子 rename。
//! - no-follow 检查与 rename 之间存在理论 TOCTOU 窗口；无窗口的 dirfd +
//!   O_NOFOLLOW 链是平台特定加固，列入后续 hardening（诚实记录，不声称
//!   已完全免疫并发攻击者）。

use crate::path::ComponentPath;
use interop_contract::error::{Error, ErrorCode};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// 发布回执（FILE-06：本地持久化 ≠ 远端确认）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishReceipt {
    /// 相对 spool root 的最终路径
    pub relative: Vec<String>,
    pub size: u64,
    pub sha256_hex: String,
}

/// 独占写入句柄。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteHandle(pub u64);

/// 源读取 lease（send 侧）：打开时快照文件身份，中途变更 → `source-changed`。
pub struct ReadLease {
    path: PathBuf,
    identity: SourceIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceIdentity {
    len: u64,
    modified_nanos: Option<u128>,
    #[cfg(unix)]
    inode: u64,
}

impl ReadLease {
    pub fn open(path: &Path) -> Result<Self, Error> {
        Ok(Self {
            path: path.to_path_buf(),
            identity: SourceIdentity::snapshot(path)?,
        })
    }

    /// 有界读取（offset 任意，长度有界）；每次读取前校验身份。
    pub fn read_chunk(&self, offset: u64, max_len: usize) -> Result<Vec<u8>, Error> {
        let now = SourceIdentity::snapshot(&self.path)?;
        if now != self.identity {
            return Err(Error::new(
                ErrorCode::SourceChanged,
                "源文件身份在读取中途发生变化",
            )
            .with_phase("transferring"));
        }
        if offset > self.identity.len {
            return Err(Error::new(ErrorCode::InvalidFrame, "offset 越界"));
        }
        let mut f = File::open(&self.path).map_err(io_err)?;
        f.seek(SeekFrom::Start(offset)).map_err(io_err)?;
        let mut buf = vec![0u8; max_len.min(self.identity.len as usize - offset as usize)];
        let mut read_total = 0;
        while read_total < buf.len() {
            let n = f.read(&mut buf[read_total..]).map_err(io_err)?;
            if n == 0 {
                break;
            }
            read_total += n;
        }
        buf.truncate(read_total);
        Ok(buf)
    }
}

impl SourceIdentity {
    fn snapshot(path: &Path) -> Result<Self, Error> {
        let meta = std::fs::symlink_metadata(path).map_err(|_| {
            Error::new(ErrorCode::SourceChanged, "源文件不存在/不可达")
        })?;
        Ok(Self {
            len: meta.len(),
            modified_nanos: meta.modified().ok().and_then(|m| {
                m.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_nanos())
            }),
            #[cfg(unix)]
            inode: {
                use std::os::unix::fs::MetadataExt;
                meta.ino()
            },
        })
    }
}

#[derive(Debug)]
struct SpoolEntry {
    temp_path: PathBuf,
    budget: u64,
    hasher: Sha256,
    written: u64,
    terminated: bool,
}

/// 受限 spool 存储。root 由 host 选择（provider 永不知道路径）。
#[derive(Debug)]
pub struct SpoolStore {
    root: PathBuf,
    entries: Vec<SpoolEntry>,
    published: Vec<PublishReceipt>,
    next_handle: u64,
}

impl SpoolStore {
    /// 打开/创建 spool root。root 自身是 symlink 时拒绝（T08-02 的 root 情形）。
    pub fn open(root: &Path) -> Result<Self, Error> {
        std::fs::create_dir_all(root).map_err(io_err)?;
        let md = std::fs::symlink_metadata(root).map_err(io_err)?;
        if md.is_symlink() || !md.is_dir() {
            return Err(Error::new(
                ErrorCode::InvalidFrame,
                "spool root 是 symlink 或不是目录",
            ));
        }
        Ok(Self {
            root: root.to_path_buf(),
            entries: Vec::new(),
            published: Vec::new(),
            next_handle: 1,
        })
    }

    /// 为条目开独占临时文件。declared 与 budget 取较小者作为写入上限。
    pub fn begin(&mut self, entry: &interop_contract::offer::OfferEntry, budget: u64) -> Result<WriteHandle, Error> {
        // 形状校验先于任何文件系统操作：被拒条目零写入（T08-01）
        crate::path::sanitize(entry.relative_components.as_deref(), &entry.display_name)?;
        let handle = WriteHandle(self.next_handle);
        self.next_handle += 1;
        let temp_path = self.root.join(format!(
            ".tmp-{}-{}",
            handle.0,
            entry.entry_id.as_ref()
        ));
        // 独占创建：已存在即失败（不跟随、不覆盖既有 link）
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
            .map_err(io_err)?;
        file.set_permissions_limited();
        self.entries.push(SpoolEntry {
            temp_path,
            budget: budget.min(entry.declared_size.0),
            hasher: Sha256::new(),
            written: 0,
            terminated: false,
        });
        Ok(handle)
    }

    /// 顺序写入（offset 必须 == 当前长度）。
    pub fn write(&mut self, handle: WriteHandle, offset: u64, bytes: &[u8]) -> Result<(), Error> {
        let e = self.entry_mut(handle)?;
        if e.terminated {
            return Err(terminated());
        }
        if offset != e.written {
            return Err(Error::new(
                ErrorCode::InvalidFrame,
                format!("offset {offset} 与当前长度 {} 不符（仅支持顺序写）", e.written),
            ));
        }
        if e.written + bytes.len() as u64 > e.budget {
            // 立即终止：删除临时文件、标记 terminated（T08-03）
            let path = e.temp_path.clone();
            e.terminated = true;
            let _ = std::fs::remove_file(&path);
            return Err(Error::new(
                ErrorCode::ResourceLimit,
                format!("写入超出预算 {}/{}", e.written, e.budget),
            )
            .with_phase("transferring"));
        }
        let path = e.temp_path.clone();
        let mut f = OpenOptions::new().append(true).open(&path).map_err(io_err)?;
        f.write_all(bytes).map_err(io_err)?;
        f.sync_all().map_err(io_err)?;
        let e = self.entry_mut(handle)?;
        e.hasher.update(bytes);
        e.written += bytes.len() as u64;
        Ok(())
    }

    /// 原子发布：hash 校验 → no-follow 中间目录检查 → 同 fs rename。
    pub fn commit(
        &mut self,
        handle: WriteHandle,
        final_rel: &ComponentPath,
        expected_sha256: Option<&str>,
    ) -> Result<PublishReceipt, Error> {
        // 阶段 1（借 entry）：终止态检查 + hash 校验，先把需要的事实复制出来
        let (temp_path, actual, written) = {
            let e = self.entry_mut(handle)?;
            if e.terminated {
                return Err(terminated());
            }
            let actual = hex_hash(&e.hasher);
            if let Some(expected) = expected_sha256 {
                if actual != expected.to_ascii_lowercase() {
                    let path = e.temp_path.clone();
                    e.terminated = true;
                    let _ = std::fs::remove_file(&path);
                    return Err(Error::new(
                        ErrorCode::IntegrityFailed,
                        format!("hash 不符：expected={expected} actual={actual}"),
                    )
                    .with_phase("verifying"));
                }
            }
            (e.temp_path.clone(), actual, e.written)
        };
        // 阶段 2：no-follow 检查（root + 已存在中间目录必须真目录，T08-02）
        check_no_follow(&self.root, final_rel.components())?;
        // 阶段 3：建目标目录（每级复查）+ 同文件系统原子 rename
        let mut cur = self.root.clone();
        let comps = final_rel.components();
        for dir in &comps[..comps.len() - 1] {
            cur.push(dir);
            std::fs::create_dir(&cur).map_err(io_err)?;
            let md = std::fs::symlink_metadata(&cur).map_err(io_err)?;
            if md.is_symlink() || !md.is_dir() {
                return Err(Error::new(
                    ErrorCode::InvalidFrame,
                    "目标目录是 symlink 或不是目录",
                ));
            }
        }
        let mut final_path = self.root.clone();
        for c in comps {
            final_path.push(c);
        }
        std::fs::rename(&temp_path, &final_path).map_err(io_err)?;
        // 阶段 4：记账
        let receipt = PublishReceipt {
            relative: comps.to_vec(),
            size: written,
            sha256_hex: actual,
        };
        let e = self.entry_mut(handle)?;
        e.terminated = true; // handle 用后即废
        self.published.push(receipt.clone());
        Ok(receipt)
    }

    pub fn abort(&mut self, handle: WriteHandle) -> Result<(), Error> {
        let e = self.entry_mut(handle)?;
        let path = e.temp_path.clone();
        e.terminated = true;
        let _ = std::fs::remove_file(&path);
        Ok(())
    }

    pub fn published(&self) -> &[PublishReceipt] {
        &self.published
    }

    fn entry_mut(&mut self, handle: WriteHandle) -> Result<&mut SpoolEntry, Error> {
        self.entries
            .iter_mut()
            .find(|e| e.temp_path.to_string_lossy().contains(&format!(".tmp-{}-", handle.0)))
            .ok_or_else(|| Error::new(ErrorCode::InvalidFrame, "未知 spool 句柄"))
    }
}

/// root + 已存在的中间目录：lstat 必须是真目录（拒绝 symlink，T08-02）。
fn check_no_follow(root: &Path, components: &[String]) -> Result<(), Error> {
    let md = std::fs::symlink_metadata(root).map_err(io_err)?;
    if md.is_symlink() || !md.is_dir() {
        return Err(Error::new(ErrorCode::InvalidFrame, "spool root 失效"));
    }
    let mut cur = root.to_path_buf();
    for dir in &components[..components.len().saturating_sub(1)] {
        cur.push(dir);
        if let Ok(md) = std::fs::symlink_metadata(&cur) {
            if md.is_symlink() || !md.is_dir() {
                return Err(Error::new(
                    ErrorCode::InvalidFrame,
                    format!("中间目录 {dir:?} 是 symlink/非目录（审批后可能被替换）"),
                )
                .with_phase("committing"));
            }
        }
        // 不存在的中间目录由 commit 的 create_dir 阶段创建并复查
    }
    Ok(())
}

fn terminated() -> Error {
    Error::new(ErrorCode::ResourceLimit, "spool 条目已终止，不可再用")
}

fn io_err(e: std::io::Error) -> Error {
    Error::new(ErrorCode::DestinationUnavailable, format!("spool I/O: {e}"))
        .retryable(true)
}

fn hex_hash(h: &Sha256) -> String {
    let clone = h.clone();
    crate::integrity::hex(&clone.finalize())
}

trait SetLimited {
    fn set_permissions_limited(&self);
}

#[cfg(unix)]
impl SetLimited for File {
    fn set_permissions_limited(&self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = self.set_permissions(std::fs::Permissions::from_mode(0o600));
    }
}

#[cfg(not(unix))]
impl SetLimited for File {
    fn set_permissions_limited(&self) {}
}
