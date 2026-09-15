//! 能力广告门禁（plans/03 T32-03）：**只广播已实现的能力**。
//!
//! 广告照抄别人的位图 = 对端按广告发来我们解不了的东西（例如广告 HEVC 却没有 decoder）。
//! 因此广告集合由 [`ImplementationInventory`]（本仓实现清单）**推导**，并提供
//! [`assert_advertised_matches_implementation`] 作为构建/能力测试用的自洽检查。

use interop_contract::error::{Error, ErrorCode};
use std::collections::BTreeSet;

/// 本仓实现清单（随 T33 落地逐项变 true；`current()` 是唯一事实来源）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImplementationInventory {
    pub h264_decode: bool,
    pub hevc_decode: bool,
    pub alac_decode: bool,
    pub pcm_playback: bool,
    pub native_window: bool,
    pub transient_pairing: bool,
    pub pin_pairing: bool,
}

impl ImplementationInventory {
    /// 当前实现清单。**每一项变 true 前必须存在对应实现与测试**；
    /// T33（镜像音视频接收）之前全部为 false——因此广告为空，对端不会把我们当成可用接收器。
    pub fn current() -> Self {
        Self {
            h264_decode: false,
            hevc_decode: false,
            alac_decode: false,
            pcm_playback: false,
            native_window: false,
            transient_pairing: false,
            pin_pairing: false,
        }
    }
}

/// 可广告的能力（语义集合；与真实 AirPlay TXT 位值的映射**待固化**，见 [`crate::discovery`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AdvertisedFeature {
    VideoReceiverH264,
    VideoReceiverHevc,
    AudioReceiverAlac,
    AudioReceiverPcm,
    TransientPairing,
    PinPairing,
}

impl AdvertisedFeature {
    pub fn as_wire(&self) -> &'static str {
        match self {
            Self::VideoReceiverH264 => "video-receiver-h264",
            Self::VideoReceiverHevc => "video-receiver-hevc",
            Self::AudioReceiverAlac => "audio-receiver-alac",
            Self::AudioReceiverPcm => "audio-receiver-pcm",
            Self::TransientPairing => "transient-pairing",
            Self::PinPairing => "pin-pairing",
        }
    }
}

/// 由实现清单推导广告集合（唯一正确来源）。
pub fn advertised_features(inv: &ImplementationInventory) -> BTreeSet<AdvertisedFeature> {
    let mut set = BTreeSet::new();
    if inv.h264_decode {
        set.insert(AdvertisedFeature::VideoReceiverH264);
    }
    if inv.hevc_decode {
        set.insert(AdvertisedFeature::VideoReceiverHevc);
    }
    if inv.alac_decode {
        set.insert(AdvertisedFeature::AudioReceiverAlac);
    }
    if inv.pcm_playback {
        set.insert(AdvertisedFeature::AudioReceiverPcm);
    }
    if inv.transient_pairing {
        set.insert(AdvertisedFeature::TransientPairing);
    }
    if inv.pin_pairing {
        set.insert(AdvertisedFeature::PinPairing);
    }
    set
}

/// 单个能力是否有实现支撑。
pub fn implementation_supports(inv: &ImplementationInventory, feature: AdvertisedFeature) -> bool {
    match feature {
        AdvertisedFeature::VideoReceiverH264 => inv.h264_decode,
        AdvertisedFeature::VideoReceiverHevc => inv.hevc_decode,
        AdvertisedFeature::AudioReceiverAlac => inv.alac_decode,
        AdvertisedFeature::AudioReceiverPcm => inv.pcm_playback,
        AdvertisedFeature::TransientPairing => inv.transient_pairing,
        AdvertisedFeature::PinPairing => inv.pin_pairing,
    }
}

/// 广告 ⊂ 实现。广告里出现未实现能力 → 明确失败（构建/能力测试用）。
pub fn assert_advertised_matches_implementation(
    advertised: &BTreeSet<AdvertisedFeature>,
    inv: &ImplementationInventory,
) -> Result<(), Error> {
    let missing: Vec<&AdvertisedFeature> = advertised
        .iter()
        .filter(|f| !implementation_supports(inv, **f))
        .collect();
    if !missing.is_empty() {
        let names: Vec<&str> = missing.iter().map(|f| f.as_wire()).collect();
        return Err(Error::new(
            ErrorCode::UnsupportedFeature,
            format!(
                "广告包含未实现能力 {:?}：只广播已实现 feature（T32-03）",
                names
            ),
        )
        .with_phase("negotiating"));
    }
    Ok(())
}
