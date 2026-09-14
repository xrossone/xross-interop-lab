# Reviewed Wire Spec — dlna.upnp-av（M07）

**状态**：review pending。SSDP/SOAP/GENA 骨架为常识级 catalogued 事实；**安全预算参数未定（P-M07-2）前
本文件不作为解析器实现 spec**。

## 已登记的事实入口

| 主题 | 来源 |
|---|---|
| UPnP SDK（BSD-3-Clause，可合规复用候选） | R46 pupnp @2dfc2751 |
| 三角色 SDK（GPL/商业双轨待核，结构 only） | R47 Platinum @2674fa6b |
| DMS/TV profiles（GPL-2.0，事实对照） | R48 gerbera @1dc78d22 |
| GNOME DMR/DMS（LGPL-2.1 为主） | R49 rygel @dc8a47dc |

## 骨架（catalogued）

- SSDP：UDP 239.255.255.250:1900，M-SEARCH/NOTIFY；设备/服务描述 XML 经 HTTP。
- 控制：SOAP over HTTP POST（AVTransport / RenderingControl / ConnectionManager）。
- 事件：GENA SUBSCRIBE + CALLBACK 回调 URL（NAT/多接口行为待证）。
- payload：对端按 URL 主动拉取（本节点有限 media URL lease）。

## 待固化

1. XML/SOAP 解析预算：禁 DTD、深度/大小/时限参数（P-M07-2，结合 docs/07 §4）。
2. 目标 TV protocolInfo 矩阵与控制时序（P-M07-1）。
3. 三角色（DMC/DMR/DMS）能力声明的拆分 schema（CORE-02）。
