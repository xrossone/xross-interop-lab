//! 发现（plans/03 T32；字段见 `specs-reviewed/m01` 字段级事实表）。
//!
//! 已固化的事实：广播 `_airplay._tcp` + `_raop._tcp` 两种 Bonjour 服务；TXT 特性位决定功能协商。
//! **未固化**：AirPlay `features` TXT 位串的**真实位值**（字段表只固化了"位决定协商"这一事实）。
//! 因此本模块不编造位串：`txt_pairs()` 只给出协议无关的通用键，位串编码明确标待固化。

use crate::capability::{advertised_features, AdvertisedFeature, ImplementationInventory};
use interop_contract::error::{Error, ErrorCode};
use std::collections::BTreeSet;

/// 两种服务类型（字段表：同时广播）。
pub fn mdns_service_types() -> Vec<&'static str> {
    vec!["_airplay._tcp", "_raop._tcp"]
}

/// features 位串的状态（诚实标注）。
pub fn feature_string_status() -> &'static str {
    "待固化：AirPlay TXT features 位值的真实映射未固化（T31 字段表），不编造"
}

/// 一条接收端广告（语义层；位串编码见 [`feature_string_status`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceAdvertisement {
    pub port: u16,
    pub device_name: String,
    pub features: BTreeSet<AdvertisedFeature>,
}

impl ServiceAdvertisement {
    /// 构造广告：能力集合来自实现清单（只广告已实现项）。
    pub fn for_receiver(inv: &ImplementationInventory, port: u16, device_name: &str) -> Self {
        Self {
            port,
            device_name: device_name.to_string(),
            features: advertised_features(inv),
        }
    }

    /// 可诚实广播的 TXT 键值（DNS-SD 通用键）。
    /// AirPlay 专属的 `features` 位串**不在此列**——位值待固化，编造会让对端误判我们的能力。
    pub fn txt_pairs(&self) -> Vec<(String, String)> {
        vec![("txtvers".to_string(), "1".to_string())]
    }
}

/// TXT RDATA 编码（RFC 1035 §3.3.14：每条为 length-prefixed string）——协议无关的通用编码。
pub fn encode_txt_rdata(pairs: &[(String, String)]) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new();
    for (k, v) in pairs {
        let entry = format!("{k}={v}");
        if entry.len() > 255 {
            return Err(Error::new(
                ErrorCode::InvalidFrame,
                format!("单条 TXT 串超过 255 字节：{k}"),
            ));
        }
        out.push(entry.len() as u8);
        out.extend_from_slice(entry.as_bytes());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn txt_rdata_is_length_prefixed() {
        let pairs = vec![("txtvers".to_string(), "1".to_string())];
        let bytes = encode_txt_rdata(&pairs).expect("encode");
        assert_eq!(bytes[0], 9, "\"txtvers=1\" 长 9");
        assert_eq!(&bytes[1..], b"txtvers=1");
    }

    #[test]
    fn txt_rdata_rejects_overlong_entry() {
        let pairs = vec![("k".to_string(), "v".repeat(300))];
        assert_eq!(
            encode_txt_rdata(&pairs).expect_err("单条超 255").code,
            ErrorCode::InvalidFrame
        );
    }
}
