//! Terminal frontend.
//!
//! Three panes: version list, live game log, status/progress bar. All heavy
//! work (manifest, downloads, spawning) runs in background tasks and reports
//! back through [`UiEvent`]; the UI thread never blocks on network or process IO.

use anyhow::{bail, Result};
use mc_core::{DownloadProgress, LaunchPlan, LogLine, ProcessState, VersionSummary};
use mc_manager::Manager;
use mc_wrapper::RunningGame;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event as TermEvent, KeyEventKind,
};
use ratatui::crossterm::execute;
use ratatui::DefaultTerminal;
use serde::{Deserialize, Serialize};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

mod app;
mod cmdline;
mod menu;
mod plugin_ui;
mod ui;

pub use app::{App, Focus, StatusAction};
pub use menu::{MenuItem, MenuScreen};

/// Persisted settings, also used as the runtime configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TuiOptions {
    pub username: String,
    pub memory_mb: u32,
    pub java: Option<PathBuf>,
}

impl Default for TuiOptions {
    fn default() -> Self {
        Self {
            username: "Player".to_string(),
            memory_mb: 2048,
            java: None,
        }
    }
}

impl TuiOptions {
    pub fn path(root: &Path) -> PathBuf {
        root.join("settings.json")
    }

    pub fn load(root: &Path) -> Self {
        let path = Self::path(root);
        match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str(&text) {
                Ok(options) => options,
                Err(err) => {
                    tracing::warn!(%err, path = %path.display(), "ignoring invalid settings");
                    Self::default()
                }
            },
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, root: &Path) -> std::io::Result<()> {
        let path = Self::path(root);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(self)
            .map_err(|err| std::io::Error::other(err.to_string()))?;
        std::fs::write(path, text)
    }
}

/// CLI flags override the persisted settings for this session only.
#[derive(Debug, Clone, Default)]
pub struct TuiOverrides {
    pub username: Option<String>,
    pub memory_mb: Option<u32>,
    pub java: Option<PathBuf>,
}

pub enum UiEvent {
    Versions(Vec<VersionSummary>),
    Status(String),
    Progress(DownloadProgress),
    Log(LogLine),
    State(ProcessState),
    Spawned {
        version: String,
        game: Box<RunningGame>,
    },
    Exited(Option<i32>),
    Failed(String),
    /// Preview of the command line for the selected version.
    Cmdline {
        version: String,
        text: String,
        plan: Box<LaunchPlan>,
    },
    CmdlineError {
        version: String,
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyOutcome {
    Continue,
    Quit,
}

pub async fn run(manager: Manager, overrides: TuiOverrides) -> Result<()> {
    if !std::io::stdout().is_terminal() {
        bail!("the TUI needs an interactive terminal; use `qcl run` for scripts");
    }

    let mut opts = TuiOptions::load(&manager.root.root);
    if let Some(username) = overrides.username {
        opts.username = username;
    }
    if let Some(memory_mb) = overrides.memory_mb {
        opts.memory_mb = memory_mb;
    }
    if let Some(java) = overrides.java {
        opts.java = Some(java);
    }

    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut app = App::new(manager, opts);
    app.load_versions(&tx);

    let mut terminal = ratatui::init();
    execute!(std::io::stdout(), EnableMouseCapture)?;

    // `ratatui::init` already installed a restore hook; wrap it so mouse
    // reporting is also disabled if we panic.
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(std::io::stdout(), DisableMouseCapture);
        previous_hook(info);
    }));

    let result = event_loop(&mut terminal, &mut app, &mut rx, &tx);
    let _ = execute!(std::io::stdout(), DisableMouseCapture);
    app.shutdown();
    ratatui::restore();
    result
}

fn event_loop(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    rx: &mut UnboundedReceiver<UiEvent>,
    tx: &UnboundedSender<UiEvent>,
) -> Result<()> {
    loop {
        while let Ok(event) = rx.try_recv() {
            app.handle(event, tx);
        }
        app.tick();
        terminal.draw(|frame| ui::draw(frame, app))?;

        if event::poll(Duration::from_millis(50))? {
            let outcome = match event::read()? {
                TermEvent::Key(key) if key.kind == KeyEventKind::Press => app.on_key(key, tx),
                TermEvent::Mouse(mouse) => app.on_mouse(mouse, tx),
                _ => KeyOutcome::Continue,
            };
            if outcome == KeyOutcome::Quit {
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TuiOptions;

    #[test]
    fn settings_roundtrip() {
        let dir = std::env::temp_dir().join(format!("qcl-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let options = TuiOptions {
            username: "Alex".to_string(),
            memory_mb: 6144,
            java: None,
        };
        options.save(&dir).unwrap();

        let loaded = TuiOptions::load(&dir);
        assert_eq!(loaded.username, "Alex");
        assert_eq!(loaded.memory_mb, 6144);
        assert!(loaded.java.is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
