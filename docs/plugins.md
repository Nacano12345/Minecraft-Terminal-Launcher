# QCL Plugin System (L1)

## Directory

```text
<root>/plugins/
├── bmclapi/
│   └── plugin.toml      # directory form
├── my-plugin.toml       # single-file form (use either one)
└── state.json           # written by QCL, do not edit
```

Scan rules:

- `<root>/plugins/*/plugin.toml`
- `<root>/plugins/*.toml`
- `state.json` is the enable state, not a plugin

## Manifest

```toml
name = "bmclapi"          # required, unique
version = "1.0.0"         # required
api = 1                   # optional, defaults to 1; > 1 is marked incompatible
description = "BMCLAPI mirrors for mainland China"
author = "QCL Contributors"
homepage = "https://bmclapi2.bangbang93.com"

# ---- download mirrors ----
[[providers]]
kind = "mirror"
priority = 100            # higher is tried first

[providers.match]
host = "libraries.minecraft.net"   # exact match, or
# host_suffix = ".mojang.com"      # suffix match
# path_prefix = "/maven"           # only rewrite paths starting with this

[providers.replace]
with = "https://bmclapi2.bangbang93.com"
path_prefix = "/maven"    # inserted between `with` and the original path

# ---- launch arguments (optional) ----
[launch]
jvm_args = ["-Dqcl.tweaks=1"]      # inserted before -cp
game_args = ["--demo"]             # appended to the game arguments
[launch.env]
QCL_HOOK = "1"                     # merged into the process environment
```

## TUI contributions (L1)

```toml
[tui]
status = "★ {user} · {memory}MB"

[[tui.actions]]
label = "Hello"
key = "h"                    # single-letter shortcut; reserved keys are ignored
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

Status template variables:

| Variable | Value |
|---|---|
| `{user}` | current offline profile name |
| `{memory}` | allocated memory (MB) |
| `{selected}` | currently selected version |
| `{running}` | running version, `-` when idle |
| `{version}` | running version, otherwise the selected one |
| `{plugins}` | number of enabled plugins |

Action types:

| `action` | `value` | Behavior |
|---|---|---|
| `message` | text | write to the status bar and log |
| `url` | URL | `xdg-open` / `open` / `start` |
| `command` | shell command | `sh -c` / `cmd /C` |
| `launch` / `install` / `refresh` | — | launch / install / reload the version list |
| `open-data-dir` | — | open `<root>` |

- All actions are listed under `Esc → Actions`; `Enter` runs the selected one
- `key` binds a single-letter global shortcut (normal mode only)
- Reserved keys: `q j k g i l s r f e / ?` and space
- When multiple plugins bind the same key, the first one wins
- Disabling a plugin removes its status text and actions immediately

> L1 `command` / `url` actions really spawn local processes or open links. Install only trusted plugins. L2 WASM will add a permission sandbox.

## Enable / disable

State lives in `<root>/plugins/state.json`:

```json
{
  "disabled": ["some-plugin"]
}
```

- Everything is enabled by default
- Disabled plugins contribute neither mirrors nor launch hooks
- Invalid plugins (TOML parse error, unsupported `api`, empty `name`, duplicate name) never take effect, but are still listed with an `error`

```bash
qcl plugin list
qcl plugin list --json
qcl plugin info bmclapi
qcl plugin enable bmclapi
qcl plugin disable bmclapi
```

TUI: `Esc` → `Plugins`

```text
Plugins (2/2)
[x] bmclapi      1.0.0    2 mirrors
[x] tweaks       1.0      launch
```

- `↑/↓` selects, `Space` / `Enter` toggles, `Esc` goes back
- `[x]` enabled, `[ ]` disabled, `[!]` invalid
- Mouse: click to select, click again to toggle

## When changes take effect

| Capability | Effect point |
|---|---|
| `providers` mirrors | next download (the manager's mirror table is rebuilt immediately after toggling) |
| `launch` JVM/game args | next LaunchPlan build (the TUI cmdline refreshes immediately) |
| `launch.env` | same as above, written to `LaunchPlan.env` |
| `tui.status` | rendered every frame, immediate |
| `tui.actions` | rebuilt after toggling; shortcuts are immediate |

## Invalid plugins

- Still appear in `qcl plugin list` and the TUI list
- Shown in red with `error: ...`
- Cannot be enabled
- Never affect launching

## Security boundary

L1 is purely declarative and executes no code. Plugins can only:

- Rewrite download URLs
- Append JVM / game arguments
- Inject environment variables
- Add TUI status-bar text
- Register TUI actions and shortcuts (`message` / `url` / `command` / built-in launcher actions)

L2 (WASM) plan:

- Executed inside a `wasmtime`/Extism sandbox
- Permissions: `net:<host>`, `fs:read:<glob>`, `fs:write:<glob>`, `process:spawn`, `auth:token`
- Hooks: `on_launch_plan`, `on_log_line`, `provide_mirror`, `provide_auth`, `register_command`
- Reuses the same registry, enable state and UI

## Next steps

1. L2 WASM host + permission model
2. Plugin install/update (Modrinth-style source verification)
3. Plugin configuration pages (manifest-declared schema, auto-generated TUI forms)
