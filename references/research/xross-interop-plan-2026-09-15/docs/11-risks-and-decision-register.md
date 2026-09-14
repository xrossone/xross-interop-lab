# 风险、不可预设事实与设计决议

## 风险清单

| ID | 风险 | 当前决策 | 关闭/重评条件 |
|---|---|---|---|
| RK01 | GPL/LGPL与自有闭源产品不兼容组合 | 每产物选路线；IPC非自动免责；必要法务审查 | 固定源码/链接/IPC/打包方案被审查 |
| RK02 | “AI翻译+私有repo”被误当独立实现 | 保留来源，研究/实现输入管理，不抹历史 | 实际来源/规范/实现得到复核 |
| RK03 | FairPlay/受限认证/SDK材料来源 | 没有合法路线不shipping；可保留合规参考provider | 授权/独立实现/适用法律判断有证据 |
| RK04 | Google Cast通用receiver无法获stock sender认可 | 独立receiver认证gate；先做媒体sender | Pixel/Chrome真实认证+launch+stream均通过 |
| RK05 | WFD受网卡/系统API限制 | 平台provider能力probe；Windows native先行 | 具名硬件/OS的合法用户态路径通过 |
| RK06 | Windows MediaSource不能导出raw frames | NativePresentation作为一等输出 | 明确公开API与测试证明可导出，否则保持仅播放 |
| RK07 | 新鸿蒙不兼容旧EMUI SDK | 标注文档年代，不泛化 | 官方支持表与真机结果 |
| RK08 | OpenHarmony源码被误认为商用普通App SDK | system/privileged/public-app三类部署分开 | 普通App真实权限验证 |
| RK09 | 互传联盟没有完整公开资料 | research/vendor gate，不虚构wire | 获得可使用spec或充分独立观察证据 |
| RK10 | 小项目/AI生成代码缺battle testing | 固定commit、安全扫描、fuzz、真机和隔离 | 选择profile通过实际qualification |
| RK11 | 全协议都开导致端口/radio/性能冲突 | 显式enable、lease调度、单会话默认 | coexistence/rollback测试 |
| RK12 | 文件传输核心重复建设 | T03/T15先映射与复用现有Xross | 主仓回归与唯一history/content authority |
| RK13 | 所有媒体被迫转成H264或统一窗口 | 四类source form，不强制转码 | native/encoded/URL能力准确表达 |
| RK14 | 实验逻辑进入高权限主daemon | worker/host ports，真实sandbox等级 | canary权限测试与依赖图审计 |
| RK15 | 多AI对同一抽象各自扩展 | contract owner + ADR +兼容fixtures | 每个change通过双方consumer测试 |
| RK16 | 1–2万LOC低估兼容性/维护成本 | gate驱动，先baseline再估计；不预言开发天数 | 一条真实闭环数据出现后重新排剩余任务 |
| RK17 | 专业协议扩展吞掉首发范围 | 每profile独立版本/feature，G7选择范围发布 | 额外profile完成而不是绑定整个项目 |
| RK18 | 用户授信被错误跨协议合并 | EndpointObservation与Xross identity分离 | 绑定proof与scope回归 |
| RK19 | Browser/TV/手机无法运行桌面sidecar | embedding/native/system-provider多部署形态 | 对应SDK/后台/分发测试通过 |
| RK20 | 跨协议转发被误称端到端加密 | 明确终止/重新加密、两次授权、hop控制 | UI和协议日志说明真实trust boundary |

## 必须写ADR的变更

新增账户/身份源；改变canonical Offer/Shelf权威；变更IPC major/安全关键枚举；引入新的GPL/厂商SDK到现有产物；放宽自动接收/明文fallback/任意URL；改变radio系统服务；采用native plugin动态加载；把可选provider变默认；移除来源证据；改变媒体格式/clock语义；改变resume/文件commit一致性。

## 接受的工程取舍

第一版native window不如嵌入UI漂亮，但有助于分离协议与renderer故障；独立worker会有部署和IPC成本，但许可/安全/原生API条件通常比单进程纯洁更重要；permissive independent core可能比直接wrapper开发量大，因此先用真实数据决定而不是全面重写。

Rust迁移是可选实施路线，**接口统一是必需目标**。能通过系统API实现的功能不必在Rust里重新写一次协议；不能通过平台API实现的功能不会因为Rust更快就获得权限。
