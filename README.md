# QuarkCraftLauncher (QCL)

终端优先的 Minecraft 启动器。三栏 TUI、可编辑启动命令、插件系统，单文件分发。

[![CI](https://github.com/Nacano12345/Minecraft-Terminal-Launcher/actions/workflows/ci.yml/badge.svg)](https://github.com/Nacano12345/Minecraft-Terminal-Launcher/actions/workflows/ci.yml)

- 仓库：<https://github.com/Nacano12345/Minecraft-Terminal-Launcher>
- 状态：v0.0_Alpha · M0–M3 完成，插件 L1 完成
- 技术栈：Rust 2021 · tokio · reqwest · ratatui · 单二进制（release 约 5 MB）

---

## 构建要求

- Rust stable（`rust-toolchain.toml` 已固定）
- JDK 17+（1.20.1 需要 17，1.20.5+ 需要 21；目前只检测，自动下载在计划中）
- Linux / macOS / Windows

---

## 快速开始

```bash
cargo build --release

# 看版本
./target/release/qcl list --limit 10

# 下载（客户端 jar + 依赖 + assets）
./target/release/qcl install 1.20.1

# 只看启动命令，不真的启动
./target/release/qcl run 1.20.1 --dry-run

# 离线模式启动
./target/release/qcl run 1.20.1 --username Steve --memory 4096

# 机器可读事件流
./target/release/qcl run 1.20.1 --json | jq .

# 终端界面
./target/release/qcl tui --username Steve --memory 4096
```

### TUI 快捷键

| 按键 | 作用 |
|---|---|
| `j` / `k` / `↑` / `↓` | 选择版本 |
| `g` / `G` | 第一个 / 最后一个 |
| `/` | 过滤版本（`Esc` 清空） |
| `Enter` / `l` | 启动选中版本（自动补全依赖） |
| `i` | 只安装不启动 |
| `Tab` / `e` | 聚焦左下角可编辑的 cmdline |
| 编辑中 | 直接输入；`Enter` 运行这条命令；`Esc` 返回 |
| `Ctrl+R` | 按当前版本重建 cmdline |
| `Ctrl+U` | 清空 cmdline |
| `s` | 停止当前游戏 |
| `PgUp` / `PgDn` / `f` | 日志翻页 / 回到尾部 |
| `r` | 重新拉取版本列表 |
| `?` | 帮助 |
| `Esc` | 打开 QCL 顶层菜单 |
| `q` / `Ctrl+C` | 退出（会结束游戏进程） |

> TUI 模式下 tracing 日志写到 `<root>/logs/qcl.log`（stderr 是备用屏，直接输出会把界面冲花）。

### 三栏布局

```
┌ VersionTable ──────────────┐┌ logs ────────────────────────────────────┐
│ 26.3                       ││ [10:31:07] [Render thread/INFO]: ...      │
│ 26.2                       ││                                          │
├ cmdline ───────────────────┤│                                          │
│ /usr/bin/java -Xmx2048M …  ││                                          │
└────────────────────────────┘└──────────────────────────────────────────┘
```

- 左上 `VersionTable`：版本列表（`/` 过滤，鼠标/键盘选择）
- 左下 `cmdline`：选中版本自动生成的启动命令，不触发下载；可编辑
- 右侧 `logs`：实时日志

编辑后的命令会先补齐选中版本的依赖，再按你改写的 argv 直接 spawn；`Enter` 运行，`Esc` 退出编辑。

### 顶层菜单

`Esc`（或点底栏 `[Esc] menu`）弹出居中菜单，包含 ASCII QCL logo 和三个选项：

```
   ██████   ██████  ██
  ██    ██ ██       ██
  ██    ██ ██       ██
  ██  ▄ ██ ██       ██
   ██████   ██████  ███████
        QuarkCraftLauncher
            v0.0_Alpha
        ▸ Options
          Plugins
          Actions
          Exit
```

打开菜单时底层界面会整体压暗（`DIM` + 前景/背景降色），菜单本身保持原亮度。

- `↑/↓` 或滚轮移动，`Enter` 进入，`Esc` 返回/关闭
- `Options`：编辑用户名、内存、Java 路径（保存到 `<root>/settings.json`）
- `Plugins`：列出插件并开关（`[x]` / `[ ]` / `[!]`），Space/Enter 切换
- `Exit`：退出（会结束游戏进程）

### 最小终端尺寸

安全尺寸为 **80 x 24**（`crates/mc-tui/src/ui.rs` 里的 `MIN_WIDTH` / `MIN_HEIGHT`）。小于该尺寸时不绘制三栏界面，只在终端中央显示：

```text
        Terminal too small

        minimum: 80 x 24
        current: 60 x 20

  Resize the window to continue.
```

- 尺寸实时检测，拖大到 80x24 后立即恢复正常界面，无需重启
- 过小时只响应 `q` / `Ctrl+C`，其他按键和鼠标都不会触发启动/安装
- 缩小到最小尺寸以下会立即切回提示文字
| 鼠标左键 | 选中版本；双击直接启动 |
| 滚轮 | 在列表上滚动选择，在日志上翻页 |
| 点击日志区 | 回到日志尾部（恢复跟随） |
| 点击底栏按钮 | `launch` / `install` / `filter` / `stop` / `help` / `quit` 均可点（右对齐固定位置） |
| `Shift` + 拖动 | 终端原生文本选择（鼠标上报会占用普通拖动） |

数据目录优先级：`--root` > `QCL_HOME` > `~/.local/share/qcl`（Linux）/ `%APPDATA%\QCL`（Windows）。

Java 查找顺序：`--java` > `QCL_JAVA` > `JAVA_HOME` > `PATH` > `<root>/java/*`。

---

## 定位

- **显示层**：cmd.exe / bash / zsh / Windows Terminal，不依赖 GUI 框架
- **游戏本体**：客户端仍然是 OpenGL 窗口，终端承载启动器 UI + 日志 + 控制台
- **服务端**：完整终端体验（日志、命令输入、看门狗）
- **非交互**：所有能力都要有 `--json` 机器可读模式，能被脚本/CI 调用

---

## 四模块划分

```
┌─────────────────────────────────────────────────────┐
│  TUI / CLI  (前台, 可替换)                           │
│  全屏界面  |  纯行模式  |  --json 机器模式            │
└───────────────┬─────────────────────────────────────┘
                │  JSON-RPC 2.0 (stdio 或 unix socket)
┌───────────────▼─────────────────────────────────────┐
│  Manager  (常驻 daemon, 唯一真相源)                  │
│  版本解析 · 下载器 · 实例库 · Java运行时 · 认证       │
│  配置 · LaunchPlan 组装 · 插件宿主                   │
│         │                    ▲                      │
│         │ spawn + LaunchPlan │ 事件流(日志/状态/退出) │
│  ┌──────▼──────┐      ┌──────┴──────┐               │
│  │  Wrapper A  │      │  Wrapper B  │  (每实例一个)  │
│  └──────┬──────┘      └──────┬──────┘               │
│         ▼                    ▼                      │
│      java 进程            java 进程                  │
└─────────────────────────────────────────────────────┘
```

### Wrapper —— java 进程宿主

Manager 决定「启动什么」，Wrapper 决定「怎么启动、怎么活着、怎么死」。

- 进程创建（绕过 shell；Windows 超长命令行走 Java 9+ `@argfile`）
- 注入点：`-javaagent:authlib-injector`、`-Dlog4j.configurationFile`、`-Dfile.encoding`、分辨率、quickPlay 参数
- IO 桥接：stdout/stderr 按行切分 → 时间戳 → 分级 → 上报；接受命令写入 java stdin
- 崩溃判定：退出码、`hs_err_pid*.log`、`crash-reports/`
- 生命周期：优雅停止 → SIGTERM → kill；Windows Job Object / Linux 进程组
- 模式：`pipe`（客户端）/ `pty`（服务端）

### Manager —— 唯一真相源

- 版本元数据缓存（`version_manifest_v2.json`，ETag 条件请求）
- 下载器：并发、续传、SHA1 校验、镜像回退（官方 → BMCLAPI）
- 实例注册表：独立 gameDir / JVM 参数 / 内存 / Java 路径
- Java 运行时：系统 JDK 发现 + Mojang JRE 自动下载
- 认证：offline / Microsoft Device Code / authlib-injector
- 组装 `LaunchPlan`（纯数据，支持 dry-run）
- 插件宿主

### TUI / CLI —— 可替换的壳

- 一套命令解析，三种渲染：全屏 TUI / 纯行模式 / `--json`
- 非交互模式必须完整，可被脚本调用

### Plugin —— 两级设计

| 级别 | 形式 | 状态 |
|---|---|---|
| L1 声明式 | `*.toml`：下载镜像 + launch hook + TUI 贡献 | ✅ |
| L2 逻辑 | WASM（沙箱） | ⬜ |

L1 已实现：

- 扫描 `<root>/plugins/*/plugin.toml` 或 `<root>/plugins/*.toml`
- `[[providers]]` 镜像改写，`[launch]` 追加 JVM/游戏参数和环境变量
- `[tui]` 追加状态栏文字，注册 Actions 菜单项和单键快捷键
- 启用状态存 `<root>/plugins/state.json`
- CLI `qcl plugin list / info / enable / disable`
- TUI `Esc → Plugins` 用 Space/Enter 切换
- 无效清单（TOML 错误、api 过新、重名）会显示但不会生效

清单示例与完整字段见 [`docs/plugins.md`](docs/plugins.md)。

L2 权限预览：`fs:read:<glob>` / `fs:write:<glob>` / `net:<host>` / `process:spawn` / `auth:token`

---

## 目录结构

```
QCL/
├── Cargo.toml              # workspace
├── crates/
│   ├── mc-core/            # 纯数据模型: LaunchPlan, Instance, LogLine, CrashInfo
│   ├── mc-ipc/             # JSON-RPC 2.0 类型 + 事件 + 行帧 codec
│   ├── mc-plugin/          # L1 插件：清单/注册表/启停/镜像/launch hook
│   ├── mc-manager/         # manifest / version / rules / download / java / auth / install / launch
│   ├── mc-wrapper/         # 进程宿主: spawn、stdio 泵、日志分级、崩溃检测
│   ├── mc-tui/             # ratatui 界面：版本列表 / 过滤 / 进度 / 日志 / 帮助
│   └── mc-cli/             # bin `qcl`
├── plugins/
│   └── bmclapi/plugin.toml # 示例镜像 provider
└── docs/
    └── protocol.md         # IPC 协议定义
```

### 当前实现状态

| 模块 | 状态 |
|---|---|
| 版本清单拉取 + 磁盘缓存（离线回退） | ✅ |
| version.json 解析、rules 求值、Maven 坐标 | ✅ |
| 并发下载 + SHA-1 校验 + 原子写入 + 镜像回退 | ✅ |
| client jar / libraries / natives 解压 / assets / log4j 配置 | ✅ |
| log4j 控制台 XML → 纯文本改写（否则日志是一堆 `<log4j:Event>`） | ✅ |
| Java 发现（启动器内自动下载 JRE） | 发现 ✅ / 下载 ⬜ |
| 离线 Profile（`OfflinePlayer:` MD5 UUID） | ✅ |
| LaunchPlan 组装（参数模板、legacy 兼容、`--dry-run`） | ✅ |
| Wrapper：spawn、stdout/stderr 分流、日志分级、崩溃识别 | ✅ |
| JSON-RPC 事件流（`--json`） | 类型 ✅ / daemon ⬜ |
| Microsoft 登录（Device Code） | ⬜ M4 |
| Mod Loader（Fabric/Forge/NeoForge） | ⬜ |
| 插件注册表 + 启停（`plugins/state.json`） | ✅ |
| L1 镜像 provider（BMCLAPI 示例） | ✅ |
| L1 launch hook（JVM/游戏参数 + env） | ✅ |
| L1 TUI 贡献（状态栏文字 / Actions / 快捷键） | ✅ |
| L2 WASM 插件 | ⬜ |
| 多实例注册表 | 读写 ✅ / CLI ⬜ |
| TUI 全屏界面 | ✅ 三栏（VersionTable / 可编辑 cmdline / logs）/ 过滤 / 进度 / 鼠标 |
| 服务端 PTY + 控制台 | ⬜ M6 |

---

## 里程碑

| 阶段 | 目标 | 状态 |
|---|---|---|
| M0 · headless | `qcl run <version>` 组装命令并拉起 java，日志直出 | ✅ |
| M1 · 协议 | 事件流改成 JSON-RPC 行帧，`--json` 可用 | 🟡 CLI 已可用，daemon/attach 待做 |
| M2 · CLI | `list / install / run / attach / kill / instance` | 🟡 前三个可用 |
| M3 · TUI | 全屏界面：版本列表、下载进度、日志窗格 | ✅ |
| M4 · 认证 | Microsoft Device Code，终端内完成 | ⬜ |
| M5 · 插件 | L1 注册表/启停/镜像/launch hook → L2 WASM | 🟡 L1 完成 |
| M6 · 服务端 | PTY + 控制台 + 看门狗 | ⬜ |

---

## 已验证（本机 smoke test · 2026-10-31）

| 项 | 结果 |
|---|---|
| `qcl list --limit 5` | ✅ 拉到 26.3 / 26.2 / 26.1.2 / 26.1.1 / 26.1 |
| `qcl install 1.20.1` | ✅ 53 个库 + 3598 个 assets；BMCLAPI 镜像失败时自动回退官方源 |
| `qcl run 1.20.1 --dry-run` | ✅ 生成完整 JVM + 游戏参数；JDK 21 版本检查通过 |
| `qcl run 1.20.1`（真机） | ✅ 10 秒进主菜单：LWJGL 3.3.1、OpenAL、`Sound engine started` |
| `qcl tui`（PTY 内） | ✅ 过滤 `1.20.1` → Enter 启动 → 主菜单 → PgUp 翻日志 → `s` 停止 → 退出码 0 |
| TUI 鼠标 | ✅ 点击第 5 行 → `selected 26.1.1`；滚轮/帮助点击无 panic；`1006h` 鼠标上报已开启 |
| 插件 CLI | ✅ `plugin list/info/enable/disable`，`state.json` 持久化 |
| 插件 launch hook | ✅ 启用后 dry-run 出现 `-Dqcl.tweaks=1` / `--demo` / `QCL_HOOK=1`，禁用后消失 |
| TUI Plugins | ✅ `[x]` → Space → `[ ]`，状态写入 state，LaunchPlan 重建 |
| TUI 插件贡献 | ✅ 状态栏出现 `★ Steve`；按 `h` 执行 message；`Esc → Actions` 列出动作；停用后立即消失 |
| Wrapper 假 java 测试 | ✅ stdout/stderr 分流、INFO/WARN/ERROR 分级、退出码、崩溃识别 |
| `--json` 行帧 | ✅ 6 行事件全部可被 `json.loads` 逐行解析 |
| `cargo test --workspace` | ✅ 21 passed |
| `cargo clippy --workspace --all-targets` | ✅ 无警告 |

---

## 已知限制

- 真机验证只在 Linux/Wayland + AMD 780M + JDK 21 上做过；Windows/macOS 未测
- 离线模式下 authlib 会打一条 401 `player/attributes` 堆栈，属预期噪音
- JDK 21 下 LWJGL 会提示 `Unsupported JNI version`，官方推荐 17，功能不受影响
- 全量 assets 首次下载约 3600 个对象，耗时取决于镜像
- 没有 daemon/attach：IPC 类型和协议已定义，CLI 仍是单进程直接 spawn
- 没有 Microsoft 登录、Mod Loader、服务端 PTY
- TUI 还没有实例（多开隔离）概念，只能按版本启动；目前同时只跑一个游戏
- 终端被 SIGKILL / 直接关闭时，子 java 可能变孤儿；需要 pdeathsig/进程组（随 Windows 兼容一起做）
- L2 WASM 插件、Java 运行时自动下载未实现

---

## 关键约束

- 不分发 Mojang 的 jar/assets
- 不实现盗版登录接口；离线模式仅限本地/自用
- 微软登录需自建 Azure 应用 client_id
- 一切跨平台逻辑不经 shell，避免引号地狱

## License

GPL-3.0-or-later，见 [LICENSE](LICENSE)。
