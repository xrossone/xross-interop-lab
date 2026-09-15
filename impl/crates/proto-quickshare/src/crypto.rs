//! 密码原语（plans/02-files.md §T20 实现决策：用成熟实现，不自写曲线/AES/HMAC 内部）。
//!
//! 用法来源（`specs-reviewed/f02-quickshare-lan.md`）：
//! - F-13：握手 cipher `P256_SHA512`（P-256 ECDH + SHA-512 commitment）；
//! - F-14：`DHS = SHA-256(ECDH 共享秘密)`（R18 `KeyAgreementSha256`、R15 对 x 坐标做 SHA-256）；
//! - F-15：**冲突行**——规范文本说 HKDF 用握手 cipher 的哈希（SHA-512），实现是 HKDF-SHA256。
//!   默认采用实现侧（互通事实），规范变体保留为 [`auth_string_spec_variant`] 供冲突固化与真机裁决；
//! - F-16：`AUTH = HKDF(ikm=DHS, salt="UKEY2 v1 auth", info=M1‖M2)`、`NEXT = HKDF(… "UKEY2 v1 next" …)`；
//! - F-17：4 位确认码（R15 实现的兼容启发式）。
//!
//! P-256 与 SHA/HKDF 都用成熟 crate（`p256`、`sha2`、`hkdf`）；本模块只做组合与长度校验。

use crate::wire::{encode_generic_public_key, GenericPublicKey, PUBLIC_KEY_TYPE_EC_P256};
use hkdf::Hkdf;
use interop_contract::error::{Error, ErrorCode};
use p256::elliptic_curve::sec1::{FromEncodedPoint, ToEncodedPoint};
use p256::elliptic_curve::generic_array::GenericArray;
use p256::{EncodedPoint, PublicKey, SecretKey};
use sha2::{Digest, Sha256, Sha512};

/// 当前采用的密钥派生哈希（F-15 裁决：实现侧）。
pub const KEY_SCHEDULE: &str = "HKDF-SHA256";
/// UKEY2 认证串的 HKDF salt（F-16）。
pub const SALT_AUTH: &[u8] = b"UKEY2 v1 auth";
/// UKEY2 next secret 的 HKDF salt（F-16）。
pub const SALT_NEXT: &[u8] = b"UKEY2 v1 next";
/// 规范要求 `random` 恰好 32 字节（F-08/F-09）。
pub const NONCE_BYTES: usize = 32;
/// P-256 坐标字节数。
const P256_COORD_BYTES: usize = 32;

fn invalid(why: impl Into<String>) -> Error {
    Error::new(ErrorCode::InvalidFrame, why.into()).with_phase("authorizing")
}

fn crypto_unavailable(why: impl Into<String>) -> Error {
    Error::new(ErrorCode::UnsupportedFeature, why.into()).with_phase("authorizing")
}

/// 32 字节系统熵（生产路径）；测试可用 [`DhKeyPair::from_scalar_bytes`] 注入确定性标量。
pub fn random_nonce() -> Result<[u8; NONCE_BYTES], Error> {
    let mut buf = [0u8; NONCE_BYTES];
    getrandom::getrandom(&mut buf)
        .map_err(|e| crypto_unavailable(format!("系统熵不可用：{e}")))?;
    Ok(buf)
}

/// 临时 P-256 密钥对（私钥不进 Debug/日志）。
pub struct DhKeyPair {
    secret: SecretKey,
    public: GenericPublicKey,
}

impl std::fmt::Debug for DhKeyPair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DhKeyPair(<secret redacted>, public={} bytes)", self.public.x.len() + self.public.y.len())
    }
}

impl DhKeyPair {
    /// 生产路径：用系统熵生成。
    pub fn generate() -> Result<Self, Error> {
        for _ in 0..8 {
            let bytes = random_nonce()?;
            if let Ok(pair) = Self::from_scalar_bytes(&bytes) {
                return Ok(pair);
            }
        }
        Err(crypto_unavailable("无法生成合法 P-256 标量（熵源异常）"))
    }

    /// 测试/语料注入：确定性标量（**不得**用于生产路径）。
    pub fn from_scalar_bytes(scalar: &[u8]) -> Result<Self, Error> {
        let secret = SecretKey::from_slice(scalar)
            .map_err(|_| invalid("标量不是合法的 P-256 私钥"))?;
        let public = encode_public_key(&secret)?;
        Ok(Self { secret, public })
    }

    pub fn public(&self) -> &GenericPublicKey {
        &self.public
    }

    /// `DHS = SHA-256(x)`（F-14）。
    pub fn dh(&self, peer: &GenericPublicKey) -> Result<[u8; 32], Error> {
        let point = decode_public_key(peer)?;
        let shared = p256::ecdh::diffie_hellman(self.secret.to_nonzero_scalar(), point.as_affine());
        Ok(sha256(shared.raw_secret_bytes()))
    }
}

fn encode_public_key(secret: &SecretKey) -> Result<GenericPublicKey, Error> {
    let point = secret.public_key().to_encoded_point(false);
    let x = point
        .x()
        .ok_or_else(|| crypto_unavailable("公钥缺少 x 坐标"))?
        .to_vec();
    let y = point
        .y()
        .ok_or_else(|| crypto_unavailable("公钥缺少 y 坐标"))?
        .to_vec();
    Ok(GenericPublicKey {
        key_type: PUBLIC_KEY_TYPE_EC_P256,
        x,
        y,
    })
}

/// 解析对端公钥，并**验证它是曲线上的合法点**（`securemessage.proto` 的显式要求）。
/// 坐标长度容错与 R15 一致：>32 取末 32 字节，<32 左侧补零。
fn decode_public_key(key: &GenericPublicKey) -> Result<PublicKey, Error> {
    if key.key_type != PUBLIC_KEY_TYPE_EC_P256 {
        return Err(Error::new(
            ErrorCode::UnsupportedFeature,
            format!("只支持 P-256 公钥（收到 type={}）", key.key_type),
        )
        .with_phase("authorizing"));
    }
    let x = normalize_coord(&key.x)?;
    let y = normalize_coord(&key.y)?;
    let point = EncodedPoint::from_affine_coordinates(&x, &y, false);
    Option::<PublicKey>::from(PublicKey::from_encoded_point(&point)).ok_or_else(|| {
        invalid("对端公钥不是 NIST P-256 上的合法点（拒绝，不做坐标猜测）")
    })
}

fn normalize_coord(bytes: &[u8]) -> Result<GenericArray<u8, p256::elliptic_curve::consts::U32>, Error> {
    let mut out = [0u8; P256_COORD_BYTES];
    match bytes.len() {
        0 => return Err(invalid("坐标长度为 0")),
        n if n > P256_COORD_BYTES => out.copy_from_slice(&bytes[n - P256_COORD_BYTES..]),
        n => out[P256_COORD_BYTES - n..].copy_from_slice(bytes),
    }
    Ok(GenericArray::clone_from_slice(&out))
}

pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(data);
    h.finalize().into()
}

/// commitment 的哈希：SHA-512（F-11）。
pub fn sha512(data: &[u8]) -> [u8; 64] {
    let mut h = Sha512::new();
    h.update(data);
    h.finalize().into()
}

/// `AUTH_STRING`：HKDF-SHA256(ikm=DHS, salt="UKEY2 v1 auth", info=M1‖M2)（F-15/F-16）。
pub fn auth_string(dhs: &[u8], transcript: &[u8], len: usize) -> Vec<u8> {
    hkdf_expand_sha256(dhs, SALT_AUTH, transcript, len)
}

/// `NEXT_SECRET`：同上，salt 换成 `"UKEY2 v1 next"`（F-16）。
pub fn next_protocol_secret(dhs: &[u8], transcript: &[u8], len: usize) -> Vec<u8> {
    hkdf_expand_sha256(dhs, SALT_NEXT, transcript, len)
}

/// **规范文本变体**（HKDF-SHA512）：只用于把 F-15 的规范/实现冲突固化成断言；
/// 采纳前需要具名真机实验（与 stock Android 握手比对 4 位码）。
pub fn auth_string_spec_variant(dhs: &[u8], transcript: &[u8], len: usize) -> Vec<u8> {
    let hk = Hkdf::<Sha512>::new(Some(SALT_AUTH), dhs);
    let mut okm = vec![0u8; len];
    hk.expand(transcript, &mut okm)
        .expect("len 远小于 HKDF 上限");
    okm
}

fn hkdf_expand_sha256(ikm: &[u8], salt: &[u8], info: &[u8], len: usize) -> Vec<u8> {
    let hk = Hkdf::<Sha256>::new(Some(salt), ikm);
    let mut okm = vec![0u8; len];
    hk.expand(info, &mut okm)
        .expect("len 远小于 HKDF 上限（255·HashLen）");
    okm
}

/// 4 位确认码（F-17；R15 实现的兼容启发式，真机一致性待 P-F02-2/3 裁决）。
pub fn pin_code(auth: &[u8]) -> String {
    const MOD: i64 = 9973;
    let mut hash: i64 = 0;
    let mut multiplier: i64 = 1;
    for byte in auth {
        let signed = *byte as i8 as i64;
        hash = (hash + signed * multiplier) % MOD;
        multiplier = (multiplier * 31) % MOD;
    }
    format!("{:04}", hash.abs())
}

/// 公钥编码（ServerInit/ClientFinished 的 `public_key` 字段）。
pub fn public_key_bytes(key: &GenericPublicKey) -> Vec<u8> {
    encode_generic_public_key(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dh_is_symmetric_and_hashed() {
        let a = DhKeyPair::from_scalar_bytes(&[0x11u8; 32]).expect("a");
        let b = DhKeyPair::from_scalar_bytes(&[0x22u8; 32]).expect("b");
        let ab = a.dh(b.public()).expect("ab");
        let ba = b.dh(a.public()).expect("ba");
        assert_eq!(ab, ba, "ECDH 必须对称");
        assert_eq!(ab.len(), 32, "DHS = SHA-256(x) 是 32 字节");
    }

    #[test]
    fn invalid_point_is_rejected() {
        let bad = GenericPublicKey {
            key_type: PUBLIC_KEY_TYPE_EC_P256,
            x: vec![0x01; 32],
            y: vec![0x02; 32],
        };
        let a = DhKeyPair::from_scalar_bytes(&[0x33u8; 32]).expect("a");
        assert_eq!(a.dh(&bad).expect_err("非法点").code, ErrorCode::InvalidFrame);
    }

    #[test]
    fn pin_code_is_four_digits_and_input_dependent() {
        let a = pin_code(&[1u8; 32]);
        let b = pin_code(&[2u8; 32]);
        assert_eq!(a.len(), 4);
        assert!(a.chars().all(|c| c.is_ascii_digit()));
        assert_ne!(a, b);
    }
}
