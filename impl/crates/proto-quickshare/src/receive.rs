//! 接收侧会话（plans/02-files.md §T21，headless 部分）：
//! introduction → 报价投影 → 用户裁决 → payload 流式落盘 → 完整性校验后原子发布。
//!
//! 边界（本 task 不做的事）：
//! - **发现**不在本模块（P-F02-1/3 未关闭）：本层只处理"已经连上并握手完成"之后的帧；
//! - 传输加密在 [`crate::secure_message`]：本层收到的是**已解密**的 payload 帧；
//! - 落盘一律经 `interop-file` 的 spool（预算/顺序写/原子发布/hash），本层不直接碰文件路径；
//! - 对端声明的 MIME **只作提示**（FILE-02）：不参与决策、不决定扩展名处理。
//!
//! T21-01：payloadID ↔ entryID 必须一一绑定；混入别的 payload id → 拒绝且不发布。
//! T21-02：用户拒绝 → 回 REJECT，且不建立任何写句柄（不留临时文件）。
//! T21-03：常数内存——`PayloadAssembler` 只保留 O(1) 状态，chunk 直接经 sink 落盘。
//! T21-04：filename 穿越/保留名/分隔符 → 由 `interop-file` 的形状规则拒绝（先于任何写）。

use interop_contract::error::{Error, ErrorCode};
use interop_contract::offer::OfferEntry;
use interop_contract::U64;
use interop_file::path::{self, ComponentPath};
use interop_file::spool::{PublishReceipt, SpoolStore, WriteHandle};
use std::path::Path;

use crate::payload::{ChunkSink, PacketType, PayloadAssembler, PayloadHeader, PayloadTransferFrame};

/// 对端在 introduction 里声明的一个文件（字段来源：F-27 的 `FileMetadata`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileOffer {
    pub name: String,
    pub size: u64,
    pub parent: Option<Vec<String>>,
    /// 对端声明的 payload id（`FileMetadata.payload_id`，F-27）——payloadID↔entryID 绑定的依据
    pub payload_id: i64,
    /// 对端声明的 MIME——**只作提示**，不参与决策（FILE-02）
    pub mime_hint: Option<String>,
}

impl FileOffer {
    /// 默认 payload id = 1（单文件场景）；多文件用 [`Self::with_payload_id`] 显式给出。
    pub fn new(name: &str, size: u64, parent: Option<Vec<String>>) -> Self {
        Self {
            name: name.to_string(),
            size,
            parent,
            payload_id: 1,
            mime_hint: None,
        }
    }

    pub fn with_payload_id(mut self, payload_id: i64) -> Self {
        self.payload_id = payload_id;
        self
    }

    pub fn with_mime_hint(mut self, hint: Option<String>) -> Self {
        self.mime_hint = hint;
        self
    }
}

/// 用户裁决（host UI 的输入）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiveDecision {
    Accept,
    Reject,
}

/// 回给对端的响应状态（`ConnectionResponseFrame.Status` 的语义子集：ACCEPT=1/REJECT=2/NOT_ENOUGH_SPACE=3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseStatus {
    Accept,
    Reject,
    NotEnoughSpace,
}

impl ResponseStatus {
    pub fn as_wire_code(self) -> i32 {
        match self {
            Self::Accept => 1,
            Self::Reject => 2,
            Self::NotEnoughSpace => 3,
        }
    }

    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::Reject => "reject",
            Self::NotEnoughSpace => "not-enough-space",
        }
    }
}

/// 已建立条目的对外视图（不含路径）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryView {
    pub entry_id: String,
    pub display_name: String,
    pub declared_size: u64,
    pub payload_id: i64,
    pub written: u64,
    pub complete: bool,
}

/// 发布回执视图（本地持久化 ≠ 远端确认，FILE-06）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishView {
    pub entry_id: String,
    pub payload_id: i64,
    pub relative_path: String,
    pub bytes: u64,
    pub sha256_hex: String,
}

struct PendingEntry {
    entry: OfferEntry,
    payload_id: i64,
    path: ComponentPath,
    handle: WriteHandle,
    assembler: PayloadAssembler,
    cancelled: bool,
}

/// 把 chunk 直接写进 spool 的 sink（顺序写由 assembler 保证）。
struct SpoolSink<'a> {
    spool: &'a mut SpoolStore,
    handle: WriteHandle,
}

impl ChunkSink for SpoolSink<'_> {
    fn write_chunk(&mut self, offset: u64, body: &[u8]) -> Result<(), Error> {
        self.spool.write(self.handle, offset, body)
    }
}

/// 接收会话：一条连接上的 payload 层状态机。
pub struct ReceiveSession {
    spool: SpoolStore,
    budget: u64,
    entries: Vec<PendingEntry>,
    published: Vec<PublishView>,
    next_entry_seq: u64,
}

impl std::fmt::Debug for ReceiveSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ReceiveSession(entries={}, published={})",
            self.entries.len(),
            self.published.len()
        )
    }
}

impl ReceiveSession {
    /// 打开会话：spool root 由 host 决定；`budget` 是本会话可写字节上限（FILE-04）。
    pub fn open(root: &Path, budget: u64) -> Result<Self, Error> {
        Ok(Self {
            spool: SpoolStore::open(root)?,
            budget,
            entries: Vec::new(),
            published: Vec::new(),
            next_entry_seq: 1,
        })
    }

    /// 处理 introduction：形状校验（FILE-05）→ 预算检查（FILE-04）→ 用户裁决 → 建立条目。
    ///
    /// 校验顺序刻意如此：**先全部校验，再产生任何副作用**——被拒的报价不留下临时文件。
    pub fn on_introduction(
        &mut self,
        offers: &[FileOffer],
        decision: ReceiveDecision,
    ) -> Result<ResponseStatus, Error> {
        if offers.is_empty() {
            return Err(Error::new(ErrorCode::InvalidFrame, "introduction 不含任何文件")
                .with_phase("offered"));
        }
        // 1) 形状：文件名/目录穿越、保留名、分隔符（T21-04）
        let mut paths: Vec<ComponentPath> = Vec::with_capacity(offers.len());
        for o in offers {
            paths.push(path::sanitize(o.parent.as_deref(), &o.name)?);
        }
        // payload id 必须唯一（同一条会话里两个条目共用一个 id 会让绑定失去意义）
        for (i, a) in offers.iter().enumerate() {
            if offers[i + 1..].iter().any(|b| b.payload_id == a.payload_id) {
                return Err(Error::new(
                    ErrorCode::InvalidFrame,
                    format!("payload id {} 在 introduction 里出现多次（拒绝）", a.payload_id),
                )
                .with_phase("offered"));
            }
        }
        // 2) 预算：声明总量超过会话预算 → NOT_ENOUGH_SPACE（不建条目、不落盘）
        let total: u64 = offers.iter().map(|o| o.size).sum();
        if total > self.budget {
            return Ok(ResponseStatus::NotEnoughSpace);
        }
        // 3) 用户裁决
        if decision == ReceiveDecision::Reject {
            return Ok(ResponseStatus::Reject);
        }
        // 4) 建立条目（每个文件一个条目 + 一个 payload id 绑定）
        for (idx, o) in offers.iter().enumerate() {
            let entry_id = format!("ofr_qs_{}_{}", self.next_entry_seq, idx);
            let payload_id = o.payload_id;
            self.next_entry_seq += 1;
            let entry = OfferEntry {
                entry_id: interop_contract::ids::EntryId::try_from(entry_id.clone())
                    .map_err(|e| Error::new(ErrorCode::InvalidFrame, e))?,
                display_name: o.name.clone(),
                relative_components: o.parent.clone(),
                media_type_hint: o.mime_hint.clone(),
                declared_size: U64(o.size),
                // 绑定：wire payload id 以字符串记在报价条目上（T21-01 用它做一一对应校验）
                wire_payload_id: payload_id.to_string(),
            };
            entry.validate()?;
            let handle = self.spool.begin(&entry, self.budget)?;
            let assembler = PayloadAssembler::new(PayloadHeader {
                id: payload_id,
                kind: crate::payload::PayloadKind::File,
                total_size: o.size,
                file_name: Some(o.name.clone()),
                parent_folder: o.parent.as_ref().map(|p| p.join("/")),
            })?;
            self.entries.push(PendingEntry {
                entry,
                payload_id,
                path: paths[idx].clone(),
                handle,
                assembler,
                cancelled: false,
            });
        }
        Ok(ResponseStatus::Accept)
    }

    pub fn entries(&self) -> Vec<EntryView> {
        self.entries
            .iter()
            .map(|e| EntryView {
                entry_id: e.entry.entry_id.as_ref().to_string(),
                display_name: e.entry.display_name.clone(),
                declared_size: e.entry.declared_size.0,
                payload_id: e.payload_id,
                written: e.assembler.written(),
                complete: e.assembler.is_complete(),
            })
            .collect()
    }

    /// 条目是否已建立（有写句柄）。测试/诊断用；句柄所有权仍在本会话。
    pub fn has_pending_entry(&self, entry_id: &str) -> bool {
        self.entries
            .iter()
            .any(|e| e.entry.entry_id.as_ref() == entry_id)
    }

    /// 条目当前已写字节（未知条目 → None）。
    pub fn entry_written(&self, entry_id: &str) -> Option<u64> {
        self.entries
            .iter()
            .find(|e| e.entry.entry_id.as_ref() == entry_id)
            .map(|e| e.assembler.written())
    }

    pub fn payload_id_of(&self, entry_id: &str) -> Option<i64> {
        self.entries
            .iter()
            .find(|e| e.entry.entry_id.as_ref() == entry_id)
            .map(|e| e.payload_id)
    }

    /// 处理一条 payload 帧。
    pub fn on_payload_frame(&mut self, frame: &PayloadTransferFrame) -> Result<(), Error> {
        match frame.packet_type {
            PacketType::Data => self.on_data_frame(frame),
            // 控制帧：取消/错误一律清理（不得留下最终文件或临时文件）
            PacketType::Control => {
                self.cancel_all()?;
                Ok(())
            }
            PacketType::PayloadAck => Err(Error::new(
                ErrorCode::UnsupportedFeature,
                "PAYLOAD_ACK 未实现（本 task 不做逐块 ACK）",
            )
            .with_phase("transferring")),
        }
    }

    fn on_data_frame(&mut self, frame: &PayloadTransferFrame) -> Result<(), Error> {
        let header = frame
            .header
            .as_ref()
            .ok_or_else(|| invalid("数据帧缺 payload_header"))?;
        let Some(idx) = self
            .entries
            .iter()
            .position(|e| e.payload_id == header.id && !e.cancelled)
        else {
            // T21-01：未知/混入的 payload id 一律拒绝——绝不"顺手"写进另一个条目
            return Err(Error::new(
                ErrorCode::InvalidFrame,
                format!(
                    "payload id {} 不属于本会话任何条目（拒绝混入；T21-01）",
                    header.id
                ),
            )
            .with_phase("transferring"));
        };
        // 逐字段借用（无 unsafe）：assembler 与 spool 属于同一结构体的不同字段
        let (entry, spool) = (&mut self.entries[idx], &mut self.spool);
        let mut sink = SpoolSink {
            spool,
            handle: entry.handle,
        };
        entry.assembler.accept(frame, &mut sink)
    }

    /// 条目完成后的原子发布（hash 校验 + 同 fs rename）。未完成 → 拒绝。
    pub fn finish_entry(&mut self, entry_id: &str) -> Result<PublishView, Error> {
        let Some(idx) = self
            .entries
            .iter()
            .position(|e| e.entry.entry_id.as_ref() == entry_id)
        else {
            return Err(Error::new(ErrorCode::DestinationUnavailable, "未知条目")
                .with_phase("committing"));
        };
        if !self.entries[idx].assembler.is_complete() {
            return Err(Error::new(
                ErrorCode::InvalidFrame,
                "条目未收满（缺 LAST_CHUNK）：不得发布",
            )
            .with_phase("committing"));
        }
        let entry = self.entries.remove(idx);
        let receipt: PublishReceipt = self.spool.commit(entry.handle, &entry.path, None)?;
        let view = PublishView {
            entry_id: entry.entry.entry_id.as_ref().to_string(),
            payload_id: entry.payload_id,
            relative_path: receipt.relative.join("/"),
            bytes: receipt.size,
            sha256_hex: receipt.sha256_hex,
        };
        self.published.push(view.clone());
        Ok(view)
    }

    /// 取消全部在途条目：临时文件立即删除（T21-02 的清理语义）。
    pub fn cancel_all(&mut self) -> Result<(), Error> {
        let entries = std::mem::take(&mut self.entries);
        for e in entries {
            if !e.assembler.is_complete() {
                self.spool.abort(e.handle)?;
            }
        }
        Ok(())
    }

    pub fn published(&self) -> &[PublishView] {
        &self.published
    }

    pub fn published_count(&self) -> usize {
        self.published.len()
    }

    /// 供上层构造响应帧使用的状态（语义与 `ConnectionResponseFrame.Status` 一致）。
    pub fn accept_status() -> ResponseStatus {
        ResponseStatus::Accept
    }
}

fn invalid(why: impl Into<String>) -> Error {
    Error::new(ErrorCode::InvalidFrame, why.into()).with_phase("transferring")
}
