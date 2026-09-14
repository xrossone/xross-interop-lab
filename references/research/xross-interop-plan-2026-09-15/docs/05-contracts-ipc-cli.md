# 公共契约、控制IPC、数据面与CLI

**本文件定义计划实现的内部v0.1契约。不是现有Xross RPC，也不是任何外部wire协议。**执行者在T01建立schema与golden vectors，再并行实现provider。

## 1. 标识与版本

所有公共ID为不可猜测的本地opaque string，类型分别为`EndpointId`、`OfferId`、`SessionId`、`EntryId`、`LeaseId`、`PresentationId`、`RunId`。示例文字如`ofr_demo`只用于fixture，生产用CSPRNG生成。不要把path/token/remote address放ID。

- API：`interop.api/0.1`；初版`major=0, minor=1`也实行明确兼容策略。
- 未知请求method返回`unsupported-method`；未知关键enum值拒绝，不映射成default。
- 事件新字段允许忽略；安全相关新字段需要`required_features`协商，缺少就fail closed。
- Big整数（文件大小、序号、时间戳）在JSON中编码为十进制字符串，避免JS 53bit丢精度。
- 所有timeout使用单调时钟，显示日期用UTC RFC3339。两者不得直接相减。

## 2. 领域结构（规范示例）

```json
{
  "synthetic": true,
  "profile_id": "quickshare.lan.v1",
  "role": "receive",
  "provider_id": "quickshare-rust",
  "state": "catalogued",
  "available": true,
  "prerequisites": ["selected-lan-interface", "user-acceptance"],
  "file_features": {"partial_accept": false, "resume": "none"},
  "media_forms": [],
  "security": {"transport": "protocol-negotiated", "xross_identity": false},
  "evidence_ids": ["run_example_not_a_real_result"]
}
```

该示例不是当前项目已经达到device-verified的记录。fixture文件必须带`synthetic=true`，不能进release能力矩阵。

`ShareOffer`字段：offer_id、endpoint_id、profile_id、entries、total_bytes（nullable）、deadline、requested_actions、trust_context、attempt_id。每entry含不受信任的`display_name`、`relative_components`（可选目录）、media_type_hint、declared_size、wire_payload_id（provider私有）和本地entry_id。**名称/声明的MIME不构成可执行安全依据。**

`TransferProgress`字段：session_id、entry_id、bytes_received、bytes_committed、total_bytes（nullable）、rate_sample_window_ms、completion_level。completion_level为`receiving | local-durable | remote-acknowledged | verified`之一或明确集合；不能用一个100%掩盖差异。

`MediaDescriptor`字段：session_id、intent、tracks、source_form、controls、permissions、owner_provider。track包含codec、format_id、timebase、clock_domain、channel_layout/video_dimensions、transport_security、codec_extradata_hash。`NativePresentation`的opaque token不能被其他worker随意使用。

## 3. 统一错误

错误对象固定含`code`、`message`、`retryable`、`phase`、`profile_id`、`evidence_id`（可选）、`remedy`（可选结构对象），无凭据。

初始codes：`unsupported-profile`、`unsupported-role`、`unsupported-feature`、`platform-unavailable`、`permission-required`、`hardware-unavailable`、`pairing-failed`、`auth-denied`、`offer-expired`、`busy`、`deadline-exceeded`、`resource-limit`、`invalid-frame`、`integrity-failed`、`source-changed`、`destination-unavailable`、`cancelled`、`provider-crashed`、`event-gap`、`license-gated`、`vendor-gated`、`protected-content`。

## 4. 状态机

**文件：**`discovered → connecting → negotiating → offered → awaiting-consent → transferring → verifying → committing → completed`。

不是所有外部协议都有逐一同名状态：provider记录`wire_phase`，投影到内部语义，不人为添加对端不存在的确认。任何非terminal状态可走cancelling→cancelled；错误后failed带可重试策略。转发/跨协议重试创建新的attempt，不能假装原transfer透明续传。

**媒体：**`requested → authorizing → negotiating → buffering → active → draining → ended`；错误failed。format changes不新造身份，但递增format_id；重连产生新的clock generation。paused仅用于支持暂停的媒体模式。

**Idempotency：**调用accept/cancel/stop含request_id；同scope内重复请求返回同结果，不重复文件commit或创建第二个player。accept与expire race由一个authority序列化裁决，只有一个终态。

## 5. CLI约定

```text
xinterop doctor --json
xinterop capabilities --json
xinterop discover --profiles localsend,quickshare.lan --interface <name> --watch --json
xinterop receive --profile localsend --output <directory> --approval manual
xinterop offers list --json
xinterop offers accept <offer-id> --destination <approved-root-id>
xinterop offers reject <offer-id>
xinterop send --to <endpoint-id> --profile auto --file <path>
xinterop media receive --profile airplay.mirror --sink null
xinterop media receive --profile airplay.mirror --sink native-window
xinterop media cast --to <endpoint-id> --file <path> --purpose media-file
xinterop media cast --to <endpoint-id> --source test-pattern --purpose screen
xinterop sessions list --json
xinterop sessions stop <session-id>
xinterop evidence export <run-id> --redacted --output <directory>
```

这些是**拟实现CLI**，交付本包时并不存在。profile `auto`只选择已验收且符合用户purpose/security的路径。files与screen不共用模糊send语义。手机端不能访问的OS功能返回不可用。

退出码：0成功；2参数不合法；3profile/平台不可用；4拒绝或未授权；5超时；6远端协议错误；7本地I/O或integrity；8内部provider故障；130用户中断。JSON输出一个结果对象；watch为逐行JSON事件。日志写stderr，不混在stdout。

## 6. 本地控制IPC v0.1

Carrier选定：**Unix domain stream（macOS/Linux）或Windows named pipe**，内部**u32 big-endian长度 + UTF-8 JSON-RPC 2.0**。长度不包含自身4字节，最大262144bytes；0、超限、坏UTF8、非JSON关闭连接并记录安全错误。禁止批量JSON-RPC初版、禁止未知notification修改状态。

方法：

| 方法 | 请求核心字段 | 返回 |
|---|---|---|
| `hello` | api_major/api_minor/client_id/token/requested_scopes |版本、已授权scope、limits、instance_id |
| `ping` | 无（hello之后） | `pong`及instance_id |
| `capabilities.list` | filters |明确profile/role/state/prerequisite |
| `discovery.start/stop` |profile_ids/interface_ids/lease_duration | discovery_lease_id |
| `endpoints.list` |filters |观察快照、可用路由，不含完整秘密 |
| `offers.list/decide` |offer_id/accept_or_reject/destination_lease |结果/会话ID |
| `transfer.send/cancel` |endpoint_id/profile_id/content_read_lease |任务receipt |
| `media.start/attach/stop` |intent/source或profile/sink_choice |session/source/presentation reference |
| `events.subscribe` |after_sequence/filters |事件流；过期cursor返回gap+snapshot token |
| `diagnostics.snapshot` |redaction_profile |脱敏诊断，不给任意路径或shell命令 |

事件使用JSON-RPC notification `event`，payload含instance_id、sequence、session_id、type、data。保留最近4096条且总量不超过16MiB，较早cursor给event-gap而不是假装完整重放。客户端恢复先snapshot再接续。

控制例：

```json
{"jsonrpc":"2.0","id":"req-1","method":"offers.decide","params":{"offer_id":"ofr_fixture","decision":"reject"}}
```

**Golden vector：**UTF8字符串`{"jsonrpc":"2.0","id":1,"method":"ping"}`为40bytes（执行时由测试计算，不盲写常量）；frame前缀应等于实际bytes长度。必须测试一字节分片读取、两帧粘包、2^32−1长度、超限前拒绝分配。ping只允许hello之后。

认证：socket目录0700/socket0600；Windows pipe DACL仅目标用户及必要service SID，禁用远程pipe访问。服务验证本地peer身份（可用时）+每client scoped随机token。token保存在host提供的受控store，不放命令行/普通日志。worker由parent创建继承pipe传bootstrap，不通过公开environment发布权限。**同UID全权恶意进程仍可能突破简单文件权限；此控制不是对整台已失陷主机的安全保证。**

## 7. 数据面

初版数据面使用parent创建的专用binary pipe/UDS连接，与control分离。client先获得短时单次绑定grant；data连接发送grant ID并经owner校验。之后每frame：

```text
magic[4] = "XMD1"
header_version u16 BE = 1
kind u16 BE              # 1 encoded access-unit, 2 PCM block, 3 codec config, 4 end
stream_id u32 BE
flags u32 BE             # keyframe/discontinuity 等已定义位；未知critical位拒绝
sequence u64 BE
pts i64 BE               # 按descriptor timebase；无PTS用独立flag
payload_len u32 BE
payload[payload_len]
```

固定header **36bytes**。不得把裸pointer/FD整数作为可跨任意进程的授权。更高性能阶段可用有界shared-memory ring，但需要producer/consumer索引验证、槽位大小/所有权/生命周期；过期租约立即失效。Windows handle duplication必须指向已鉴权进程；Unix SCM_RIGHTS仅用于已批准描述符。移动平台先用系统允许的进程内接口。

NativePresentation不走上述frame pipe；它由同一owner持有MediaSource/窗口，control只选择/关闭呈现。若平台不能安全把它嵌进Tauri窗，单独native window是合法首版，不以屏幕录制假装raw output。

## 8. Rust API边界

按能力拆分接口，禁止一个巨大`InteropAdapter`强迫所有协议实现所有方法。拟议接口集合：DiscoveryProvider、FileSendProvider、FileReceiveProvider、MediaReceiveProvider、MediaSendProvider、MediaControlProvider、PlatformProbe。接口通过trait object/显式registry注册，第一版静态编译注册，不使用不可信native plugin任意dlopen。

每个provider拿`SessionContext`（cancellation、clock、budget、policy、network/content/media ports），不能拿全局`XrossApp`。Rust API采用async任务生命周期返回owned session handle；drop仅best-effort，生产关闭必须显式stop并等待有deadline的资源回收。

`source.read(offset, max_len)`与`sink.write(offset, chunk)`由lease范围限制。未知remote payload offset不能直接当本机seek地址；越界/重复重叠按协议和integrity规则处理。native-onlyprovider不需要实现encoded packet trait。

## 9. 版本与混合运行

主app、interop supervisor、worker可能独立升级。hello必须声明contract版本和profile wire实现版本。N/N−1契约测试固定schema和golden事件；worker不懂请求时明确拒绝。pending传输的重启恢复只恢复本地已持久化授权/文件，不擅自重连一个已经取消的外部会话。
