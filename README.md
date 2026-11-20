# EasyStream

**EasyStream** 是一套面向 Windows x64 双端的开源跨网远程桌面与游戏串流整合外壳（替代 UU 远程），基于 **Tauri v2 + Rust + React 18 + TypeScript + Tailwind CSS** 构建，统一编排以下三大上游组件：

- **EasyTier (`v2.6.4`, LGPL-3.0)**：提供去中心化虚拟 IP 组网、P2P 打洞与中继转发。
- **Sunshine (`v2026.914.233613`, GPL-3.0)**：负责被控端桌面捕获、硬件编码与输入注入。
- **Moonlight Qt (`v6.2.0`, GPL-3.0)**：负责控制端硬解码播放与键鼠/手柄低延迟回传。

媒体流全程沿 **主机 Sunshine ↔ EasyTier 虚拟网络 ↔ 客户端 Moonlight** 直连传输，音视频不经过 Rust 转发或 WebView 渲染。

---

## 核心特性

1. **零公网 IP 跨网组网**
   - 随机生成高熵房间名与密钥（可选存入 Windows 凭据管理器），两端加入同一房间即可发现彼此。
   - 自动检测虚拟网段（`10.144.144.0/24`）与本机物理网卡 / 已有 VPN 的路由重叠并提示切换静态网段。
   - 每 2 秒轮询 `easytier-cli -o json peer / route`，过滤基础设施节点与无效地址，明确区分 **直连 P2P / 中继转发 / 未知路径** 及 **已进房 / 主机可达 / 串流可用** 状态。
2. **安全被控端与界面内首次配对**
   - 默认关闭被控；开启时自动配置 `upnp = disabled`、`origin_web_ui_allowed = pc`，并生成每台机器独立的管理员凭据存入 Windows 凭据管理器。
   - 若本机已安装非 EasyStream 管理的独立 Sunshine，默认提示冲突并仅作控制端运行，绝不覆盖用户现有配置。
   - 首次连接由控制端生成 4 位随机短码（保留前导零），被控端在 EasyStream 窗口内直接核对来源与设备名并录入短码完成配对（基于本机 `GET / POST / DELETE /api/pin`）。
   - 支持在“已授权设备”中按持久 UUID 撤销设备并立即结束当前会话。
3. **预设串流与强生命周期管理**
   - 内置三档预设：省流 `720p30 (5 Mbps)`、标准 `1080p60 (15 Mbps)`、高刷候选 `1440p120 (40 Mbps)`，首轮默认 `H.264` 基线。
   - 所有启动的子进程均在恢复执行前绑定至 Windows Job Object (`KILL_ON_JOB_CLOSE`)，关闭窗口自动收起至系统托盘，退出时按序断开会话、停止受管被控服务并离开房间。
   - 支持导出脱敏诊断包，自动擦除房间密钥、管理密码、配对 PIN 与私钥。

---

## 开发环境构建

### Windows 安装包

两台电脑使用同一个 Windows x64 安装包。安装时会准备 WebView2 及两端所需的 EasyTier、Wintun 和 Moonlight。加入房间后，只有第一次点击“允许远程控制”的电脑会从包内安装 Sunshine。安装版启动时会显示 Windows 管理员权限确认。最终用户只需下载 EasyStream 安装包。

开发机装好下方工具后，运行 `pnpm install --frozen-lockfile --ignore-scripts`、`pnpm build:windows`。构建自动下载并校验固定版本的上游资源，生成 `src-tauri/target/release/bundle/nsis/` 下的安装包。也可以通过 GitHub Actions 的 **Windows x64 installer** 工作流构建。

### 前置依赖
- Node.js 20+ & `pnpm`
- Rust stable toolchain (`rustc` / `cargo`)
- Windows 10/11 x64（含 WebView2 Runtime）

### 1. 安装前端依赖
```powershell
pnpm install
```

### 2. 拉取并校验上游固定版本组件
```powershell
.\scripts\fetch-upstream.ps1
```
该脚本会从 GitHub Releases 下载并校验 SHA-256 后部署至 `src-tauri/resources/`：
- `resources/easytier/easytier-core.exe` & `easytier-cli.exe`
- `resources/sunshine/sunshine-windows-installer.msi`
- `resources/moonlight/Moonlight.exe`（完整便携目录）

### 3. 本地开发与打包
```powershell
# 前端类型检查与构建
pnpm build

# 启动 Tauri 开发模式
pnpm tauri dev

# 构建 Windows x64 NSIS 安装包
pnpm build:windows
```

---

## 开源许可

- EasyStream 本体采用 **GNU General Public License v3.0 (GPL-3.0-or-later)**，完整协议文本见 [LICENSE](./LICENSE)。
- 第三方组件版本、许可证与对应源码获取说明见 [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md)。
- 详细架构与 P0–P4 实机验收规范见 [easystream.md](./easystream.md)。
