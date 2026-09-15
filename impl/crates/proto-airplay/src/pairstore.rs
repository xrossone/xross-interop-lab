//! 配对存储与配对端点策略（T34）：**登记层，不是认证层**。
//!
//! 字段来源见 `specs-reviewed/m01-airplay-legacy-mirror.md` 的「T34 增量」配对表：
//! UxPlay 的已配对注册表（`~/.uxplay.register`，每行 `pk,device_id,name`，只追加无删除）与
//! shairplay-rust 的 `PairingStore{get/put/remove/has_any_pairing/load_identity/save_identity}`
//! + `OneTimePairingRequired = statusFlags bit 9`。
//!
//! **核心诚实约束**（lab gate 机器检查）：本模块**不得**把"登记过"当成"认证过"。它只回答
//! "这个 device id 是否登记过"；签名校验属于 SRP/X25519/Ed25519 的实现，本仓没有 →
//! 认证结论由 `keying` seam 给（当前 `UnavailableKeying` → SETUP 503）。
//!
//! 与来源的**有意差异**（本仓策略，已在字段表标注）：来源的注册表只追加、无法删除；本仓提供
//! 显式遗忘与容量上限（拒绝而不是静默淘汰），并把状态序列化交给宿主（本层不做文件 I/O）。

use interop_contract::error::{Error, ErrorCode};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 设备 id 的上限（**本仓策略**）。
pub const MAX_DEVICE_ID_BYTES: usize = 128;
/// 显示名的上限（**本仓策略**）。
pub const MAX_DISPLAY_NAME_BYTES: usize = 64;
/// 登记条目上限（**本仓策略**；满了明确拒绝，不静默淘汰）。
pub const MAX_PAIRED_DEVICES: usize = 64;
/// 长期公钥长度（Ed25519 公钥 32 字节，来源：`lib/pairing.c:629` 的 `client_pk[32]`）。
pub const PUBLIC_KEY_BYTES: usize = 32;
/// `statusFlags` 的 bit 9：`OneTimePairingRequired`（来源：`types.rs:146-152`）。
pub const STATUS_FLAG_ONE_TIME_PAIRING_REQUIRED: u32 = 1 << 9;

/// 一条登记记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairedDevice {
    pub device_id: String,
    /// 控制器的 Ed25519 公钥（**由调用方提供**；本仓不生成、不派生任何密钥材料）。
    pub controller_public_key: [u8; PUBLIC_KEY_BYTES],
    pub display_name: Option<String>,
    pub added_at_ms: u64,
    pub last_seen_ms: u64,
}

/// 存储对某个设备的结论（**没有 `Authenticated` 变体**，这是刻意的）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairingVerdict {
    /// 从未登记过。
    Unknown,
    /// 登记过，但**签名未校验**——不得据此放行任何受保护操作。
    KnownButUnverified,
}

/// 配对端点。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairingEndpoint {
    /// `/pair-pin-start`：让用户看到 PIN。
    PairPinStart,
    /// `/pair-setup-pin`：三步 SRP 配对。
    PairSetupPin,
    /// `/pair-setup`：HomeKit 风格路线的裸 32 字节公钥交换。
    PairSetup,
    /// `/pair-verify`：已配对设备的重连校验。
    PairVerify,
    /// `/fp-setup`：FairPlay 设备认证——**永久 vendor-gated**。
    FpSetup,
}

/// 端点处理策略。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndpointPolicy {
    /// 本仓只做键名/形状校验，交换本身不实现。
    ShapeCheckOnly {
        reason: &'static str,
        /// 该端点涉及的字段名（来自字段表，用于形状校验与报告）。
        keys: &'static [&'static str],
    },
    /// 永久 vendor-gated：不实现、不解析载荷。
    VendorGated { reason: &'static str },
}

impl PairingEndpoint {
    /// 从 RTSP 路径解析端点；未知路径 → `None`（调用方按 not-found 处理）。
    pub fn from_path(path: &str) -> Option<Self> {
        match path {
            "/pair-pin-start" => Some(Self::PairPinStart),
            "/pair-setup-pin" => Some(Self::PairSetupPin),
            "/pair-setup" => Some(Self::PairSetup),
            "/pair-verify" => Some(Self::PairVerify),
            "/fp-setup" => Some(Self::FpSetup),
            _ => None,
        }
    }

    pub fn path(self) -> &'static str {
        match self {
            Self::PairPinStart => "/pair-pin-start",
            Self::PairSetupPin => "/pair-setup-pin",
            Self::PairSetup => "/pair-setup",
            Self::PairVerify => "/pair-verify",
            Self::FpSetup => "/fp-setup",
        }
    }

    pub fn policy(self) -> EndpointPolicy {
        match self {
            Self::PairPinStart => EndpointPolicy::ShapeCheckOnly {
                reason: "只回 PIN 显示时机；PIN 的生成与校验属配对握手，本仓不实现",
                keys: &[],
            },
            Self::PairSetupPin => EndpointPolicy::ShapeCheckOnly {
                reason: "三步 SRP-SHA1 + AES-GCM authTag（字段表 T34）；本仓不实现 SRP，交换一律拒绝",
                keys: &["method", "user", "pk", "salt", "proof", "epk", "authTag"],
            },
            Self::PairSetup => EndpointPolicy::ShapeCheckOnly {
                reason: "收发裸 32 字节 Ed25519 公钥；本仓不做密钥交换",
                keys: &["pk"],
            },
            Self::PairVerify => EndpointPolicy::ShapeCheckOnly {
                reason: "两步 X25519+Ed25519 签名校验；本仓不实现签名校验（这正是不能声称认证的原因）",
                keys: &["pk", "signature"],
            },
            Self::FpSetup => EndpointPolicy::VendorGated {
                reason: "FairPlay/设备认证材料永久 vendor-gated：不实现、不解析载荷、不绕过",
            },
        }
    }

    /// 该端点的**形状**校验：请求里出现的键必须都在字段表登记过。
    /// 返回 `Err(UnsupportedFeature)` 表示"形状对但本仓不实现交换"，调用方据此回明确错误。
    pub fn check_request(self, keys: &[&str]) -> Result<(), Error> {
        match self.policy() {
            EndpointPolicy::VendorGated { reason } => Err(Error::new(
                ErrorCode::VendorGated,
                format!("{}：{reason}", self.path()),
            )),
            EndpointPolicy::ShapeCheckOnly { reason, keys: known } => {
                for key in keys {
                    if !known.is_empty() && !known.contains(key) {
                        return Err(Error::new(
                            ErrorCode::InvalidFrame,
                            format!("{} 请求出现字段表未登记的键 {key}", self.path()),
                        ));
                    }
                }
                Err(Error::new(
                    ErrorCode::UnsupportedFeature,
                    format!("{} 形状合法但本仓不实现交换：{reason}", self.path()),
                ))
            }
        }
    }

    /// 全表（用于报告/演示：每条给出策略）。
    pub fn all() -> &'static [PairingEndpoint] {
        &[
            Self::PairPinStart,
            Self::PairSetupPin,
            Self::PairSetup,
            Self::PairVerify,
            Self::FpSetup,
        ]
    }
}

/// 配对存储：登记/查询/遗忘 + 配件身份种子保管。**无加密、无 I/O**。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PairStore {
    devices: BTreeMap<String, PairedDevice>,
    /// 配件自己的长期身份种子（Ed25519 seed，32 字节）。**由宿主提供与保管**；
    /// 参考实现同样只把它交给存储实现（`load_identity`/`save_identity`），本层不生成。
    identity_seed: Option<[u8; 32]>,
    #[serde(default)]
    max_entries: usize,
}

impl PairStore {
    pub fn new() -> Self {
        Self {
            devices: BTreeMap::new(),
            identity_seed: None,
            max_entries: MAX_PAIRED_DEVICES,
        }
    }

    pub fn with_capacity(max_entries: usize) -> Self {
        Self {
            max_entries,
            ..Self::new()
        }
    }

    /// 登记一台设备（同 id 覆盖，视为重新配对）。
    pub fn put(
        &mut self,
        device_id: &str,
        controller_public_key: [u8; PUBLIC_KEY_BYTES],
        display_name: Option<String>,
        now_ms: u64,
    ) -> Result<(), Error> {
        if device_id.is_empty() || device_id.len() > MAX_DEVICE_ID_BYTES {
            return Err(Error::new(
                ErrorCode::InvalidFrame,
                format!("device id 长度非法（本仓策略上限 {MAX_DEVICE_ID_BYTES}）"),
            ));
        }
        if let Some(name) = &display_name {
            if name.len() > MAX_DISPLAY_NAME_BYTES {
                return Err(Error::new(
                    ErrorCode::ResourceLimit,
                    format!("显示名超过本仓上限 {MAX_DISPLAY_NAME_BYTES}"),
                ));
            }
        }
        if !self.devices.contains_key(device_id) && self.devices.len() >= self.max_entries {
            return Err(Error::new(
                ErrorCode::ResourceLimit,
                format!(
                    "登记表已满（{} 条，本仓策略）：先遗忘旧条目，不静默淘汰",
                    self.max_entries
                ),
            ));
        }
        let added_at_ms = self
            .devices
            .get(device_id)
            .map(|d| d.added_at_ms)
            .unwrap_or(now_ms);
        self.devices.insert(
            device_id.to_string(),
            PairedDevice {
                device_id: device_id.to_string(),
                controller_public_key,
                display_name,
                added_at_ms,
                last_seen_ms: now_ms,
            },
        );
        Ok(())
    }

    pub fn get(&self, device_id: &str) -> Option<&PairedDevice> {
        self.devices.get(device_id)
    }

    pub fn remove(&mut self, device_id: &str) -> bool {
        self.devices.remove(device_id).is_some()
    }

    /// 是否至少登记过一台（驱动 `statusFlags` 的 bit 9）。
    pub fn has_any_pairing(&self) -> bool {
        !self.devices.is_empty()
    }

    pub fn len(&self) -> usize {
        self.devices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.devices.is_empty()
    }

    /// 记录一次见到该设备（只影响 `last_seen_ms`）。
    pub fn touch(&mut self, device_id: &str, now_ms: u64) -> bool {
        match self.devices.get_mut(device_id) {
            Some(entry) => {
                entry.last_seen_ms = now_ms;
                true
            }
            None => false,
        }
    }

    /// 对某个设备的结论：**只有 Unknown / KnownButUnverified**（见模块文档）。
    pub fn verdict(&self, device_id: &str) -> PairingVerdict {
        if self.devices.contains_key(device_id) {
            PairingVerdict::KnownButUnverified
        } else {
            PairingVerdict::Unknown
        }
    }

    /// `statusFlags`：首个成功配对之前置 bit 9（`OneTimePairingRequired`）。
    pub fn status_flags(&self) -> u32 {
        if self.has_any_pairing() {
            0
        } else {
            STATUS_FLAG_ONE_TIME_PAIRING_REQUIRED
        }
    }

    pub fn identity_seed(&self) -> Option<[u8; PUBLIC_KEY_BYTES]> {
        self.identity_seed
    }

    /// 由宿主提供身份种子（本层不生成、不派生）。
    pub fn set_identity_seed(&mut self, seed: [u8; PUBLIC_KEY_BYTES]) {
        self.identity_seed = Some(seed);
    }

    pub fn devices(&self) -> impl Iterator<Item = &PairedDevice> {
        self.devices.values()
    }

    /// 序列化（宿主决定落盘位置与时机；本层不做文件 I/O）。
    pub fn to_json(&self) -> Result<String, Error> {
        serde_json::to_string(self).map_err(|e| {
            Error::new(
                ErrorCode::InvalidFrame,
                format!("配对存储序列化失败：{e}"),
            )
        })
    }

    /// 反序列化；条目数超上限即拒绝（不静默截断）。
    pub fn from_json(text: &str) -> Result<Self, Error> {
        let store: Self = serde_json::from_str(text).map_err(|e| {
            Error::new(ErrorCode::InvalidFrame, format!("配对存储反序列化失败：{e}"))
        })?;
        if store.devices.len() > MAX_PAIRED_DEVICES {
            return Err(Error::new(
                ErrorCode::ResourceLimit,
                format!("配对存储含 {} 条，超过本仓上限 {MAX_PAIRED_DEVICES}", store.devices.len()),
            ));
        }
        Ok(store)
    }
}
