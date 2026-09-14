//! scoped token 存储/校验。token 由 host 提供的受控 store 生成与保管
//! （docs/05 §6：不放命令行/普通日志；本 crate 只做持有与匹配）。

/// 单条 scoped token。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopedToken {
    pub token: String,
    pub subject: String,
    pub scopes: Vec<String>,
}

/// token 集合。验证用非常量时间比较的近似替代（异或折叠 + 提前长度检查），
/// 同 UID 攻击者模型下的真正边界是文件权限/peer-UID，不在此处。
#[derive(Debug, Default, Clone)]
pub struct TokenStore {
    tokens: Vec<ScopedToken>,
}

impl TokenStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册一条 token（生成由 host 层负责；见 apps/interopd）。
    pub fn issue_with(&mut self, token: &str, subject: &str, scopes: Vec<String>) {
        self.tokens.push(ScopedToken {
            token: token.to_string(),
            subject: subject.to_string(),
            scopes,
        });
    }

    /// 常量时间式匹配后返回授权。
    pub fn verify(&self, token: &str) -> Option<&ScopedToken> {
        self.tokens.iter().find(|t| ct_eq(t.token.as_bytes(), token.as_bytes()))
    }
}

/// 长度无关的逐字节异或折叠比较。
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}
