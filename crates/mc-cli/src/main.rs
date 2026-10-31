use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use mc_core::LogLevel;
use mc_ipc::Event;
use mc_manager::auth::Profile;
use mc_manager::install::InstallOptions;
use mc_manager::launch::LaunchOptions;
use mc_manager::paths::DataRoot;
use mc_manager::{java, Manager};
use std::io::IsTerminal;
use std::path::PathBuf;
use tokio::sync::mpsc;

#[derive(Parser)]
#[command(
    name = "qcl",
    version,
    about = "QuarkCraftLauncher — a terminal-first Minecraft launcher"
)]
struct Cli {
    /// Data directory (also honours QCL_HOME)
    #[arg(long, global = true, env = "QCL_HOME")]
    root: Option<PathBuf>,

    /// Version manifest URL
    #[arg(
        long,
        global = true,
        default_value = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json"
    )]
    manifest: String,

    /// Log filter, e.g. info or qcl=debug
    #[arg(long, global = true, default_value = "info", env = "QCL_LOG")]
    log: String,

    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum PluginCmd {
    /// List discovered plugins
    List {
        #[arg(long)]
        json: bool,
    },
    /// Show details for one plugin
    Info { name: String },
    /// Enable a plugin
    Enable { name: String },
    /// Disable a plugin
    Disable { name: String },
}

#[derive(Subcommand)]
enum Cmd {
    /// List versions from the manifest
    List {
        #[arg(long)]
        json: bool,
        /// Include snapshots and old betas
        #[arg(long)]
        snapshots: bool,
        #[arg(long, default_value_t = 30)]
        limit: usize,
    },

    /// Download a version (client jar, libraries, assets)
    Install {
        version: String,
        #[arg(long)]
        no_assets: bool,
        #[arg(long, default_value_t = 16)]
        concurrency: usize,
    },

    /// Build a launch plan and run the game (offline profile)
    Run {
        version: String,
        #[arg(long, default_value = "Player")]
        username: String,
        /// Print the launch plan and exit
        #[arg(long)]
        dry_run: bool,
        /// Emit JSON-RPC notification lines on stdout
        #[arg(long)]
        json: bool,
        #[arg(long)]
        no_assets: bool,
        #[arg(long)]
        memory: Option<u32>,
        #[arg(long)]
        width: Option<u32>,
        #[arg(long)]
        height: Option<u32>,
        #[arg(long = "jvm-arg")]
        jvm_arg: Vec<String>,
        #[arg(long = "game-arg")]
        game_arg: Vec<String>,
        /// Override the java executable
        #[arg(long = "java")]
        java_path: Option<PathBuf>,
    },

    /// Show the detected Java runtime
    Java {
        #[arg(long = "java")]
        java_path: Option<PathBuf>,
    },

    /// Manage L1 plugins
    Plugin {
        #[command(subcommand)]
        command: PluginCmd,
    },

    /// Terminal UI
    Tui {
        #[arg(long)]
        username: Option<String>,
        #[arg(long)]
        memory: Option<u32>,
        #[arg(long = "java")]
        java_path: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = DataRoot::new(cli.root.clone().unwrap_or_else(DataRoot::default_root));

    // In TUI mode stderr is the alternate screen: any tracing output would
    // corrupt the layout, so send logs to a file instead.
    let log_file = matches!(&cli.command, Cmd::Tui { .. }).then(|| root.root.join("logs/qcl.log"));
    init_tracing(&cli.log, log_file);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("build tokio runtime")?;
    runtime.block_on(run(cli, root))
}

async fn run(cli: Cli, root: DataRoot) -> Result<()> {
    let manager = Manager::new(root, cli.manifest)?;

    match cli.command {
        Cmd::List {
            json,
            snapshots,
            limit,
        } => {
            let manifest = manager.manifest().await?;
            let versions: Vec<_> = manifest
                .versions
                .iter()
                .filter(|version| snapshots || version.kind == "release")
                .take(limit)
                .collect();

            if json {
                println!("{}", serde_json::to_string_pretty(&versions)?);
            } else {
                for version in versions {
                    println!(
                        "{:<18} {:<10} {}",
                        version.id, version.kind, version.release_time
                    );
                }
            }
        }

        Cmd::Install {
            version,
            no_assets,
            concurrency,
        } => {
            let version = manager.version(&version).await?;
            let options = InstallOptions {
                assets: !no_assets,
                concurrency,
            };
            let report = manager.install(&version, &options, None).await?;
            eprintln!(
                "installed {} files, {} assets for {}",
                report.files, report.assets, version.id
            );
        }

        Cmd::Run {
            version,
            username,
            dry_run,
            json,
            no_assets,
            memory,
            width,
            height,
            jvm_arg,
            game_arg,
            java_path,
        } => {
            let version_file = manager.version(&version).await?;

            if !dry_run {
                let options = InstallOptions {
                    assets: !no_assets,
                    concurrency: 16,
                };
                let report = manager.install(&version_file, &options, None).await?;
                eprintln!("ready: {} files, {} assets", report.files, report.assets);
            }

            let java_bin = java::find(java_path.as_deref(), &manager.root)
                .context("no java runtime found; install a JDK or set QCL_JAVA")?;
            if let Some(version_line) = java::version_of(&java_bin).await {
                eprintln!("java: {version_line}");
                let required = version_file.required_java();
                if let Some(major) = java::major_from_line(&version_line) {
                    if major < required {
                        anyhow::bail!(
                            "java {major} is too old for {} (requires {required}+)",
                            version_file.id
                        );
                    }
                }
            }

            let profile = Profile::offline(&username);
            let options = LaunchOptions {
                memory_mb: memory.unwrap_or(2048),
                min_memory_mb: 512,
                extra_jvm: jvm_arg,
                extra_game: game_arg,
                width,
                height,
                demo: false,
            };
            let plan = manager.plan(&version_file, &version, java_bin, &profile, &options)?;

            if dry_run {
                if json {
                    println!("{}", serde_json::to_string_pretty(&plan)?);
                } else {
                    println!("{}", plan.command_line());
                }
                return Ok(());
            }

            let session = version.clone();
            let (tx, mut rx) = mpsc::unbounded_channel();
            let mut game = mc_wrapper::spawn(&plan, session.clone(), tx).await?;

            let exit_code = loop {
                tokio::select! {
                    incoming = rx.recv() => {
                        match incoming {
                            Some(event) => emit(&event, json)?,
                            None => break None,
                        }
                    }
                    status = game.wait() => {
                        break status?;
                    }
                }
            };

            // Drain anything the pumps buffered before the process exited.
            while let Ok(event) = rx.try_recv() {
                emit(&event, json)?;
            }

            let crash = mc_wrapper::classify::detect_crash(exit_code, &plan.working_dir);
            let exit = Event::ProcessExit {
                session,
                code: exit_code,
                crash: crash.clone(),
            };
            emit(&exit, json)?;
            if let Some(crash) = crash {
                eprintln!("crash detected: {}", crash.reason);
            }
        }

        Cmd::Java { java_path } => {
            let path = java::find(java_path.as_deref(), &manager.root)
                .context("no java runtime found; install a JDK or set QCL_JAVA")?;
            println!("{}", path.display());
            if let Some(line) = java::version_of(&path).await {
                println!("{line}");
            }
        }

        Cmd::Plugin { command } => {
            let mut registry = mc_plugin::PluginRegistry::load(&manager.root.root);
            match command {
                PluginCmd::List { json } => {
                    if json {
                        println!("{}", serde_json::to_string_pretty(&registry.entries)?);
                    } else if registry.entries.is_empty() {
                        println!("no plugins in {}", manager.root.plugins().display());
                    } else {
                        for entry in &registry.entries {
                            let state = if entry.enabled { "on " } else { "off" };
                            let error = entry
                                .error
                                .as_deref()
                                .map(|error| format!("  error: {error}"))
                                .unwrap_or_default();
                            println!(
                                "{state} {:<14} {:<8} {}{error}",
                                entry.name,
                                entry.version,
                                entry.capabilities()
                            );
                        }
                    }
                }
                PluginCmd::Info { name } => {
                    let Some(entry) = registry.entries.iter().find(|entry| entry.name == name)
                    else {
                        anyhow::bail!("plugin not found: {name}");
                    };
                    println!("name:         {}", entry.name);
                    println!("version:      {}", entry.version);
                    println!("api:          {}", entry.api);
                    println!("enabled:      {}", entry.enabled);
                    println!("description:  {}", entry.description);
                    println!("author:       {}", entry.author);
                    println!("homepage:     {}", entry.homepage);
                    println!("capabilities: {}", entry.capabilities());
                    println!("path:         {}", entry.path.display());
                    if let Some(error) = &entry.error {
                        println!("error:        {error}");
                    }
                }
                PluginCmd::Enable { name } => {
                    registry.set_enabled(&name, true)?;
                    println!("enabled {name}");
                }
                PluginCmd::Disable { name } => {
                    registry.set_enabled(&name, false)?;
                    println!("disabled {name}");
                }
            }
        }

        Cmd::Tui {
            username,
            memory,
            java_path,
        } => {
            let overrides = mc_tui::TuiOverrides {
                username,
                memory_mb: memory,
                java: java_path,
            };
            mc_tui::run(manager, overrides).await?;
        }
    }

    Ok(())
}

fn emit(event: &Event, json: bool) -> Result<()> {
    if json {
        // `encode` already appends the framing newline.
        print!("{}", mc_ipc::encode(event)?);
    } else {
        render(event);
    }
    Ok(())
}

fn render(event: &Event) {
    match event {
        Event::LogLine { line, .. } => {
            let tag = match line.level {
                LogLevel::Trace => "TRC",
                LogLevel::Debug => "DBG",
                LogLevel::Info => "INF",
                LogLevel::Warn => "WRN",
                LogLevel::Error => "ERR",
            };
            println!("[{tag}] {}", line.text);
        }
        Event::ProcessState { state, .. } => eprintln!("== {state:?} =="),
        Event::ProcessExit { code, .. } => eprintln!("== exited {code:?} =="),
        Event::DownloadProgress { label, done, total } => {
            eprintln!("{label}: {done}/{total}")
        }
    }
}

fn init_tracing(level: &str, file: Option<PathBuf>) {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| level.to_string());
    let builder = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(filter))
        .with_target(false);

    if let Some(path) = file {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            Ok(file) => {
                let _ = builder
                    .with_ansi(false)
                    .with_writer(move || file.try_clone().expect("clone log file"))
                    .try_init();
            }
            Err(_) => {
                let _ = builder.with_writer(std::io::sink).try_init();
            }
        }
        return;
    }

    let _ = builder
        .with_ansi(std::io::stderr().is_terminal())
        .with_writer(std::io::stderr)
        .try_init();
}
