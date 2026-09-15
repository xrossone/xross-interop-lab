//! 路由（plans/01 T11）：purpose/方向、能力与格式、安全、平台状态四项检查。
//!
//! 路由只接受 `EndpointId`（**不接受展示分组**）——展示聚合不产生任何路由/授权效果。
//! 决策全部 fail-closed：不支持的 purpose/profile 返回 `unsupported-feature`，
//! 用户拒绝的设备`auth-denied`（**拒绝而非降级**，与主仓 `svc-devices::route.rs` 同立场），
//! 平台不可用返回 `platform-unavailable`，没有任何候选返回 `destination-unavailable`。

use interop_contract::capability::Role;
use interop_contract::error::{Error, ErrorCode};
use interop_contract::ids::EndpointId;
use interop_contract::media::{MediaForm, SessionIntent};

use crate::endpoints::{AddressCandidate, EndpointRegistry};

/// 路由目的。方向不静默互换（CORE-05）：发送与接收要求对端声明不同 role。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutePurpose {
    /// 我们发送文件给对端 → 对端需声明接收能力
    FileSend,
    /// 我们从对端接收文件 → 对端需声明发送能力
    FileReceive,
    Control,
    /// 媒体会话；`SessionIntent` 区分镜像/音频/URL（URL cast 不能冒充完整镜像）
    Media(SessionIntent),
}

impl RoutePurpose {
    pub fn required_role(&self) -> Role {
        match self {
            Self::FileSend => Role::Receive,
            Self::FileReceive => Role::Send,
            Self::Control => Role::Control,
            Self::Media(_) => Role::Renderer,
        }
    }

    /// 该 purpose 可接受的媒体形态；`None` = 不限（文件/控制）。
    pub fn required_forms(&self) -> Option<&'static [MediaForm]> {
        match self {
            Self::FileSend | Self::FileReceive | Self::Control => None,
            Self::Media(SessionIntent::Screen) | Self::Media(SessionIntent::Mirror) => {
                Some(&[MediaForm::NativePresentation, MediaForm::EncodedStream])
            }
            Self::Media(SessionIntent::Audio) => {
                Some(&[MediaForm::PcmStream, MediaForm::EncodedStream])
            }
            Self::Media(SessionIntent::MediaUrl) => Some(&[MediaForm::MediaResource]),
        }
    }
}

/// 调用方对传输安全的要求。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityRequirement {
    /// 允许明文，但仍受用户策略约束（`RoutePolicy::allow_plaintext_fallback`）
    AllowPlaintext,
    /// 必须安全通道（明文候选一律不满足）
    RequireSecure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteRequest {
    pub endpoint_id: EndpointId,
    pub profile_id: String,
    pub purpose: RoutePurpose,
    pub min_security: SecurityRequirement,
}

/// 用户/平台策略（standalone 与集成模式各自注入）。
/// `Default` 是安全默认：**不放行明文回落**（显式设置了字段类型默认值）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RoutePolicy {
    /// 用户是否为"明文回落到明文"放行；默认拒绝（安全默认）。
    pub allow_plaintext_fallback: bool,
    /// 用户已拒绝的设备：拒绝而非降级。
    pub denied_endpoints: Vec<EndpointId>,
    /// 平台能力探测判为不可用的 profile（T12 的 doctor 结果接这里）。
    pub platform_blocked_profiles: Vec<String>,
}

/// 选定的路由：绑定具体 endpoint 记录与地址候选。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub endpoint_id: EndpointId,
    /// = `EndpointRecord::subject()`；授权/审计以此为准，而不是地址或名字
    pub subject: String,
    pub profile_id: String,
    pub address: AddressCandidate,
    pub secure: bool,
    pub purpose: RoutePurpose,
}

/// 计划一条路由。检查顺序固定（拒绝先于能力、能力先于地址、安全最后过滤）。
pub fn plan_route(
    registry: &EndpointRegistry,
    policy: &RoutePolicy,
    request: &RouteRequest,
    now_ms: u64,
) -> Result<Route, Error> {
    let record = registry.get(&request.endpoint_id).ok_or_else(|| {
        Error::new(
            ErrorCode::DestinationUnavailable,
            format!("未知端点 {}", request.endpoint_id),
        )
        .with_phase("discovered")
    })?;

    if policy.denied_endpoints.contains(&request.endpoint_id) {
        return Err(Error::new(
            ErrorCode::AuthDenied,
            format!("用户已拒绝端点 {}：拒绝而非降级", request.endpoint_id),
        )
        .with_phase("discovered"));
    }

    if policy
        .platform_blocked_profiles
        .iter()
        .any(|p| p == &request.profile_id)
    {
        return Err(Error::new(
            ErrorCode::PlatformUnavailable,
            format!(
                "profile {} 在当前平台的 probe 结果为不可用",
                request.profile_id
            ),
        )
        .with_profile(request.profile_id.clone())
        .with_phase("discovered"));
    }

    let role = request.purpose.required_role();
    if !record.supports(&request.profile_id, role) {
        return Err(Error::new(
            ErrorCode::UnsupportedFeature,
            format!(
                "端点未声明 {}/{:?} 能力（purpose {:?}）",
                request.profile_id, role, request.purpose
            ),
        )
        .with_profile(request.profile_id.clone())
        .with_phase("negotiating"));
    }

    if let Some(required) = request.purpose.required_forms() {
        let declared = record.media_forms(&request.profile_id, role);
        if !required.iter().any(|f| declared.contains(f)) {
            let want = required
                .iter()
                .map(|f| f.as_wire())
                .collect::<Vec<_>>()
                .join("/");
            return Err(Error::new(
                ErrorCode::UnsupportedFeature,
                format!(
                    "端点声明的媒体形态不满足 {:?}：需要 {want}，实际声明 [{}]",
                    request.purpose,
                    declared
                        .iter()
                        .map(|f| f.as_wire())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )
            .with_profile(request.profile_id.clone())
            .with_phase("negotiating"));
        }
    }

    let live = record.sendable_addresses(now_ms);
    if live.is_empty() {
        return Err(Error::new(
            ErrorCode::DestinationUnavailable,
            "没有仍有效的地址候选（接口断开或观察过期）",
        )
        .with_phase("discovered"));
    }

    let plaintext_forbidden =
        request.min_security == SecurityRequirement::RequireSecure || !policy.allow_plaintext_fallback;
    let mut candidates: Vec<&AddressCandidate> = live
        .into_iter()
        .filter(|a| !plaintext_forbidden || a.secure)
        .collect();
    if candidates.is_empty() {
        let why = if !policy.allow_plaintext_fallback {
            "明文 fallback 被用户策略禁止（allow_plaintext_fallback=false）"
        } else {
            "请求要求安全通道，但仅有明文候选"
        };
        return Err(Error::new(ErrorCode::AuthDenied, why)
            .with_profile(request.profile_id.clone())
            .with_phase("discovered"));
    }
    // 稳定排序：安全候选优先，其次接口/host/port 字典序。
    candidates.sort_by(|a, b| {
        (!a.secure, a.interface.as_str(), a.host.as_str(), a.port).cmp(&(
            !b.secure,
            b.interface.as_str(),
            b.host.as_str(),
            b.port,
        ))
    });
    let address = (*candidates[0]).clone();
    Ok(Route {
        endpoint_id: record.endpoint_id.clone(),
        subject: record.subject(),
        profile_id: request.profile_id.clone(),
        secure: address.secure,
        address,
        purpose: request.purpose,
    })
}
