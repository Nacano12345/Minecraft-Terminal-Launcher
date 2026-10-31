# QuarkCraftLauncher (QCL)

A terminal-first Minecraft launcher. Three-pane TUI, editable launch command, plugin system, single-binary distribution.

[![CI](https://github.com/Nacano12345/Minecraft-Terminal-Launcher/actions/workflows/ci.yml/badge.svg)](https://github.com/Nacano12345/Minecraft-Terminal-Launcher/actions/workflows/ci.yml)

- Repository: <https://github.com/Nacano12345/Minecraft-Terminal-Launcher>
- Status: v0.0_Alpha · M0–M3 complete, plugin L1 complete
- Stack: Rust 2021 · tokio · reqwest · ratatui · single binary (release ~5 MB)

---

## Requirements

- Rust stable (pinned by `rust-toolchain.toml`)
- JDK 17+ (17 for 1.20.1, 21 for 1.20.5+; currently detected only, auto-download planned)
- Linux / macOS / Windows

---

## Quick start

```bash
cargo build --release

# List versions
./target/release/qcl list --limit 10

# Download (client jar + libraries + assets)
./target/release/qcl install 1.20.1

# Print the launch command without starting the game
./target/release/qcl run 1.20.1 --dry-run

# Offline launch
./target/release/qcl run 1.20.1 --username Steve --memory 4096

# Machine-readable event stream
./target/release/qcl run 1.20.1 --json | jq .

# Terminal UI
./target/release/qcl tui --username Steve --memory 4096
```

### TUI keys

| Key | Action |
|---|---|
| `j` / `k` / `↑` / `↓` | Move selection |
| `g` / `G` | First / last |
| `/` | Filter versions (`Esc` clears) |
| `Enter` / `l` | Launch selected version (installs missing files) |
| `i` | Install without launching |
| `Tab` / `e` | Focus the editable cmdline pane |
| Editing | Type to edit; `Enter` runs the command; `Esc` leaves |
| `Ctrl+R` | Rebuild the cmdline for the selected version |
| `Ctrl+U` | Clear the cmdline |
| `s` | Stop the running game |
| `PgUp` / `PgDn` / `f` | Scroll logs / follow tail |
| `r` | Reload the version list |
| `?` | Help |
| `Esc` | Open the QCL top-level menu |
| `q` / `Ctrl+C` | Quit (stops the game process) |
| Left click | Select a version; double-click to launch |
| Wheel | Scroll the version list or logs |
| Click log pane | Resume following the tail |
| Click bottom bar | `launch` / `install` / `filter` / `stop` / `help` / `quit` (fixed right-aligned positions) |
| `Shift` + drag | Native terminal text selection (mouse reporting captures plain drag) |

> In TUI mode, tracing logs are written to `<root>/logs/qcl.log`. stderr is the alternate screen, so writing there would corrupt the UI.

### Three-pane layout

```
┌ VersionTable ──────────────┐┌ logs ────────────────────────────────────┐
│ 26.3                       ││ [10:31:07] [Render thread/INFO]: ...      │
│ 26.2                       ││                                          │
├ cmdline ───────────────────┤│                                          │
│ /usr/bin/java -Xmx2048M …  ││                                          │
└────────────────────────────┘└──────────────────────────────────────────┘
```

- **VersionTable** (top left): version list; `/` filters, mouse or keyboard selects
- **cmdline** (bottom left): launch command generated for the selected version, no download triggered; editable
- **logs** (right): live game output

An edited command installs the selected version's dependencies first, then spawns your argv verbatim. `Enter` runs it; `Esc` leaves editing.

### Top-level menu

`Esc` (or the `[Esc] menu` button in the bottom bar) opens a centered menu with the ASCII QCL logo and four items:

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

Opening the menu dims the background (`DIM` plus darker foreground/background colors); the menu itself stays at full brightness.

- `↑/↓` or the wheel moves, `Enter` opens, `Esc` goes back/closes
- **Options**: edit username, memory and Java path (saved to `<root>/settings.json`)
- **Plugins**: list plugins and toggle them (`[x]` / `[ ]` / `[!]`); Space/Enter switches
- **Actions**: plugin-provided actions
- **Exit**: quit (stops the game process)

### Minimum terminal size

The safe minimum is **80 x 24** (`MIN_WIDTH` / `MIN_HEIGHT` in `crates/mc-tui/src/ui.rs`). Below that, the three-pane UI is not drawn; a centered message is shown instead:

```text
        Terminal too small

        minimum: 80 x 24
        current: 60 x 20

  Resize the window to continue.
```

- The size is checked every frame; resizing back to 80x24 restores the normal UI immediately, no restart needed
- While too small, only `q` / `Ctrl+C` are accepted; other keys and the mouse cannot trigger installs or launches
- Shrinking below the minimum switches back to the message immediately

Data directory precedence: `--root` > `QCL_HOME` > `~/.local/share/qcl` (Linux) / `%APPDATA%\QCL` (Windows).

Java lookup order: `--java` > `QCL_JAVA` > `JAVA_HOME` > `PATH` > `<root>/java/*`.

---

## Positioning

- **Display layer**: cmd.exe / bash / zsh / Windows Terminal; no GUI framework
- **Game client**: still an OpenGL window; the terminal hosts the launcher UI, logs and console
- **Server**: full terminal experience (logs, console input, watchdog)
- **Non-interactive**: every capability has a `--json` machine-readable mode for scripts and CI

---

## Architecture

```
┌─────────────────────────────────────────────────────┐
│  TUI / CLI  (frontend, replaceable)                  │
│  full-screen  |  line mode  |  --json                │
└───────────────┬─────────────────────────────────────┘
                │  JSON-RPC 2.0 (stdio or unix socket)
┌───────────────▼─────────────────────────────────────┐
│  Manager  (daemon, single source of truth)            │
│  versions · downloads · instances · Java · auth       │
│  config · LaunchPlan · plugin host                    │
│         │                    ▲                       │
│         │ spawn + plan       │ events (log/state/exit)│
│  ┌──────▼──────┐      ┌──────┴──────┐                │
│  │  Wrapper A  │      │  Wrapper B  │  (one per game)│
│  └──────┬──────┘      └──────┬──────┘                │
│         ▼                    ▼                       │
│      java process        java process                │
└─────────────────────────────────────────────────────┘
```

### Wrapper — java process host

The manager decides *what* to launch; the wrapper decides *how it starts, lives and dies*.

- Process creation (bypasses the shell; on Windows, long command lines use Java 9+ `@argfile`)
- Injection points: `-javaagent:authlib-injector`, `-Dlog4j.configurationFile`, `-Dfile.encoding`, resolution, quickPlay arguments
- IO bridge: split stdout/stderr by line → timestamp → classify → report; write commands to java stdin
- Crash detection: exit code, `hs_err_pid*.log`, `crash-reports/`
- Lifecycle: graceful stop → SIGTERM → kill; Windows Job Object / Linux process groups
- Modes: `pipe` (client) / `pty` (server)

### Manager — single source of truth

- Version metadata cache (`version_manifest_v2.json`, ETag conditional requests)
- Downloader: concurrency, resume, SHA-1 verification, mirror fallback (official → BMCLAPI)
- Instance registry: isolated gameDir / JVM args / memory / Java path
- Java runtime: system JDK discovery + Mojang JRE auto-download
- Auth: offline / Microsoft Device Code / authlib-injector
- Builds the `LaunchPlan` (pure data, supports dry-run)
- Plugin host

### TUI / CLI — replaceable shell

- One command parser, three renderers: full-screen TUI / line mode / `--json`
- The non-interactive mode must be complete and scriptable

### Plugin — two tiers

| Tier | Form | Status |
|---|---|---|
| L1 declarative | `*.toml`: download mirrors + launch hooks + TUI contributions | ✅ |
| L2 logic | WASM (sandboxed) | ⬜ |

L1 is implemented:

- Scans `<root>/plugins/*/plugin.toml` or `<root>/plugins/*.toml`
- `[[providers]]` mirror rewriting; `[launch]` appends JVM/game args and environment variables
- `[tui]` adds status-bar text and registers Actions menu entries and single-key shortcuts
- Enable state lives in `<root>/plugins/state.json`
- CLI: `qcl plugin list / info / enable / disable`
- TUI: `Esc → Plugins`, toggle with Space/Enter
- Invalid manifests (TOML errors, unsupported api, duplicate names) are listed but never applied

See [`docs/plugins.md`](docs/plugins.md) for the full manifest reference.

L2 permission preview: `fs:read:<glob>` / `fs:write:<glob>` / `net:<host>` / `process:spawn` / `auth:token`

---

## Repository layout

```
QCL/
├── Cargo.toml              # workspace
├── crates/
│   ├── mc-core/            # pure data model: LaunchPlan, Instance, LogLine, CrashInfo
│   ├── mc-ipc/             # JSON-RPC 2.0 types, events, line framing
│   ├── mc-plugin/          # L1 plugins: manifest, registry, enable state, mirrors, launch hooks
│   ├── mc-manager/         # manifest / version / rules / download / java / auth / install / launch
│   ├── mc-wrapper/         # process host: spawn, stdio pumps, log levels, crash detection
│   ├── mc-tui/             # ratatui UI: version list / filter / progress / logs / help
│   └── mc-cli/             # `qcl` binary
├── plugins/
│   └── bmclapi/plugin.toml # example mirror provider
└── docs/
    ├── plugins.md          # plugin manifest reference
    └── protocol.md         # IPC protocol
```

### Implementation status

| Area | Status |
|---|---|
| Version manifest fetch + disk cache (offline fallback) | ✅ |
| version.json parsing, rules evaluation, Maven coordinates | ✅ |
| Concurrent downloads + SHA-1 + atomic writes + mirror fallback | ✅ |
| client jar / libraries / natives extraction / assets / log4j config | ✅ |
| log4j console XML → plain text rewrite (otherwise logs are `<log4j:Event>` blobs) | ✅ |
| Java discovery (auto JRE download) | discovery ✅ / download ⬜ |
| Offline profile (`OfflinePlayer:` MD5 UUID) | ✅ |
| LaunchPlan assembly (argument templates, legacy support, `--dry-run`) | ✅ |
| Wrapper: spawn, stdout/stderr split, log levels, crash detection | ✅ |
| JSON-RPC event stream (`--json`) | types ✅ / daemon ⬜ |
| Microsoft login (Device Code) | ⬜ M4 |
| Mod loader (Fabric/Forge/NeoForge) | ⬜ |
| Plugin registry + enable state (`plugins/state.json`) | ✅ |
| L1 mirror providers (BMCLAPI example) | ✅ |
| L1 launch hooks (JVM/game args + env) | ✅ |
| L1 TUI contributions (status text / Actions / shortcuts) | ✅ |
| L2 WASM plugins | ⬜ |
| Instance registry | read/write ✅ / CLI ⬜ |
| TUI full-screen UI | ✅ three panes (VersionTable / editable cmdline / logs) / filter / progress / mouse |
| Server PTY + console | ⬜ M6 |

---

## Milestones

| Phase | Goal | Status |
|---|---|---|
| M0 · headless | `qcl run <version>` assembles the command and starts java, logs stream out | ✅ |
| M1 · protocol | Event stream as JSON-RPC line frames, `--json` usable | 🟡 CLI usable, daemon/attach pending |
| M2 · CLI | `list / install / run / attach / kill / instance` | 🟡 first three usable |
| M3 · TUI | Full-screen version list, download progress, log pane | ✅ |
| M4 · auth | Microsoft Device Code, completed inside the terminal | ⬜ |
| M5 · plugins | L1 registry/enable/mirrors/launch hooks → L2 WASM | 🟡 L1 complete |
| M6 · server | PTY + console + watchdog | ⬜ |

---

## Verified (local smoke test · 2026-10-31)

| Check | Result |
|---|---|
| `qcl list --limit 5` | ✅ fetched 26.3 / 26.2 / 26.1.2 / 26.1.1 / 26.1 |
| `qcl install 1.20.1` | ✅ 53 libraries + 3598 assets; BMCLAPI failure falls back to the official source |
| `qcl run 1.20.1 --dry-run` | ✅ full JVM + game arguments; JDK 21 version check passed |
| `qcl run 1.20.1` (real machine) | ✅ main menu in 10s: LWJGL 3.3.1, OpenAL, `Sound engine started` |
| `qcl tui` (PTY) | ✅ filter `1.20.1` → Enter → main menu → PgUp → `s` stop → exit code 0 |
| TUI mouse | ✅ click row 5 → `selected 26.1.1`; wheel/help clicks no panic; SGR `1006h` enabled |
| Plugin CLI | ✅ `plugin list/info/enable/disable`, `state.json` persisted |
| Plugin launch hook | ✅ enabled dry-run shows `-Dqcl.tweaks=1` / `--demo` / `QCL_HOOK=1`; disabled it disappears |
| TUI Plugins | ✅ `[x]` → Space → `[ ]`, state written, LaunchPlan rebuilt |
| TUI plugin contributions | ✅ `★ Steve` in status bar; `h` runs message; `Esc → Actions` lists actions; disabled it disappears |
| Wrapper fake-java test | ✅ stdout/stderr split, INFO/WARN/ERROR levels, exit code, crash detection |
| `--json` framing | ✅ all 6 event lines parse with `json.loads` |
| `cargo test --workspace` | ✅ 21 passed |
| `cargo clippy --workspace --all-targets` | ✅ no warnings |

---

## Known limitations

- Real-machine validation was only done on Linux/Wayland + AMD 780M + JDK 21; Windows/macOS are untested
- Offline mode logs a 401 `player/attributes` stack trace from authlib; expected noise
- JDK 21 makes LWJGL print `Unsupported JNI version`; JDK 17 is recommended, functionality is unaffected
- The first full asset download is ~3600 objects; duration depends on the mirror
- No daemon/attach yet: IPC types and protocol are defined, but the CLI is still a single process that spawns directly
- No Microsoft login, mod loader, or server PTY
- The TUI has no instance (multi-instance isolation) concept yet; it launches by version and runs one game at a time
- If the terminal is SIGKILLed or closed, the child java process may be orphaned; needs `pdeathsig`/process groups (alongside Windows support)
- L2 WASM plugins and automatic Java runtime download are not implemented

---

## Constraints

- Never redistribute Mojang jars/assets
- No piracy login endpoints; offline mode is for local/personal use only
- Microsoft login requires your own Azure app `client_id`
- Cross-platform logic avoids the shell to dodge quoting hell

## License

GPL-3.0-or-later, see [LICENSE](LICENSE).
