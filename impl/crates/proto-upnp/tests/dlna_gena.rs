//! T42+ 验收：GENA 通知的构造与调度（语料 dl-025..dl-030）。
//!
//! **范围**：本仓只产出通知**字节**与记账，不建立连接（F-22）；测试里用一个记录型的
//! `Transport` 假装投递，验证"给调用方的字节"是否正确。

#![allow(clippy::result_large_err)]

use interop_contract::error::ErrorCode;
use proto_upnp::gena::{
    build_propertyset, escape_xml_text, LastChangeLog, NotifyRequest, NotifyScheduler,
    NotifySubscription, NotifyTransport, AVT_EVENT_NS, EVENT_NAMESPACE, LAST_CHANGE_COALESCE_MS,
    NOTIFY_CONTENT_TYPE, NT_EVENT, NTS_PROPCHANGE, RCS_EVENT_NS,
};

/// 记录型 transport：**只记录字节**，不做任何 I/O（本仓不让 gena 模块自己建连接）。
#[derive(Default)]
struct Recorder {
    delivered: Vec<NotifyRequest>,
}

impl NotifyTransport for Recorder {
    fn deliver(&mut self, request: &NotifyRequest) -> Result<(), interop_contract::error::Error> {
        self.delivered.push(request.clone());
        Ok(())
    }
}

// ---------------------------------------------------------------- dl-025

#[test]
fn dl_025_notify_bytes_match_the_fact_table() {
    let mut subscription = NotifySubscription::new("uuid:6f1e", "/evt").expect("合法");
    let body = build_propertyset(&[("LastChange", escape_xml_text("<Event/>"))]);
    let request = subscription.next_notify(body.clone());
    let bytes = request.to_bytes();

    // 请求行 + 头集合（顺序按字段表）。
    let head = bytes.split("\r\n\r\n").next().expect("头");
    let lines: Vec<&str> = head.split("\r\n").collect();
    assert_eq!(lines[0], "NOTIFY /evt HTTP/1.1");
    assert_eq!(lines[1], format!("Content-Type: {NOTIFY_CONTENT_TYPE}"));
    assert_eq!(lines[2], format!("Content-Length: {}", body.len() + 2));
    assert_eq!(lines[3], format!("NT: {NT_EVENT}"));
    assert_eq!(lines[4], format!("NTS: {NTS_PROPCHANGE}"));
    assert_eq!(lines[5], "SID: uuid:6f1e");
    assert_eq!(lines[6], "SEQ: 0");

    // 正文：无 XML 声明、每个变量一个 e:property、以两个换行结尾。
    assert!(!bytes.contains("<?xml"), "不得发送 XML 声明（F-24）");
    assert!(body.starts_with(&format!("<e:propertyset xmlns:e=\"{EVENT_NAMESPACE}\">\n")));
    assert_eq!(body.matches("<e:property>").count(), 1);
    assert!(body.ends_with("</e:propertyset>\n\n"));

    // 投递交给调用方（本仓不建连接）。
    let mut recorder = Recorder::default();
    recorder.deliver(&request).expect("记录型投递");
    assert_eq!(recorder.delivered.len(), 1);
    assert_eq!(recorder.delivered[0].body, body);
}

// ---------------------------------------------------------------- dl-026 / dl-027

#[test]
fn dl_026_last_change_is_escaped_before_it_enters_the_propertyset() {
    let mut log = LastChangeLog::avt();
    log.log("TransportState", "PLAYING");
    let document = log.finish();
    assert_eq!(
        document,
        format!("<Event xmlns=\"{AVT_EVENT_NS}\"><InstanceID val=\"0\"><TransportState val=\"PLAYING\"/></InstanceID></Event>")
    );
    // 放进 propertyset 必须**整体转义**（F-25：来源把转义责任交给值的一方）。
    let body = build_propertyset(&[("LastChange", escape_xml_text(&document))]);
    assert!(body.contains("&lt;Event xmlns="), "内嵌文档必须被转义：{body}");
    assert!(
        !body.contains("<Event xmlns="),
        "未转义的内嵌文档会让正文非法（这正是来源不做转义要我们补的地方）"
    );
    // 反例：不转义直接塞进去 → 正文里出现裸的 <Event>，说明"必须转义"这条是**可判定**的。
    let raw = build_propertyset(&[("LastChange", document.clone())]);
    assert!(raw.contains("<Event xmlns="), "未转义形态确实可被检出");
    assert_ne!(raw, body);
}

#[test]
fn dl_027_rendering_control_channel_variant() {
    let mut log = LastChangeLog::rcs();
    log.log_with_channel("Volume", "50", "Master");
    log.log("Mute", "0");
    let document = log.finish();
    assert!(document.contains("<Volume val=\"50\" channel=\"Master\"/>"));
    assert!(document.contains("<Mute val=\"0\"/>"));
    assert!(document.contains(RCS_EVENT_NS));
    assert!(!document.contains(AVT_EVENT_NS));
    assert_eq!(log.len(), 2);
    // 值里的特殊字符在 log 阶段就转义（属性值用双引号 ⇒ 引号也要转）。
    let mut tricky = LastChangeLog::rcs();
    tricky.log("PresetNameFactoryDefaults", "a&b<c>\"d\"");
    assert!(tricky.finish().contains("val=\"a&amp;b&lt;c&gt;&quot;d&quot;\""));
}

// ---------------------------------------------------------------- dl-028

#[test]
fn dl_028_seq_starts_at_zero_and_never_skips() {
    let mut subscription = NotifySubscription::new("uuid:seq", "/n").expect("合法");
    let mut seen = Vec::new();
    for _ in 0..4 {
        seen.push(subscription.next_notify("x".to_string()).seq);
    }
    assert_eq!(seen, vec![0, 1, 2, 3], "初始 0 之后严格 +1（F-28）");
    assert!(subscription.initial_sent());
}

// ---------------------------------------------------------------- dl-029

#[test]
fn dl_029_negative_shapes_are_refused() {
    // SID 形式不符 / 回调路径不是绝对路径 / 空 SID。
    assert_eq!(
        NotifySubscription::new("6f1e", "/n").unwrap_err().code,
        ErrorCode::InvalidFrame
    );
    assert_eq!(
        NotifySubscription::new("uuid:a", "http://x/").unwrap_err().code,
        ErrorCode::InvalidFrame
    );
    assert_eq!(
        NotifySubscription::new("uuid:a", "").unwrap_err().code,
        ErrorCode::InvalidFrame
    );
    // NT/NTS 取值是**控制点侧**的校验（F-29）：本仓把常量暴露出来，
    // 供上层在解析通知时比对；这里确认它们与来源一致、且不是别家的拼写。
    assert_eq!(NT_EVENT, "upnp:event");
    assert_eq!(NTS_PROPCHANGE, "upnp:propchange");
}

// ---------------------------------------------------------------- dl-030

#[test]
fn dl_030_escaping_covers_the_dangerous_characters() {
    assert_eq!(escape_xml_text("<&>\"'"), "&lt;&amp;&gt;&quot;&apos;");
    assert_eq!(escape_xml_text("plain"), "plain");
    // ]]> 不在转义表里（它不是标记定界符本身），但尖括号已转 ⇒ 无法提前闭合 CDATA 或元素。
    assert_eq!(escape_xml_text("]]>"), "]]&gt;");
    // 转义后的值放进正文可被 XML 子集解析器读回（本仓 xml 模块）。
    let body = build_propertyset(&[("LastChange", escape_xml_text("<Event a=\"b\"/>"))]);
    assert!(!body.contains("<Event"));
}

// ---------------------------------------------------------------- 合并窗口（F-27）

#[test]
fn coalescing_window_is_source_taken_and_configurable() {
    let subscription = NotifySubscription::new("uuid:c", "/n").expect("合法");
    let mut scheduler = NotifyScheduler::new(subscription, AVT_EVENT_NS);
    scheduler.log("TransportState", "PLAYING", 10_000);
    scheduler.log("TransportStatus", "OK", 10_010);
    assert!(
        scheduler.tick(10_000 + LAST_CHANGE_COALESCE_MS - 1).is_none(),
        "窗口未到（来源取值 150 ms）"
    );
    let request = scheduler.tick(10_000 + LAST_CHANGE_COALESCE_MS).expect("到点");
    assert_eq!(request.seq, 0, "第一条通知仍是初始 SEQ");
    assert_eq!(request.body.matches("<e:property>").count(), 1, "同一窗口合并成一条通知");
    assert!(request.body.contains("TransportState") && request.body.contains("TransportStatus"));

    // 窗口可配置（本仓策略）：0 ms 表示不合并。
    let subscription = NotifySubscription::new("uuid:d", "/n").expect("合法");
    let mut immediate = NotifyScheduler::new(subscription, AVT_EVENT_NS).with_window(0);
    immediate.log("Volume", "1", 0);
    assert!(immediate.tick(0).is_some(), "0 窗口立即产出");
}
