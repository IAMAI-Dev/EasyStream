# EasyStream 开发规范

**评审日期**　2026-11-19　　**首版范围**　Windows x64 双端　　**状态**　方案评审完成，候选版本待双机联调

## 1. 可行性与首版体验

**建议立项。** 对“两台自己或已获授权使用的 Windows 电脑之间，跨网看桌面、听声音、用键鼠操作和玩游戏”这个目标，三个上游组件已经覆盖了完整的数据通路。EasyTier 提供虚拟 IP 和路由，Sunshine 捕获被控端桌面、编码并接收输入，Moonlight Qt 在控制端解码、播放和回传输入。EasyStream 负责安装、发现、授权、启停和故障提示，无需重写串流协议。跨网成功率、高峰期中继带宽、输入到画面的实际延迟以及无屏兼容性仍需实测。[EasyTier 组网说明](https://doc.easytier.cn/guide/network/networking-without-public-ip.html) · [Sunshine Windows 安装说明](https://github.com/SunshineStream/Sunshine/blob/v2026.914.233613/docs/getting_started.md) · [Moonlight Qt](https://github.com/moonlight-stream/moonlight-qt)

用户在两台电脑安装同一个包，创建或加入房间。被控端主动打开“允许远程控制”。控制端选中设备，第一次连接时在被控端 EasyStream 窗口核对设备并录入控制端显示的短码。以后点击同一设备即可连接。被控端托盘始终能看到被控状态、断开连接和撤销设备。常规流程由 EasyStream 完成，无需用户打开 Sunshine 网页、编辑 IP 或配置家用路由器。

| 交付层次 | 用户可以获得什么 | 成立条件 |
| --- | --- | --- |
| 首版 | 登录 Windows 后双方入网，发现主机，首次授权，已配对设备一键重连，有屏桌面的画面、声音、键鼠与画质选择 | 系统唤醒，交互桌面可捕获，网络直连或有可用中继 |
| 下一阶段 | 登录后自动入网、可信设备无人值守访问、无物理屏串流、手柄及 120 FPS 档位 | 分别通过重启、显示恢复、输入驱动和高刷实机验收 |
| 面向更多用户的服务 | 多地区自有接入和中继、故障切换、可量化的跨运营商连接体验 | 实际部署节点、承担带宽成本、积累峰值测试数据 |

登录前访问、远程唤醒、移动端、文件传输和跨机剪贴板独立评估。用户所说的“替代 UU 远程”在首版按上述桌面与游戏场景验收。

## 2. 技术栈与组件交付

保留 Tauri v2、Rust、React 18、TypeScript、Tailwind CSS 与 Lucide。Rust 管理平台能力，WebView 显示设备和连接状态。媒体流沿“主机 Sunshine ↔ EasyTier 虚拟网络 ↔ 客户端 Moonlight”传输，音视频不经过 Rust 转发或 WebView 渲染。首版允许 Moonlight 打开独立的原生串流窗口。

| 组件及 PoC 候选版本 | Windows 集成方式 | 需在干净系统验证 |
| --- | --- | --- |
| EasyTier core + CLI v2.6.4 | 同版本配套分发，由受限的本机辅助进程管理组网及所需网卡权限 | TUN 依赖、普通用户入网、RPC 对应实例、退网清理。[发行版](https://github.com/EasyTier/EasyTier/releases/tag/v2.6.4) |
| Sunshine v2026.914.233613 | 同一安装入口按需安装官方 x64 MSI，通过 Windows 服务管理器管理本产品安装的实例。候选版本的独立包为 lite，上游首选 MSI | 服务启动方式、用户会话捕获、初始凭据和防火墙。已有独立 Sunshine 时提示冲突并允许仅作控制端，不覆盖配置。[发行包](https://github.com/SunshineStream/Sunshine/releases/tag/v2026.914.233613) · [安装说明](https://github.com/SunshineStream/Sunshine/blob/v2026.914.233613/docs/getting_started.md#windows) |
| Moonlight Qt v6.2.0 | 保留官方 x64 便携包目录、Qt 插件及 DLL，由当前桌面用户启动 | 首次运行、CLI 输出、配对身份保存；用普通账户验证安装目录只读时仍能保存设置并跨版本复用。[发行说明](https://github.com/moonlight-stream/moonlight-qt/releases/tag/v6.2.0) |

上述版本用于建立第一组可复现的兼容性记录。随安装包保存确切 tag、资产文件名、架构、SHA-256、许可证和已通过的场景。每次升级先重跑配对、撤销、串流和旧配置迁移。

**建议目录与责任**

~~~text
src/                         React 设备大厅、首次配对、串流预设、托盘状态
src-tauri/src/commands.rs    有类型的前后端命令
src-tauri/src/core/
  process_mgr.rs             本产品进程、Job Object、服务所有权
  easytier.rs                组网、路由快照、虚拟 IP 变化
  sunshine.rs                本机管理 API、实例初始化、配对与撤销
  moonlight.rs               配对、应用枚举、串流参数与退出状态
  session.rs                 连接状态、超时、取消、并发控制
  display_mgr.rs             可选虚拟屏的检测、启用与还原
src-tauri/binaries/          独立 sidecar，构建输入名带目标三元组
src-tauri/resources/         按上游布局分发的运行目录和第三方声明
src-tauri/capabilities/      前端可用的最小 Tauri 权限
installer/                   MSI 编排、受限权限配置、升级及卸载
~~~

`bundle.externalBin` 用于单个可执行文件，其他资源使用 `bundle.resources` 或安装器部署。前端只调用产品命令。需要提升权限的网卡、服务和防火墙操作经限权本机 IPC 执行，校验调用者、操作枚举及安装路径。WebView 和 Moonlight 以登录用户身份运行。[Tauri 外部二进制](https://v2.tauri.app/develop/sidecar/)

## 3. 入网、发现与网络质量

1. 首次创建房间时生成高熵随机密钥，两端使用同一网络名和密钥。默认使用 DHCP。EasyTier 的 `-d` 表示 DHCP，`--ipv4` 指本节点地址。避免把同一个 `10.144.144.0/24` 当成两端的主机地址。静态模式需为每台设备分配独立地址。检测与本地 LAN、VPN 的重叠路由，提示更换虚拟网段。[EasyTier 配置](https://www.easytier.cn/guide/network/configurations.html)
2. Rust 写入具有限定 ACL 的实例配置，再以配置文件启动 core。房间密钥按用户选择存入 Windows 凭据管理器；运行时必需的配置限制给所属账户及服务，退出后清理临时副本。仅开放当前房间所需的转发，无需子网代理或出口节点。管理 RPC 限于回环地址，并验证连接的实例归属。
3. 用匹配版本的 CLI 读取 JSON。示例 `easytier-cli -p 127.0.0.1:15888 -o json peer`，端口替换为本实例实际端口。结合 `route` 输出确认到目标的实际路径。过滤自身、基础设施节点和缺少虚拟地址的节点。每 2 秒合并变化，未知路径标为“未知”；RTT 缺失显示“—”。虚拟 IP 改变时重新解析目标，清除旧就绪状态。[v2.6.4 CLI 定义](https://github.com/EasyTier/EasyTier/blob/v2.6.4/easytier/src/easytier-cli.rs)
4. 设备列表先展示房间节点。对用户选中的候选节点，通过标准 Sunshine/GameStream 主机信息确认“可串流”，根据配对状态显示“需要授权”或“连接”。节点名和 IP 只用于发现。已信任主机以配对身份核验，身份变化要求重新确认。MVP 无需额外开放远程 PIN 或探活服务。
5. 两端共享同一套可配置的接入节点。开发可用社区共享节点验证发现，跨网验收另准备一个允许转发测试房间流量的自有公网节点。打洞成功走直连，打洞失败且有转发路径时显示“中继”；均不可用时显示失败位置。监测实际路由及端到端质量，分开显示“已进房”“主机可达”“串流可用”。[共享节点组网](https://doc.easytier.cn/guide/network/networking-without-public-ip.html)

**中继预算示例。** 单路 20 Mbit/s 视频持续 1 小时，约产生 9 GB 十进制出站流量，中继还要接收约 9 GB。10 路并发至少需要约 200 Mbit/s 出站能力，再计音频、协议开销和峰值余量。自用可从一台可控节点起步，对外发布前按预期并发、地区和月流量测算。连通成功率和带宽要分开记录。

## 4. 被控端、配对与撤销

**本机初始化。** 默认关闭被控。首次开启时检查 Sunshine 版本、实例及端口归属，配置 `upnp = disabled` 和 `origin_web_ui_allowed = pc`，按该版本支持的初始化接口创建每台机器独有的管理员凭据并安全保存。后端从受保护的本机安装状态取得 Sunshine 证书并固定校验。证书替换时重新确认实例。管理请求由 Rust 发往本机 HTTPS，前端和远端设备均拿不到管理凭据。[Sunshine 配置](https://github.com/SunshineStream/Sunshine/blob/v2026.914.233613/docs/configuration.md) · [管理 API](https://github.com/SunshineStream/Sunshine/blob/v2026.914.233613/docs/api.md)

**网络边界。** 默认管理端口 TCP 47990 仅允许本机访问。串流常用 TCP 47984、47989、48010 和 UDP 47998、47999、48000，仅为所需功能放行到 EasyTier 接口，按配置基准端口及候选包的实际监听复核。47999/UDP 用于串流控制，原稿的 47999/TCP 在协议层可以共存，但没有新设配对服务的必要。MSI 安装后检查已有宽泛放行规则的有效作用域。独立测试物理 LAN 和虚拟网卡的可达性。设置了 Web UI 来源限制后仍须验证媒体端口规则。[Sunshine 端口表](https://docs.lizardbyte.dev/projects/sunshine/v0.23.1/about/advanced_usage.html#port) · [候选版本网络配置](https://github.com/SunshineStream/Sunshine/blob/v2026.914.233613/docs/configuration.md#network)

**首次连接（两端的界面均由 EasyStream 提供）**

1. 控制端选定主机，生成随机四位 PIN（保留前导零），运行 `Moonlight.exe pair <主机虚拟IP> --pin <本次PIN>` 并显示本次短码。将完整命令行从日志和故障报告中排除。保持配对进程运行，等待主机处理。[Moonlight v6.2.0 参数](https://github.com/moonlight-stream/moonlight-qt/blob/v6.2.0/app/cli/commandlineparser.cpp)
2. 被控端通过本机 `GET /api/pin` 读取待配对条目，显示请求名称、来源地址与有效状态。用户选中预期请求，输入控制端显示的短码并确认。对已取消、过期、选项已改变或无法区分的请求要求重新发起。
3. 被控端通过本机 `POST /api/pin` 提交 JSON，字段为所选条目的 `pairing_id`（GET 返回的 `id`）、四位字符串 `pin` 和 `name`。检查业务返回 `status` 和控制端握手结果。拒绝或取消时按同一 ID 调用本机 `DELETE /api/pin`。该版本允许提交请求等待握手完成，超时按阶段反馈。首次配对计时单独记录人工等待。[Sunshine 固定版本的路由和字段](https://github.com/SunshineStream/Sunshine/blob/v2026.914.233613/src/confighttp.cpp#L1604-L1723)
4. 控制端用同一用户配置运行 `Moonlight.exe list <主机虚拟IP> --csv`。列表可读后，选择实际发布的桌面条目。默认尝试匹配 Desktop，名称改变时允许选择并保存。已配对设备直接走此步。保留客户端密钥和主机配对库，重装导致身份改变时回到首次授权。
5. 被控端在“已授权设备”显示 Sunshine 已配对客户端。撤销时按持久客户端 ID 调用本机 `/api/clients/unpair`，同步清除产品缓存。正在串流时按实测行为显式结束本次会话；必要时停止本产品管理的主机服务。验收时同时确认旧会话结束、旧证书不能重连。主机端保留“立即停止被控”入口。[客户端管理接口](https://github.com/SunshineStream/Sunshine/blob/v2026.914.233613/docs/api.md)

## 5. 串流、生命周期与无屏运行

**串流启动。** 由 Rust 以独立 argv 参数启动 Moonlight。示例假设目标确实发布了 Desktop。界面上的“启动中”持续到取得串流证据；如上游无法可靠通知首帧，产品显示“已打开播放器”，首帧由实测记录。[Moonlight CLI 定义与码率单位](https://github.com/moonlight-stream/moonlight-qt/blob/v6.2.0/app/cli/commandlineparser.cpp)

~~~text
Moonlight.exe stream <主机虚拟IP> "Desktop" --resolution 1920x1080 --fps 60 --bitrate 15000 --display-mode fullscreen
~~~

初始预设取 720p30 / 5 Mbps（省流）、1080p60 / 15 Mbps（桌面和普通游戏）、1440p120 / 40 Mbps（高刷候选），实际参数受编码器、解码器、屏幕及链路约束。首轮统一用 H.264 建立基线。中继或丢包明显时提示降档并重连。诊断记录路由类型、RTT、丢帧及可取得的编解码耗时；码率充足但有周期性卡顿时再检查隧道 MTU 和分片。

**生命周期。** 关闭窗口收起到托盘；“退出”结束所拥有的会话、停止本产品开启的被控服务并离开房间。“离开房间”按同样顺序释放本房间资源。首版只允许单个入站串流；重复连接复用进行中的操作，第二个控制端收到“主机忙”。设置并验证 Sunshine 自身的并发行为，避免原生客户端绕过 UI 限制。

| 对象 | 管理者 | 退出、崩溃或断网时的行为 |
| --- | --- | --- |
| EasyStream 启动的普通子进程 | 所属桌面进程或本机辅助进程 | 先正常停止，超时后使用 Job Object 的 `KILL_ON_JOB_CLOSE` 回收。本产品负责的子进程在开始执行前完成关联；绑定失败即终止启动。[Windows Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects) |
| 由本产品安装的 Sunshine 服务 | Windows 服务管理器及本机辅助进程 | 服务按需启动；桌面所有者退出或异常断开后，辅助进程停止服务并核验状态。安装失败、重启或辅助进程重启时按持久化意图收敛到默认关闭。避免按同名进程批量终止。 |
| 断网与恢复 | 会话协调模块 | 报告断开、释放本地播放器，有限退避重新入网；核对主机身份及新地址后提供重连。旧会话是否结束由主机状态确认。 |
| 登录自启、无人值守 | 后续独立功能 | 用户显式开启，限定已授权客户端。区分登录自启、锁屏、注销、Windows 登录前与系统睡眠，逐项记录支持范围。 |

**无屏。** 先用物理屏打通串流。第二阶段在主机开启被控时检查活动显示路径（`QueryDisplayConfig`）和 Sunshine 实际可捕获的输出。在无可捕获输出时，由用户选择受支持且已签名的虚拟显示驱动，记录原拓扑，启用本产品管理的虚拟输出，等待枚举并配置 1080p60，再对外显示“可串流”。这样首个客户端发起时已有画面来源。若驱动支持可靠的会话前后钩子，再优化为会话级启停。验证正常断开、崩溃、物理屏热插拔与重启恢复，清理范围限于本产品创建的显示设备。缺少驱动时保留有屏路径。笔记本合盖须分别测“仍唤醒”和“已睡眠”；后者须先恢复供电运行状态。微软 IddSample 是驱动开发示例，量产驱动另行选型。[Microsoft IddSample 说明](https://learn.microsoft.com/en-us/samples/microsoft/windows-driver-samples/indirect-display-driver-sample/)

## 6. 前后端契约

所有命令异步执行。Rust 持有实际实例和连接状态。前端引用后端给出的对象 ID；重用旧 ID 时重新验证所属房间、身份和在线状态。启动程序名、路径和 argv 均由后端决定。

| Tauri 命令 | 输入 → 结果 | 约定 |
| --- | --- | --- |
| `join_network` / `leave_network` | 房间及密钥 → 房间快照 / 当前房间 → 完成 | 重试幂等；入网成功表示本机网卡、地址及路由已就绪 |
| `toggle_host_service` | enable → 主机状态 | 开启前检查权限、端口、采集设备；关闭时阻止新会话并终止现有会话 |
| `connect_to_peer` / `disconnect` | peerId、画质预设 → operationId / operationId → 完成 | 支持取消待配对和启动中的播放器；丢弃过期操作的回调 |
| `decide_pairing` | requestId、允许及 PIN，或拒绝 → 授权结果 | 仅被控端本机界面可调用；后端验证请求仍有效 |
| `list_trusted_clients` / `revoke_client` | 当前主机 → 客户端列表 / clientId → 撤销结果 | 以 Sunshine 持久授权状态为准 |
| `get_snapshot` | 无 → 房间、设备、主机、会话快照 | 窗口重开、事件丢失后恢复界面 |

设备事件 `network-peers-updated` 包含 roomId、快照序号、peerId、名称、当前虚拟 IP、可空 RTT、路径（直连／中继／未知）及主机就绪状态。配对事件使用临时 requestId，已授权列表使用持久 clientId。会话事件携带 operationId、阶段及可恢复错误码；区分入网失败、无可达路径、未授权、主机忙、应用缺失、编码或采集失败。房间密钥、管理密码、PIN 和证书私钥不得进入诊断包。

## 7. 开发顺序与验收

以下均为拟定验收条件。仓库目前只有开发文档。发布前记录版本、Windows 构建、GPU/驱动、两端网络、路由类型、实际码率和失败原因，保留脱敏日志或截图。

| 阶段 | 开发任务 | 通过标准 |
| --- | --- | --- |
| P0　验证核心链路 | 两台 Windows x64 电脑、物理屏、固定版本，先用原生工具在 LAN 串流，再让两台电脑接入不同网络，经 EasyTier 虚拟 IP 重复。在仅允许中继的受控测试网络再跑一次。 | 三条路径分别留证；画面、声音、键鼠往返可用。首次授权、重启后重连及撤销均有记录。无法直连或中继不可用时给出具体环节。 |
| P1　安装与大厅 | 做安装、权限、实例管理、设备卡片和托盘，打通主机开关。 | 两台干净机器用同一包完成入网。模拟已有 Sunshine、端口占用、无管理员权限和 UI 崩溃，服务状态、配置及已有软件符合预期。 |
| P2　授权与一键连接 | 封装本机配对 API、Moonlight 配对、应用列表及串流；加入会话状态、断开和撤销。 | 首次连接全程在产品界面完成。已配对设备连续重连 10 次；记录每次成功或失败。错码、拒绝、超时、请求并发、同 IP 更换设备和撤销后重连有明确结果。 |
| P3　首版验证 | 在预先记录的硬件和网络条件下，直连和中继各进行 30 分钟 1080p60 测试，再测弱网降档。 | 有声画和输入、无崩溃，记录实际帧率、丢帧、首帧耗时及断线次数。与同条件原生组合比较。联调后用基线确定发布阈值；首次人工授权耗时、网络 RTT 与输入到画面的延迟分别统计。 |
| P4　扩展与分发 | 加入登录自启、无屏、手柄、高刷；补齐发布包签名、组件升级、回滚与卸载。 | 无屏插拔、合盖与休眠、重启复连、120 FPS 实际输出逐项验收。升级保留授权，卸载清理本产品服务及规则；共享驱动按所有权处理。 |

**首个开发任务**是 P0。它能分别回答三个影响立项成本的问题：原生串流在目标显卡上是否流畅，EasyTier 在用户常用的两条宽带之间能否直连，以及直连失败时需要为目标画质购买多少中继带宽。

## 8. 许可与发布

Sunshine 与 Moonlight Qt 的候选版本随附 GPLv3，EasyTier 声明 LGPL-3.0。EasyStream 外壳可以主动选择 GPLv3；同包内独立程序的聚合、紧密集成、链接上游库或修改其代码，对许可的影响要按实际集成方式判断。发布包需保留各组件的版权、许可证和第三方声明；在二进制下载处提供对应版本完整源码的获取方式，包含适用的补丁与构建脚本，并确保持续可取。[Sunshine 许可证](https://github.com/SunshineStream/Sunshine/blob/v2026.914.233613/LICENSE) · [Moonlight 许可证](https://github.com/moonlight-stream/moonlight-qt/blob/v6.2.0/LICENSE) · [EasyTier 许可声明](https://github.com/EasyTier/EasyTier/blob/v2.6.4/README_CN.md) · [GNU 对聚合分发的说明](https://www.gnu.org/licenses/gpl-faq.html#MereAggregation)

对 Qt、编解码库、虚拟网卡、虚拟显示驱动及手柄驱动逐项核实再分发权和签名。Sunshine 候选版本把 Virtual HID Driver 列为可选付费功能，首版基础键鼠可独立验证，游戏手柄与依赖 Raw Input 的应用单独测。每次发布校验安装包与组件哈希，在干净系统完成安装、升级回滚和卸载后再更新兼容性清单。[Sunshine Windows 输入说明](https://github.com/SunshineStream/Sunshine/blob/v2026.914.233613/docs/getting_started.md)