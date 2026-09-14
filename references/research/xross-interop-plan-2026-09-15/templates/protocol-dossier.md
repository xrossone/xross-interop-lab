# Protocol Dossier — 使用时将标题改为真实profile/角色

状态初始为catalogued。本模板不是协议规范或测试证据。

## A. Scope

填写profile ID、sender/receiver/control角色、应用/系统入口、预期场景、排除场景、目标平台及版本。列出容易混淆的品牌/协议，并说明区别。

## B. Sources and provenance

每项列R/S ID、完整commit/标准版本、阅读路径、许可证及引用用途、允许进入实现的信息范围。未知项标`unverified`并列核对步骤，不能省略来源。

## C. Wire layers

分别写discovery、网络介质、transport、认证/密钥、控制状态机、payload、timing、取消、重试/恢复；每一项事实带来源或capture ID。必要字段未确认时记录具体probe，不猜测。

## D. Message/state map

按方向写当前状态、输入、条件、输出、next state、资源变化、超时/错误。输出字段不复制他人函数结构；由协议事实和独立测试约束。

## E. Platform contract

列公开API、SDK/硬件/权限、进程/UI线程、后台、分发条件、是否会影响网络。native media source能做什么分别记录，不能默认raw export。

## F. Compatibility matrix

写reference baseline、目标设备、OS/ROM/App、profile/codec、网络条件、发现/认证/数据/停止各阶段和结果。自对通不是stock验证。

## G. Security review

列所有未认证输入、长度/深度/timeouts、文件/URL/secret/subprocess/decoder路径、依赖执行入口与实际isolation等级。

## H. Decision

选择reuse library / standalone provider / native API / independent implementation / vendor-gated / blocked；写理由、证据与重评条件。来源审查与功能验证分别签结论。

## I. Implementation input release

列允许交给实现agent的文件/fixtures/hash、禁止材料、未关闭阻塞项、验收cases及负责的task IDs。没有批准记录时不得把本dossier当production-ready spec。
