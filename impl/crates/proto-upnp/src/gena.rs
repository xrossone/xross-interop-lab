//! GENA 通知的**构造与调度**（T42+）：NOTIFY 字节、propertyset 正文、LastChange 转义、SEQ 规则。
//!
//! 字段来源见 `specs-reviewed/m07-dlna-upnp-av.md` 的 F-23..F-29（R46 pupnp 设备侧、
//! R49 rygel 服务侧）。**范围边界（F-22）**：本模块只产出通知**字节**并记账，**不建立任何连接**——
//! 字节交给调用方提供的 [`NotifyTransport`] 落地（路由/NAT/多接口行为不归本仓）。lab gate 机器检查
//! 本文件里不出现网络客户端或进程调用。
//!
//! 三条容易做错的来源事实（都写进了代码注释与测试）：
//! 1. **不发送 XML 声明**（F-24）：来源留着 `XML_VERSION` 宏但注释说与其他厂商不互操作，故不发送；
//! 2. **转义由值的一方负责**（F-25）：来源把值原样写进正文 ⇒ LastChange 的内嵌 XML 必须先整体转义；
//! 3. `SEQ` **从 0 开始**（初始事件），之后每次 +1（F-28）。

use interop_contract::error::{Error, ErrorCode};

/// GENA 事件命名空间（F-24：propertyset 的 `e:` 前缀）。
pub const EVENT_NAMESPACE: &str = "urn:schemas-upnp-org:event-1-0";
/// 通知头取值（F-23/F-29）。
pub const NT_EVENT: &str = "upnp:event";
/// 通知头取值（F-23/F-29）。
pub const NTS_PROPCHANGE: &str = "upnp:propchange";
/// 通知正文的 Content-Type（F-23 来源原样）。
pub const NOTIFY_CONTENT_TYPE: &str = "text/xml; charset=\"utf-8\"";
/// AVTransport 的 LastChange 命名空间（F-26）。
pub const AVT_EVENT_NS: &str = "urn:schemas-upnp-org:metadata-1-0/AVT/";
/// RenderingControl 的 LastChange 命名空间（F-26）。
pub const RCS_EVENT_NS: &str = "urn:schemas-upnp-org:metadata-1-0/RCS/";
/// LastChange 合并窗口（F-27：**来源实现取值** 150 ms，不是规范常量）。
pub const LAST_CHANGE_COALESCE_MS: u64 = 150;

fn invalid(msg: impl Into<String>) -> Error {
    Error::new(ErrorCode::InvalidFrame, msg)
}

/// XML 文本转义（F-25：来源不做转义 ⇒ 由值的一方负责）。
///
/// 覆盖 `& < > " '`——属性值里用的是双引号，因此 `"` 也必须转义。
pub fn escape_xml_text(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            other => out.push(other),
        }
    }
    out
}

/// 某个服务的变量变化记录（可变状态 → LastChange 内嵌文档）。
#[derive(Debug, Clone, Default)]
pub struct LastChangeLog {
    namespace: &'static str,
    entries: Vec<String>,
}

impl LastChangeLog {
    /// 按服务命名空间新建（AVT/RCS）。
    pub fn new(namespace: &'static str) -> Self {
        Self {
            namespace,
            entries: Vec::new(),
        }
    }

    pub fn avt() -> Self {
        Self::new(AVT_EVENT_NS)
    }

    pub fn rcs() -> Self {
        Self::new(RCS_EVENT_NS)
    }

    /// 记录一次变量变化（F-26：`<VAR val="已转义值"/>`）。
    pub fn log(&mut self, variable: &str, value: &str) {
        self.entries.push(format!(
            "<{variable} val=\"{}\"/>",
            escape_xml_text(value)
        ));
    }

    /// 带通道的变量（F-26：`<VAR val=".." channel=".."/>`，RenderingControl 用）。
    pub fn log_with_channel(&mut self, variable: &str, value: &str, channel: &str) {
        self.entries.push(format!(
            "<{variable} val=\"{}\" channel=\"{}\"/>",
            escape_xml_text(value),
            escape_xml_text(channel)
        ));
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 生成内嵌文档（F-26）：**未转义**的 XML 文档本身；放进 propertyset 时还要整体转义一次。
    pub fn finish(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "<Event xmlns=\"{}\"><InstanceID val=\"0\">",
            self.namespace
        ));
        for entry in &self.entries {
            out.push_str(entry);
        }
        out.push_str("</InstanceID></Event>");
        out
    }

    /// 清空（发送后重置，来源同此：`hash.clear()`）。
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

/// 通知正文（propertyset）构造（F-24）。
///
/// 变量值**必须已经转义**（[`escape_xml_text`]）；本函数不再转义，与来源一致（F-25）。
pub fn build_propertyset(properties: &[(&str, String)]) -> String {
    let mut out = String::new();
    out.push_str(&format!("<e:propertyset xmlns:e=\"{EVENT_NAMESPACE}\">\n"));
    for (name, value) in properties {
        out.push_str("<e:property>\n");
        out.push_str(&format!("<{name}>{value}</{name}>\n"));
        out.push_str("</e:property>\n");
    }
    out.push_str("</e:propertyset>\n\n");
    out
}

/// 一条待投递的通知（已经是有序的字节，未建立任何连接）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotifyRequest {
    pub callback_path: String,
    pub sid: String,
    pub seq: u32,
    pub body: String,
}

impl NotifyRequest {
    /// 渲染为完整的 HTTP 请求字节（F-23：头集合与 **Content-Length = 正文 + 2** 的来源行为）。
    pub fn to_bytes(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("NOTIFY {} HTTP/1.1\r\n", self.callback_path));
        out.push_str(&format!("Content-Type: {NOTIFY_CONTENT_TYPE}\r\n"));
        // 来源的既有行为：正文以 "\n\n" 结尾，故 Content-Length 报的是正文字节数 + 2。
        out.push_str(&format!(
            "Content-Length: {}\r\n",
            self.body.len() + 2
        ));
        out.push_str(&format!("NT: {NT_EVENT}\r\n"));
        out.push_str(&format!("NTS: {NTS_PROPCHANGE}\r\n"));
        out.push_str(&format!("SID: {}\r\n", self.sid));
        out.push_str(&format!("SEQ: {}\r\n", self.seq));
        out.push_str("\r\n");
        out.push_str(&self.body);
        out
    }
}

/// 通知的落地方（**由调用方实现**：本仓不建立连接，F-22）。
pub trait NotifyTransport {
    /// 把已经渲染好的请求字节送出去；错误由调用方决定是否重试。
    fn deliver(&mut self, request: &NotifyRequest) -> Result<(), Error>;
}

/// 订阅的投递状态：SID + SEQ 记账（F-28）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotifySubscription {
    pub sid: String,
    pub callback_path: String,
    seq: u32,
    initial_sent: bool,
}

impl NotifySubscription {
    /// 新订阅：`SEQ` **从 0 开始**（初始事件，F-28）。
    pub fn new(sid: &str, callback_path: &str) -> Result<Self, Error> {
        if sid.is_empty() {
            return Err(invalid("SID 不得为空"));
        }
        if !sid.starts_with("uuid:") {
            return Err(invalid(format!(
                "SID 必须是 uuid: 形式（F-23 来源形态），收到 {sid:?}"
            )));
        }
        if callback_path.is_empty() || !callback_path.starts_with('/') {
            return Err(invalid("回调路径必须是绝对路径（/…）"));
        }
        Ok(Self {
            sid: sid.to_string(),
            callback_path: callback_path.to_string(),
            seq: 0,
            initial_sent: false,
        })
    }

    pub fn seq(&self) -> u32 {
        self.seq
    }

    pub fn initial_sent(&self) -> bool {
        self.initial_sent
    }

    /// 构造下一条通知并推进 SEQ（构造**不投递**，投递由调用方通过 transport 做）。
    pub fn next_notify(&mut self, body: String) -> NotifyRequest {
        let request = NotifyRequest {
            callback_path: self.callback_path.clone(),
            sid: self.sid.clone(),
            seq: self.seq,
            body,
        };
        self.initial_sent = true;
        self.seq = self.seq.wrapping_add(1);
        // 来源规则（F-28）：自增后若"为负"（即 u32 溢出）则回绕到 1——0 只用于初始事件。
        if self.seq == 0 {
            self.seq = 1;
        }
        request
    }
}

/// 通知调度器：把 LastChange 的变化合并成一条通知（F-27 的 150 ms 窗口语义，**由调用方驱动时钟**）。
#[derive(Debug)]
pub struct NotifyScheduler {
    subscription: NotifySubscription,
    pending: LastChangeLog,
    coalesce_ms: u64,
    window_started_ms: Option<u64>,
    sent: u32,
}

impl NotifyScheduler {
    /// 默认窗口 = 来源取值 150 ms（F-27）。
    pub fn new(subscription: NotifySubscription, namespace: &'static str) -> Self {
        Self {
            subscription,
            pending: LastChangeLog::new(namespace),
            coalesce_ms: LAST_CHANGE_COALESCE_MS,
            window_started_ms: None,
            sent: 0,
        }
    }

    /// 覆盖合并窗口（**本仓策略可配**；来源的 150 ms 是取值而非规范常量）。
    pub fn with_window(mut self, coalesce_ms: u64) -> Self {
        self.coalesce_ms = coalesce_ms;
        self
    }

    pub fn subscription(&self) -> &NotifySubscription {
        &self.subscription
    }

    pub fn sent(&self) -> u32 {
        self.sent
    }

    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// 记录变量变化（启动合并窗口）。
    pub fn log(&mut self, variable: &str, value: &str, now_ms: u64) {
        self.pending.log(variable, value);
        if self.window_started_ms.is_none() {
            self.window_started_ms = Some(now_ms);
        }
    }

    pub fn log_with_channel(&mut self, variable: &str, value: &str, channel: &str, now_ms: u64) {
        self.pending.log_with_channel(variable, value, channel);
        if self.window_started_ms.is_none() {
            self.window_started_ms = Some(now_ms);
        }
    }

    /// 到窗口就绪时刻则产出**一条**通知（含 LastChange）；未到点或没有待发变化 → `None`。
    ///
    /// 需要**初始事件**（订阅后立即发一次，SEQ=0）时由调用方先调 [`Self::initial_notify`]。
    pub fn tick(&mut self, now_ms: u64) -> Option<NotifyRequest> {
        let started = self.window_started_ms?;
        if self.pending.is_empty() || now_ms.saturating_sub(started) < self.coalesce_ms {
            return None;
        }
        Some(self.flush())
    }

    /// 立即产出（忽略窗口）。
    pub fn flush(&mut self) -> NotifyRequest {
        let document = self.pending.finish();
        self.pending.clear();
        self.window_started_ms = None;
        self.sent += 1;
        // LastChange 的值是内嵌 XML 文档：作为字符串放进 propertyset 时必须**整体转义**（F-25）。
        self.subscription.next_notify(build_propertyset(&[(
            "LastChange",
            escape_xml_text(&document),
        )]))
    }

    /// 初始事件（订阅成功后立即发一次，SEQ=0，F-28）：带上当前完整状态。
    pub fn initial_notify(
        &mut self,
        variables: &[(&str, &str)],
        now_ms: u64,
    ) -> NotifyRequest {
        for (name, value) in variables {
            self.pending.log(name, value);
        }
        let _ = now_ms;
        self.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn propertyset_has_no_xml_declaration_and_source_envelope() {
        let body = build_propertyset(&[("LastChange", escape_xml_text("<Event/>"))]);
        assert!(
            !body.contains("<?xml"),
            "来源明确不发送 XML 声明（F-24）：{body}"
        );
        assert!(body.starts_with(&format!(
            "<e:propertyset xmlns:e=\"{EVENT_NAMESPACE}\">\n"
        )));
        assert!(body.contains("<e:property>\n<LastChange>&lt;Event/&gt;</LastChange>\n</e:property>\n"));
        assert!(body.ends_with("</e:propertyset>\n\n"), "正文以两个换行结尾（来源行为）");
    }

    #[test]
    fn notify_bytes_match_the_source_headers_and_content_length_quirk() {
        let mut subscription = NotifySubscription::new("uuid:abc", "/notify").expect("合法");
        let body = build_propertyset(&[("LastChange", "x".to_string())]);
        let request = subscription.next_notify(body.clone());
        let bytes = request.to_bytes();
        assert!(bytes.starts_with("NOTIFY /notify HTTP/1.1\r\n"));
        assert!(bytes.contains(&format!("Content-Type: {NOTIFY_CONTENT_TYPE}\r\n")));
        assert!(bytes.contains(&format!("Content-Length: {}\r\n", body.len() + 2)), "来源怪癖");
        assert!(bytes.contains("NT: upnp:event\r\n"));
        assert!(bytes.contains("NTS: upnp:propchange\r\n"));
        assert!(bytes.contains("SID: uuid:abc\r\n"));
        assert!(bytes.contains("SEQ: 0\r\n"));
        assert!(bytes.ends_with(&body));
    }

    #[test]
    fn last_change_document_shape_and_channel_variant() {
        let mut log = LastChangeLog::avt();
        log.log("TransportState", "PLAYING");
        assert_eq!(
            log.finish(),
            format!("<Event xmlns=\"{AVT_EVENT_NS}\"><InstanceID val=\"0\"><TransportState val=\"PLAYING\"/></InstanceID></Event>")
        );
        let mut rcs = LastChangeLog::rcs();
        rcs.log_with_channel("Volume", "50", "Master");
        assert!(rcs.finish().contains("<Volume val=\"50\" channel=\"Master\"/>"));
        assert!(rcs.finish().contains(RCS_EVENT_NS));
        // 值里的引号/尖括号必须被转义（F-25：来源不转义，责任在值的一方）。
        let mut tricky = LastChangeLog::avt();
        tricky.log("CurrentTrackMetaData", "<a b=\"c\">&");
        assert!(tricky.finish().contains("val=\"&lt;a b=&quot;c&quot;&gt;&amp;\""));
    }

    #[test]
    fn seq_starts_at_zero_then_increments() {
        let mut subscription = NotifySubscription::new("uuid:x", "/n").expect("合法");
        assert_eq!(subscription.seq(), 0);
        let first = subscription.next_notify("a".to_string());
        assert_eq!(first.seq, 0, "初始事件 SEQ=0（F-28）");
        let second = subscription.next_notify("b".to_string());
        assert_eq!(second.seq, 1);
        assert!(subscription.initial_sent());
    }

    #[test]
    fn invalid_sid_and_callback_are_refused() {
        assert_eq!(
            NotifySubscription::new("abc", "/n").unwrap_err().code,
            ErrorCode::InvalidFrame
        );
        assert_eq!(
            NotifySubscription::new("uuid:a", "notify").unwrap_err().code,
            ErrorCode::InvalidFrame
        );
        assert_eq!(
            NotifySubscription::new("", "/n").unwrap_err().code,
            ErrorCode::InvalidFrame
        );
    }

    #[test]
    fn scheduler_coalesces_within_the_window() {
        let subscription = NotifySubscription::new("uuid:z", "/n").expect("合法");
        let mut scheduler = NotifyScheduler::new(subscription, AVT_EVENT_NS);
        scheduler.log("TransportState", "PLAYING", 1_000);
        scheduler.log("CurrentTrackURI", "http://x/1", 1_050);
        assert!(scheduler.tick(1_100).is_none(), "窗口内不产出（150 ms）");
        let request = scheduler.tick(1_150).expect("到点产出");
        assert_eq!(request.seq, 0);
        let body = &request.body;
        assert!(body.contains("TransportState"), "两条变化合并进一条通知");
        assert!(body.contains("CurrentTrackURI"));
        assert!(!scheduler.has_pending(), "发送后清空");
        assert_eq!(scheduler.sent(), 1);
        // 再记录 → 又是一条，SEQ 递增。
        scheduler.log("TransportState", "STOPPED", 2_000);
        let next = scheduler.tick(2_200).expect("第二条");
        assert_eq!(next.seq, 1);
    }

    #[test]
    fn initial_notify_carries_full_state() {
        let subscription = NotifySubscription::new("uuid:i", "/n").expect("合法");
        let mut scheduler = NotifyScheduler::new(subscription, AVT_EVENT_NS);
        let request = scheduler.initial_notify(
            &[("TransportState", "STOPPED"), ("TransportStatus", "OK")],
            0,
        );
        assert_eq!(request.seq, 0);
        assert!(request.body.contains("TransportState"));
        assert!(request.body.contains("TransportStatus"));
    }
}
