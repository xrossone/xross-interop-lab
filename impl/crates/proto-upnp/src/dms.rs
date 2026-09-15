//! 受限 DMS（Digital Media Server，plans/03 §T41）：ContentDirectory 的授权与分页。
//!
//! 规矩（T41-03）：
//! - **只暴露批准目录**：objectID 必须命中批准集合（根或其子项），否则 701（NO_SUCH_OBJECT）；
//! - **分页有上限**：`RequestedCount` 超上限 → 402（INVALID_ARGS）；`BrowseFlag` 未知 → 402；
//! - 不扫描全盘、不暴露未批准扩展名；`Result` 只是最小 DIDL-Lite 片段（不是完整实现）。
//!
//! 错误码来源：R49 rygel `src/librygel-server/rygel-content-directory.vala:33,47,49`（701/720/402）。

use std::fmt;

/// ContentDirectory 错误码（F-08）。
pub const NO_SUCH_OBJECT: u32 = 701;
pub const CANT_PROCESS: u32 = 720;
pub const INVALID_ARGS: u32 = 402;

/// 分页上限（本仓策略值）。
pub const MAX_REQUESTED_COUNT: u32 = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DmsError {
    pub code: u32,
    pub description: String,
}

impl DmsError {
    pub fn new(code: u32, description: impl Into<String>) -> Self {
        Self {
            code,
            description: description.into(),
        }
    }

    pub fn code(&self) -> u32 {
        self.code
    }
}

impl fmt::Display for DmsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "UPnPError {}: {}", self.code, self.description)
    }
}

impl std::error::Error for DmsError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowseFlag {
    BrowseMetadata,
    BrowseDirectChildren,
}

impl BrowseFlag {
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::BrowseMetadata => "BrowseMetadata",
            Self::BrowseDirectChildren => "BrowseDirectChildren",
        }
    }

    pub fn from_wire(s: &str) -> Result<Self, DmsError> {
        match s {
            "BrowseMetadata" => Ok(Self::BrowseMetadata),
            "BrowseDirectChildren" => Ok(Self::BrowseDirectChildren),
            other => Err(DmsError::new(
                INVALID_ARGS,
                format!("未知 BrowseFlag {other:?}"),
            )),
        }
    }
}

/// 一个批准的内容根（子项必须显式列出，不做磁盘扫描）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentRoot {
    pub object_id: String,
    pub title: String,
    /// 子对象 (object_id, title)
    pub children: Vec<(String, String)>,
    /// 允许暴露的扩展名（小写，不含点）
    pub extensions: Vec<String>,
}

impl ContentRoot {
    pub fn new(object_id: &str, children: Vec<(&str, &str)>, extensions: Vec<&str>) -> Self {
        Self {
            object_id: object_id.to_string(),
            title: object_id.to_string(),
            children: children
                .into_iter()
                .map(|(id, title)| (id.to_string(), title.to_string()))
                .collect(),
            extensions: extensions.into_iter().map(|e| e.to_ascii_lowercase()).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowseResult {
    pub number_returned: u32,
    pub total_matches: u32,
    pub update_id: u32,
    /// 最小 DIDL-Lite 片段（只有对象 id/标题/class）
    pub didl: String,
}

/// 受限 ContentDirectory。
#[derive(Debug)]
pub struct Dms {
    roots: Vec<ContentRoot>,
    max_requested_count: u32,
}

impl Dms {
    pub fn new(roots: Vec<ContentRoot>) -> Self {
        Self {
            roots,
            max_requested_count: MAX_REQUESTED_COUNT,
        }
    }

    /// 该扩展名是否允许暴露。
    pub fn extension_allowed(&self, ext: &str) -> bool {
        let ext = ext.trim_start_matches('.').to_ascii_lowercase();
        self.roots.iter().any(|r| r.extensions.contains(&ext))
    }

    pub fn browse(
        &self,
        object_id: &str,
        flag: BrowseFlag,
        starting_index: u32,
        requested_count: u32,
    ) -> Result<BrowseResult, DmsError> {
        self.browse_raw(object_id, flag.as_wire(), starting_index, requested_count)
    }

    /// 与 `browse` 相同，但 BrowseFlag 以字符串给出（用于校验非法值）。
    pub fn browse_raw(
        &self,
        object_id: &str,
        flag: &str,
        starting_index: u32,
        requested_count: u32,
    ) -> Result<BrowseResult, DmsError> {
        let flag = BrowseFlag::from_wire(flag)?;
        if requested_count > self.max_requested_count {
            return Err(DmsError::new(
                INVALID_ARGS,
                format!(
                    "RequestedCount {requested_count} 超过策略上限 {}",
                    self.max_requested_count
                ),
            ));
        }

        // 授权集合 = 批准的根 + 显式登记的子对象；其余一律 701
        let root = match self.roots.iter().find(|r| r.object_id == object_id) {
            Some(r) => r,
            None => {
                let child = self
                    .roots
                    .iter()
                    .flat_map(|r| r.children.iter())
                    .find(|(id, _)| id == object_id)
                    .ok_or_else(|| {
                        DmsError::new(NO_SUCH_OBJECT, format!("未知 objectID {object_id:?}"))
                    })?;
                return match flag {
                    // 子对象的元数据可读；它没有登记子项（本模型只两层）
                    BrowseFlag::BrowseMetadata => Ok(BrowseResult {
                        number_returned: 1,
                        total_matches: 1,
                        update_id: 0,
                        didl: didl_item(&child.0, &child.1, "object.item"),
                    }),
                    BrowseFlag::BrowseDirectChildren => Ok(BrowseResult {
                        number_returned: 0,
                        total_matches: 0,
                        update_id: 0,
                        didl: "<DIDL-Lite></DIDL-Lite>".to_string(),
                    }),
                };
            }
        };

        match flag {
            BrowseFlag::BrowseMetadata => Ok(BrowseResult {
                number_returned: 1,
                total_matches: 1,
                update_id: 0,
                didl: didl_item(&root.object_id, &root.title, "object.container.storageFolder"),
            }),
            BrowseFlag::BrowseDirectChildren => {
                let total = root.children.len() as u32;
                let start = starting_index.min(total);
                let take = requested_count.min(total - start);
                let slice = &root.children[start as usize..(start + take) as usize];
                let mut didl = String::from("<DIDL-Lite>");
                for (id, title) in slice {
                    didl.push_str(&didl_item(id, title, "object.container"));
                }
                didl.push_str("</DIDL-Lite>");
                Ok(BrowseResult {
                    number_returned: slice.len() as u32,
                    total_matches: total,
                    update_id: 0,
                    didl,
                })
            }
        }
    }

    /// 子对象授权：子 id 必须显式登记在某 root 下。
    pub fn child_allowed(&self, object_id: &str) -> bool {
        self.roots
            .iter()
            .any(|r| r.children.iter().any(|(id, _)| id == object_id))
    }
}

fn didl_item(id: &str, title: &str, class: &str) -> String {
    format!(
        "<item><dc:title>{}</dc:title><upnp:class>{class}</upnp:class><upnp:objectID>{}</upnp:objectID></item>",
        escape(title),
        escape(id)
    )
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dms() -> Dms {
        Dms::new(vec![ContentRoot::new(
            "0",
            vec![("music", "Music")],
            vec!["mp3"],
        )])
    }

    #[test]
    fn metadata_and_children_browse_work() {
        let d = dms();
        let meta = d
            .browse("music", BrowseFlag::BrowseMetadata, 0, 1)
            .expect("metadata");
        assert_eq!(meta.number_returned, 1);
        let kids = d
            .browse("0", BrowseFlag::BrowseDirectChildren, 0, 10)
            .expect("children");
        assert_eq!(kids.total_matches, 1);
        assert!(kids.didl.contains("Music"));
    }

    #[test]
    fn unknown_ids_and_bad_args_are_rejected() {
        let d = dms();
        assert_eq!(
            d.browse("nope", BrowseFlag::BrowseDirectChildren, 0, 1)
                .expect_err("未知 id")
                .code(),
            NO_SUCH_OBJECT
        );
        assert_eq!(
            d.browse("0", BrowseFlag::BrowseDirectChildren, 0, 10_000)
                .expect_err("超上限")
                .code(),
            INVALID_ARGS
        );
    }
}
