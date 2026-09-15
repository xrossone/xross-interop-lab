//! 加固的 XML 子集解析（plans/03 §T41；字段表 F-11/F-12）。
//!
//! UPnP 的设备/服务描述与控制消息都是 XML，而 XML 是攻击面（XXE、实体扩展、深嵌套、体积炸弹）。
//! 本模块**不追求完整 XML**：只解析设备描述与控制信封需要的那一小撮语法，并把不安全的部分
//! 直接拒绝：
//! - `<!DOCTYPE` / `<!ENTITY` / 任何 `<!…>`（含 CDATA）→ 拒绝（**没有 DTD 就没有 XXE**）；
//! - 未定义实体引用（`&foo;`）→ 拒绝；只允许 5 个标准实体；
//! - 深度 / 元素数 / 属性数 / 总字节数都有上限——**这些预算是本仓策略值**（P-M07-2），不是协议常量；
//! - 标签/属性名按 XML 名字字符集校验，不猜、不宽容修复。
//!
//! 只保留解析结果需要的结构（名称/属性/子节点/文本），不实现命名空间解析（属性里保留原始前缀）。

use interop_contract::error::{Error, ErrorCode};
use std::collections::BTreeMap;

/// 解析预算（**本仓策略**，不是协议常量）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XmlBudget {
    pub max_bytes: usize,
    pub max_depth: usize,
    pub max_elements: usize,
    pub max_attrs: usize,
}

impl Default for XmlBudget {
    fn default() -> Self {
        Self {
            max_bytes: 64 * 1024,
            max_depth: 32,
            max_elements: 4096,
            max_attrs: 32,
        }
    }
}

/// 解析出的一棵小树。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Element {
    pub name: String,
    pub attrs: BTreeMap<String, String>,
    pub children: Vec<Element>,
    /// 直接文本内容（子元素之外的文本，已做实体解码）
    pub text: String,
}

impl Element {
    /// 按名字找第一个子元素。
    pub fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.name == name)
    }

    /// 按名字找所有子元素。
    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }

    /// 去掉命名空间前缀后的本地名（`s:Body` → `Body`）。
    pub fn local_name(&self) -> &str {
        match self.name.rsplit_once(':') {
            Some((_, local)) => local,
            None => &self.name,
        }
    }
}

fn invalid(why: impl Into<String>) -> Error {
    Error::new(ErrorCode::InvalidFrame, why.into()).with_phase("discovered")
}

fn limit(why: impl Into<String>) -> Error {
    Error::new(ErrorCode::ResourceLimit, why.into()).with_phase("discovered")
}

fn is_name_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b == b':'
}

fn is_name_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b':' | b'.' | b'-')
}

/// 解析一个文档（必须恰好一个根元素）。
pub fn parse_document(bytes: &[u8], budget: XmlBudget) -> Result<Element, Error> {
    if bytes.len() > budget.max_bytes {
        return Err(limit(format!(
            "XML {} 字节超过策略上限 {}（XmlBudget，本仓策略值）",
            bytes.len(),
            budget.max_bytes
        )));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| invalid("XML 不是合法 UTF-8"))?
        .to_string();

    let mut p = Parser {
        bytes: text.as_bytes(),
        pos: 0,
        budget,
        elements: 0,
    };
    let root = p.parse()?;
    Ok(root)
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
    budget: XmlBudget,
    elements: usize,
}

impl<'a> Parser<'a> {
    fn parse(&mut self) -> Result<Element, Error> {
        self.skip_prolog()?;
        let root = self.parse_element(0)?;
        // 根之后只允许空白
        self.skip_ws();
        if self.pos != self.bytes.len() {
            return Err(invalid("根元素之后出现多余内容"));
        }
        Ok(root)
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ') | Some(b'\t') | Some(b'\r') | Some(b'\n')) {
            self.pos += 1;
        }
    }

    /// 跳过多余声明、注释；拒绝 DTD/实体/CDATA。
    fn skip_prolog(&mut self) -> Result<(), Error> {
        loop {
            self.skip_ws();
            if self.rest().starts_with(b"<?") {
                // XML 声明 / 处理指令：跳到 ?>
                let end = self.rest().windows(2).position(|w| w == b"?>");
                match end {
                    Some(i) => self.pos += i + 2,
                    None => return Err(invalid("未闭合的处理指令")),
                }
                continue;
            }
            if self.rest().starts_with(b"<!--") {
                let end = self.rest().windows(3).position(|w| w == b"-->");
                match end {
                    Some(i) => self.pos += i + 3,
                    None => return Err(invalid("未闭合的注释")),
                }
                continue;
            }
            break;
        }
        Ok(())
    }

    fn rest(&self) -> &'a [u8] {
        &self.bytes[self.pos.min(self.bytes.len())..]
    }

    fn expect(&mut self, b: u8) -> Result<(), Error> {
        if self.peek() == Some(b) {
            self.pos += 1;
            Ok(())
        } else {
            Err(invalid(format!("期望字节 {:?}", b as char)))
        }
    }

    fn name(&mut self) -> Result<String, Error> {
        let start = self.pos;
        match self.peek() {
            Some(b) if is_name_start(b) => self.pos += 1,
            _ => return Err(invalid("非法的名称起始字符")),
        }
        while matches!(self.peek(), Some(b) if is_name_char(b)) {
            self.pos += 1;
        }
        Ok(std::str::from_utf8(&self.bytes[start..self.pos])
            .map_err(|_| invalid("名称不是合法 UTF-8"))?
            .to_string())
    }

    fn parse_element(&mut self, depth: usize) -> Result<Element, Error> {
        if depth > self.budget.max_depth {
            return Err(limit(format!(
                "XML 嵌套深度超过策略上限 {}（XmlBudget，本仓策略值）",
                self.budget.max_depth
            )));
        }
        self.elements += 1;
        if self.elements > self.budget.max_elements {
            return Err(limit(format!(
                "XML 元素数超过策略上限 {}（XmlBudget，本仓策略值）",
                self.budget.max_elements
            )));
        }
        // 任何 <!…>（DOCTYPE/ENTITY/CDATA）在这里就被拒绝
        if self.rest().starts_with(b"<!") {
            return Err(invalid(
                "XML 含 <!…> 声明（DOCTYPE/ENTITY/CDATA）：一律拒绝（无 DTD 即无 XXE）",
            ));
        }
        self.expect(b'<')?;
        let name = self.name()?;
        let mut attrs = BTreeMap::new();
        loop {
            self.skip_ws();
            match self.peek() {
                Some(b'/') => {
                    self.pos += 1;
                    self.expect(b'>')?;
                    return Ok(Element {
                        name,
                        attrs,
                        children: Vec::new(),
                        text: String::new(),
                    });
                }
                Some(b'>') => {
                    self.pos += 1;
                    break;
                }
                Some(b) if is_name_start(b) => {
                    if attrs.len() >= self.budget.max_attrs {
                        return Err(limit(format!(
                            "元素属性数超过策略上限 {}（XmlBudget，本仓策略值）",
                            self.budget.max_attrs
                        )));
                    }
                    let aname = self.name()?;
                    self.skip_ws();
                    self.expect(b'=')?;
                    self.skip_ws();
                    let quote = self.peek().ok_or_else(|| invalid("属性缺少引号"))?;
                    if quote != b'"' && quote != b'\'' {
                        return Err(invalid("属性值必须加引号"));
                    }
                    self.pos += 1;
                    let vstart = self.pos;
                    while matches!(self.peek(), Some(b) if b != quote) {
                        self.pos += 1;
                    }
                    let raw = std::str::from_utf8(&self.bytes[vstart..self.pos])
                        .map_err(|_| invalid("属性值不是合法 UTF-8"))?
                        .to_string();
                    self.expect(quote)?;
                    attrs.insert(aname, decode_entities(&raw)?);
                }
                _ => return Err(invalid("元素内部出现非法字符")),
            }
        }

        // 内容：子元素与文本
        let mut children = Vec::new();
        let mut text = String::new();
        loop {
            if self.rest().starts_with(b"<!--") {
                let end = self.rest().windows(3).position(|w| w == b"-->");
                match end {
                    Some(i) => self.pos += i + 3,
                    None => return Err(invalid("未闭合的注释")),
                }
                continue;
            }
            if self.rest().starts_with(b"</") {
                self.pos += 2;
                let close = self.name()?;
                if close != name {
                    return Err(invalid(format!("结束标签 </{close}> 与 <{name}> 不匹配")));
                }
                self.skip_ws();
                self.expect(b'>')?;
                break;
            }
            if self.peek() == Some(b'<') {
                children.push(self.parse_element(depth + 1)?);
                continue;
            }
            if self.peek().is_none() {
                return Err(invalid(format!("元素 <{name}> 未闭合")));
            }
            // 文本片段
            let start = self.pos;
            while matches!(self.peek(), Some(b) if b != b'<') {
                self.pos += 1;
            }
            let raw = std::str::from_utf8(&self.bytes[start..self.pos])
                .map_err(|_| invalid("文本不是合法 UTF-8"))?;
            let decoded = decode_entities(raw)?;
            if !decoded.trim().is_empty() {
                text.push_str(&decoded);
            }
        }
        Ok(Element {
            name,
            attrs,
            children,
            text,
        })
    }
}

/// 只允许 5 个标准实体；其它 `&…;`（包括自定义实体）一律拒绝。
fn decode_entities(raw: &str) -> Result<String, Error> {
    if !raw.contains('&') {
        return Ok(raw.to_string());
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(idx) = rest.find('&') {
        out.push_str(&rest[..idx]);
        let tail = &rest[idx..];
        let semi = tail.find(';').ok_or_else(|| invalid("未闭合的实体引用"))?;
        let name = &tail[1..semi];
        let rep = match name {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            other => {
                return Err(invalid(format!(
                    "未知实体引用 &{other};：拒绝（自定义实体是 XXE 的入口）"
                )))
            }
        };
        out.push(rep);
        rest = &tail[semi + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_elements_and_entities() {
        let doc = parse_document(
            br#"<root a="1"><child>x &amp; y</child><empty/></root>"#,
            XmlBudget::default(),
        )
        .expect("parse");
        assert_eq!(doc.name, "root");
        assert_eq!(doc.attrs.get("a").map(String::as_str), Some("1"));
        assert_eq!(doc.child("child").expect("child").text, "x & y");
        assert_eq!(doc.child("empty").expect("empty").children.len(), 0);
    }

    #[test]
    fn comments_and_prolog_are_skipped() {
        let doc = parse_document(
            br#"<?xml version="1.0"?><!-- hi --><r><!-- inner --><a>1</a></r>"#,
            XmlBudget::default(),
        )
        .expect("parse");
        assert_eq!(doc.child("a").expect("a").text, "1");
    }
}
