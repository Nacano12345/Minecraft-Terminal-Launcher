# QCL 插件系统（L1）

## 目录

```text
<root>/plugins/
├── bmclapi/
│   └── plugin.toml      # 目录形式
├── my-plugin.toml       # 单文件形式（二选一）
└── state.json           # 由 QCL 写入，不要手改
```

扫描规则：

- `<root>/plugins/*/plugin.toml`
- `<root>/plugins/*.toml`
- `state.json` 是启用状态，不是插件

## 清单格式

```toml
name = "bmclapi"          # 必填，唯一
version = "1.0.0"         # 必填
api = 1                   # 可选，默认 1；大于 1 会被标记为不兼容
description = "BMCLAPI mirrors for mainland China"
author = "QCL Contributors"
homepage = "https://bmclapi2.bangbang93.com"

# ---- 下载镜像 ----
[[providers]]
kind = "mirror"
priority = 100            # 越大越先尝试

[providers.match]
host = "libraries.minecraft.net"   # 精确匹配，或
# host_suffix = ".mojang.com"      # 后缀匹配
# path_prefix = "/maven"           # 只改写以它开头的路径

[providers.replace]
with = "https://bmclapi2.bangbang93.com"
path_prefix = "/maven"    # 拼在 with 和原 path 之间

# ---- 启动参数（可选）----
[launch]
jvm_args = ["-Dqcl.tweaks=1"]      # 插入到 -cp 之前
game_args = ["--demo"]             # 追加到游戏参数末尾
[launch.env]
QCL_HOOK = "1"                     # 合并进进程环境变量
```

## TUI 贡献（L1）

```toml
[tui]
status = "★ {user} · {memory}MB"

[[tui.actions]]
label = "Hello"
key = "h"                    # 单字母快捷键，保留键会被忽略
action = "message"
value = "hi from plugin"

[[tui.actions]]
label = "Open data folder"
key = "o"
action = "open-data-dir"

[[tui.actions]]
label = "Refresh versions"
action = "refresh"
```

状态栏模板变量：

| 变量 | 值 |
|---|---|
| `{user}` | 当前离线档案名 |
| `{memory}` | 分配内存（MB） |
| `{selected}` | 当前选中的版本 |
| `{running}` | 正在运行的版本，未运行为 `-` |
| `{version}` | 运行中版本，否则选中版本 |
| `{plugins}` | 启用插件数 |

动作类型：

| `action` | `value` | 行为 |
|---|---|---|
| `message` | 文本 | 写状态栏 + 日志 |
| `url` | URL | `xdg-open` / `open` / `start` |
| `command` | shell 命令 | `sh -c` / `cmd /C` |
| `launch` / `install` / `refresh` | — | 启动 / 安装 / 刷新版本列表 |
| `open-data-dir` | — | 打开 `<root>` |

- 所有动作汇总在 `Esc → Actions`，`Enter` 执行
- `key` 绑定单字母全局快捷键（普通模式下生效）
- 保留键：`q j k g i l s r f e / ? 空格`
- 多个插件绑定同一键时先到先得
- 停用插件后，状态栏文字和动作立即消失

> L1 的 `command` / `url` 会真实执行本地进程或打开链接，请只安装可信插件；L2 WASM 会加权限沙箱。

## 启用 / 停用

状态保存在 `<root>/plugins/state.json`：

```json
{
  "disabled": ["some-plugin"]
}
```

- 默认全部启用
- 停用的插件不贡献镜像，也不贡献 launch hook
- 无效插件（TOML 解析失败、`api` 过新、`name` 为空、重名）永远不生效，
  但在列表里显示 `error`

```bash
qcl plugin list
qcl plugin list --json
qcl plugin info bmclapi
qcl plugin enable bmclapi
qcl plugin disable bmclapi
```

TUI：`Esc` → `Plugins`

```text
Plugins (2/2)
[x] bmclapi      1.0.0    2 mirrors
[x] tweaks       1.0      launch
```

- `↑/↓` 选择，`Space` / `Enter` 切换，`Esc` 返回
- `[x]` 启用，`[ ]` 停用，`[!]` 无效
- 鼠标点击选中，再点一次切换

## 生效时机

| 能力 | 生效点 |
|---|---|
| `providers` 镜像 | 下一次下载（Manager 的镜像表在切换后立即重建） |
| `launch` JVM/游戏参数 | 下一次构建 LaunchPlan（TUI 左下 cmdline 会立即刷新） |
| `launch.env` | 同上，写入 `LaunchPlan.env` |
| `tui.status` | 每帧渲染，立即生效 |
| `tui.actions` | 切换插件后重建；快捷键立即生效 |

## 无效插件的表现

- 仍然出现在 `qcl plugin list` / TUI 列表
- 标红并显示 `error: ...`
- 不能被启用
- 不会影响启动

## 安全边界

L1 是纯声明式，不执行代码，插件能做的只有：

- 改写下载 URL
- 追加 JVM / 游戏参数
- 注入环境变量
- 在 TUI 状态栏追加文字
- 注册 TUI 菜单动作与快捷键（`message` / `url` / `command` / 内置启动器动作）

L2（WASM）计划：

- 由 `wasmtime`/Extism 沙箱执行
- 权限：`net:<host>`、`fs:read:<glob>`、`fs:write:<glob>`、`process:spawn`、`auth:token`
- 钩子：`on_launch_plan`、`on_log_line`、`provide_mirror`、`provide_auth`、`register_command`
- 复用同一套注册表、启停状态和 UI

## 下一步

1. L2 WASM 宿主 + 权限模型
2. 插件安装/更新（Modrinth 风格来源校验）
3. 插件配置页（manifest 声明 schema，TUI 自动生成表单）
