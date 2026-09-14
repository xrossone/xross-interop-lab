//! 大整数 wire 编码：十进制字符串（docs/05 §1），避免 JS 53bit 精度丢失。

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

/// `u64` 的 wire 形式：JSON 十进制字符串。反序列化只接受字符串（严格，不收数字形式）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct U64(pub u64);

impl Serialize for U64 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for U64 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl de::Visitor<'_> for V {
            type Value = U64;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("十进制字符串编码的 u64")
            }
            fn visit_str<E: de::Error>(self, s: &str) -> Result<U64, E> {
                s.parse::<u64>().map(U64).map_err(de::Error::custom)
            }
        }
        deserializer.deserialize_str(V)
    }
}
