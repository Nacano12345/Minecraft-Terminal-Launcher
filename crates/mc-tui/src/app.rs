use crate::cmdline;
use crate::menu::{MenuItem, MenuScreen, MenuState, OptionField};
use crate::plugin_ui::{self, PluginAction, StatusContext};
use crate::ui::Areas;
use crate::{KeyOutcome, TuiOptions, UiEvent};
use mc_core::{
    DownloadProgress, LaunchPlan, LogLevel, LogLine, LogSource, ProcessState, VersionSummary,
};
use mc_manager::auth::Profile;
use mc_manager::install::InstallOptions;
use mc_manager::java;
use mc_manager::launch::LaunchOptions;
use mc_manager::Manager;
use mc_wrapper::RunningGame;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use ratatui::widgets::ListState;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{self, UnboundedSender};

const MAX_LOG_LINES: usize = 5000;

/// Which pane receives keyboard input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Versions,
    Cmdline,
}

/// Actions reachable by clicking the bottom hint bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusAction {
    Menu,
    Launch,
    Install,
    EditCmd,
    Filter,
    Stop,
    Help,
    Quit,
    Apply,
    Clear,
    Run,
    Done,
    ResetCmd,
}

pub struct App {
    pub manager: Manager,
    pub opts: TuiOptions,
    pub all_versions: Vec<VersionSummary>,
    pub versions: Vec<VersionSummary>,
    pub filter: String,
    pub searching: bool,
    pub selected: usize,
    pub list_state: ListState,
    pub logs: VecDeque<LogLine>,
    /// Lines scrolled up from the tail; 0 means follow.
    pub log_offset: usize,
    pub status: String,
    pub progress: Option<DownloadProgress>,
    pub game: Option<RunningGame>,
    pub running_version: Option<String>,
    pub busy: bool,
    pub help: bool,
    /// Regions from the last frame, for mouse hit-testing.
    pub areas: Areas,
    /// Last left-click (time, version index) for double-click detection.
    pub last_click: Option<(Instant, usize)>,
    /// Clickable regions in the bottom bar, rebuilt every frame.
    pub status_hits: Vec<(Rect, StatusAction)>,
    pub focus: Focus,
    /// Editable launch command line (previewed from the selected version).
    pub cmdline: String,
    /// Cursor as a char index into `cmdline`.
    pub cmdline_cursor: usize,
    pub cmdline_dirty: bool,
    pub plan: Option<LaunchPlan>,
    pub menu: MenuState,
    /// Set by the UI each frame; false while the terminal is below the safe size.
    pub size_ok: bool,
    pub plugin_actions: Vec<PluginAction>,
    pub plugin_statuses: Vec<String>,
}

impl App {
    pub fn new(manager: Manager, opts: TuiOptions) -> Self {
        let mut app = Self {
            manager,
            opts,
            all_versions: Vec::new(),
            versions: Vec::new(),
            filter: String::new(),
            searching: false,
            selected: 0,
            list_state: ListState::default(),
            logs: VecDeque::new(),
            log_offset: 0,
            status: "loading versions…".to_string(),
            progress: None,
            game: None,
            running_version: None,
            busy: false,
            help: false,
            areas: Areas::default(),
            last_click: None,
            status_hits: Vec::new(),
            focus: Focus::Versions,
            cmdline: String::new(),
            cmdline_cursor: 0,
            cmdline_dirty: false,
            plan: None,
            menu: MenuState::default(),
            size_ok: false,
            plugin_actions: Vec::new(),
            plugin_statuses: Vec::new(),
        };
        app.sync_plugin_ui();
        app
    }

    pub fn sync_plugin_ui(&mut self) {
        let (actions, statuses) = plugin_ui::collect(&self.manager.plugins);
        self.plugin_actions = actions;
        self.plugin_statuses = statuses;
        self.menu.action_selected = self
            .menu
            .action_selected
            .min(self.plugin_actions.len().saturating_sub(1));
    }

    pub fn plugin_status_text(&self) -> String {
        if self.plugin_statuses.is_empty() {
            return String::new();
        }
        let context = StatusContext {
            user: self.opts.username.clone(),
            memory_mb: self.opts.memory_mb,
            selected: self.current_version_id().unwrap_or_else(|| "-".to_string()),
            running: self
                .running_version
                .clone()
                .unwrap_or_else(|| "-".to_string()),
            plugins: self
                .manager
                .plugins
                .entries
                .iter()
                .filter(|entry| entry.enabled)
                .count(),
        };

        self.plugin_statuses
            .iter()
            .map(|template| plugin_ui::render_status(template, &context))
            .filter(|text| !text.trim().is_empty())
            .collect::<Vec<_>>()
            .join(" · ")
    }

    pub fn load_versions(&mut self, tx: &UnboundedSender<UiEvent>) {
        self.status = "loading versions…".to_string();
        let manager = self.manager.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            match manager.manifest().await {
                Ok(manifest) => {
                    let versions: Vec<VersionSummary> = manifest
                        .versions
                        .into_iter()
                        .filter(|version| version.kind == "release")
                        .collect();
                    let _ = tx.send(UiEvent::Versions(versions));
                }
                Err(err) => {
                    let _ = tx.send(UiEvent::Failed(format!("manifest: {err}")));
                }
            }
        });
    }

    /// Rebuild the command line preview for the selected version.
    pub fn refresh_cmdline(&mut self, tx: &UnboundedSender<UiEvent>) {
        self.cmdline_dirty = false;
        self.plan = None;
        self.cmdline.clear();
        self.cmdline_cursor = 0;

        let Some(summary) = self.selected_summary() else {
            return;
        };
        self.cmdline = "building command line…".to_string();

        let manager = self.manager.clone();
        let opts = self.opts.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            let version = match manager.version_summary(&summary).await {
                Ok(version) => version,
                Err(err) => {
                    let _ = tx.send(UiEvent::CmdlineError {
                        version: summary.id,
                        message: format!("version: {err}"),
                    });
                    return;
                }
            };

            let Some(java_bin) = java::find(opts.java.as_deref(), &manager.root) else {
                let _ = tx.send(UiEvent::CmdlineError {
                    version: summary.id,
                    message: "no java runtime found (set QCL_JAVA or JAVA_HOME)".to_string(),
                });
                return;
            };

            let profile = Profile::offline(&opts.username);
            let launch_options = LaunchOptions {
                memory_mb: opts.memory_mb,
                ..LaunchOptions::default()
            };
            match manager.plan(&version, &summary.id, java_bin, &profile, &launch_options) {
                Ok(plan) => {
                    let text = plan.command_line();
                    let _ = tx.send(UiEvent::Cmdline {
                        version: summary.id,
                        text,
                        plan: Box::new(plan),
                    });
                }
                Err(err) => {
                    let _ = tx.send(UiEvent::CmdlineError {
                        version: summary.id,
                        message: format!("plan: {err}"),
                    });
                }
            }
        });
    }

    pub fn install_selected(&mut self, tx: &UnboundedSender<UiEvent>) {
        if self.busy {
            return;
        }
        let Some(summary) = self.selected_summary() else {
            self.status = "no version selected".to_string();
            return;
        };

        self.busy = true;
        self.progress = None;
        self.status = format!("installing {}", summary.id);

        let manager = self.manager.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            let version = match manager.version_summary(&summary).await {
                Ok(version) => version,
                Err(err) => {
                    let _ = tx.send(UiEvent::Failed(format!("version: {err}")));
                    return;
                }
            };

            let (sink, progress_rx) = mpsc::unbounded_channel();
            let forward = forward_progress(progress_rx, tx.clone());
            let result = manager
                .install(&version, &InstallOptions::default(), Some(&sink))
                .await;
            drop(sink);
            let _ = forward.await;

            match result {
                Ok(report) => {
                    let _ = tx.send(UiEvent::Status(format!(
                        "installed {} ({} files, {} assets)",
                        version.id, report.files, report.assets
                    )));
                }
                Err(err) => {
                    let _ = tx.send(UiEvent::Failed(format!("install: {err}")));
                }
            }
        });
    }

    pub fn launch_selected(&mut self, tx: &UnboundedSender<UiEvent>) {
        if self.busy {
            return;
        }
        if self.game.is_some() {
            self.status = "a game is already running (press s to stop)".to_string();
            return;
        }
        let Some(summary) = self.selected_summary() else {
            self.status = "no version selected".to_string();
            return;
        };

        let edited = self.cmdline.clone();
        let dirty = self.cmdline_dirty;

        self.busy = true;
        self.progress = None;
        self.logs.clear();
        self.log_offset = 0;
        self.status = format!("preparing {}", summary.id);

        let manager = self.manager.clone();
        let opts = self.opts.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            let version = match manager.version_summary(&summary).await {
                Ok(version) => version,
                Err(err) => {
                    let _ = tx.send(UiEvent::Failed(format!("version: {err}")));
                    return;
                }
            };

            let (sink, progress_rx) = mpsc::unbounded_channel();
            let forward = forward_progress(progress_rx, tx.clone());
            let result = manager
                .install(&version, &InstallOptions::default(), Some(&sink))
                .await;
            drop(sink);
            let _ = forward.await;
            if let Err(err) = result {
                let _ = tx.send(UiEvent::Failed(format!("install: {err}")));
                return;
            }

            let Some(java_bin) = java::find(opts.java.as_deref(), &manager.root) else {
                let _ = tx.send(UiEvent::Failed(
                    "no java runtime found (set QCL_JAVA or JAVA_HOME)".to_string(),
                ));
                return;
            };
            let required = version.required_java();
            if let Some(line) = java::version_of(&java_bin).await {
                if let Some(major) = java::major_from_line(&line) {
                    if major < required {
                        let _ = tx.send(UiEvent::Failed(format!(
                            "java {major} is too old for {} (requires {required}+)",
                            version.id
                        )));
                        return;
                    }
                }
            }

            let profile = Profile::offline(&opts.username);
            let launch_options = LaunchOptions {
                memory_mb: opts.memory_mb,
                ..LaunchOptions::default()
            };
            let plan =
                match manager.plan(&version, &summary.id, java_bin, &profile, &launch_options) {
                    Ok(plan) => plan,
                    Err(err) => {
                        let _ = tx.send(UiEvent::Failed(format!("plan: {err}")));
                        return;
                    }
                };

            let (event_tx, mut event_rx) = mpsc::unbounded_channel();
            let spawn_result = if dirty {
                match cmdline::tokenize(&edited) {
                    Ok(tokens) => {
                        let program = &tokens[0];
                        let args = &tokens[1..];
                        mc_wrapper::spawn_raw(
                            program,
                            args,
                            &plan.working_dir,
                            &plan.env,
                            summary.id.clone(),
                            event_tx,
                        )
                        .await
                    }
                    Err(err) => {
                        let _ = tx.send(UiEvent::Failed(format!("command line: {err}")));
                        return;
                    }
                }
            } else {
                mc_wrapper::spawn(&plan, summary.id.clone(), event_tx).await
            };

            let game = match spawn_result {
                Ok(game) => game,
                Err(err) => {
                    let _ = tx.send(UiEvent::Failed(format!("spawn: {err}")));
                    return;
                }
            };
            let _ = tx.send(UiEvent::Spawned {
                version: summary.id.clone(),
                game: Box::new(game),
            });

            tokio::spawn(async move {
                while let Some(event) = event_rx.recv().await {
                    match event {
                        mc_ipc::Event::LogLine { line, .. } => {
                            let _ = tx.send(UiEvent::Log(line));
                        }
                        mc_ipc::Event::ProcessState { state, .. } => {
                            let _ = tx.send(UiEvent::State(state));
                        }
                        mc_ipc::Event::ProcessExit { code, .. } => {
                            let _ = tx.send(UiEvent::Exited(code));
                        }
                        mc_ipc::Event::DownloadProgress { .. } => {}
                    }
                }
            });
        });
    }

    pub fn stop(&mut self) {
        match &mut self.game {
            Some(game) => {
                game.kill();
                self.status = "stopping…".to_string();
            }
            None => {
                self.status = "nothing is running".to_string();
            }
        }
    }

    pub fn handle(&mut self, event: UiEvent, tx: &UnboundedSender<UiEvent>) {
        match event {
            UiEvent::Versions(versions) => {
                self.all_versions = versions;
                self.filter.clear();
                self.apply_filter();
                self.status = format!("{} versions", self.all_versions.len());
                self.busy = false;
                self.refresh_cmdline(tx);
            }
            UiEvent::Status(status) => {
                self.status = status;
                self.busy = false;
                self.progress = None;
            }
            UiEvent::Progress(progress) => {
                self.progress = Some(progress);
            }
            UiEvent::Log(line) => {
                self.push_log(line);
            }
            UiEvent::State(state) => {
                self.status = match state {
                    ProcessState::Starting => "starting…",
                    ProcessState::Running => "running",
                    ProcessState::Stopping => "stopping…",
                    ProcessState::Exited => "exited",
                }
                .to_string();
            }
            UiEvent::Spawned { version, game } => {
                self.game = Some(*game);
                self.running_version = Some(version.clone());
                self.busy = false;
                self.progress = None;
                self.status = format!("running {version}");
            }
            UiEvent::Exited(code) => {
                self.game = None;
                self.running_version = None;
                self.busy = false;
                self.status = match code {
                    Some(code) => format!("exited ({code})"),
                    None => "exited".to_string(),
                };
            }
            UiEvent::Failed(message) => {
                self.busy = false;
                self.progress = None;
                self.status = format!("error: {message}");
                self.push_system(LogLevel::Error, format!("error: {message}"));
            }
            UiEvent::Cmdline {
                version,
                text,
                plan,
            } => {
                if self.current_version_id().as_deref() == Some(version.as_str())
                    && !self.cmdline_dirty
                {
                    self.plan = Some(*plan);
                    self.cmdline = text;
                    self.cmdline_cursor = self.cmdline.chars().count();
                }
            }
            UiEvent::CmdlineError { version, message } => {
                if self.current_version_id().as_deref() == Some(version.as_str())
                    && !self.cmdline_dirty
                {
                    self.plan = None;
                    self.cmdline = format!("(!) {message}");
                    self.cmdline_cursor = self.cmdline.chars().count();
                }
            }
        }
    }

    /// Poll the child process without blocking the UI.
    pub fn tick(&mut self) {
        let outcome = self.game.as_mut().map(|game| game.try_wait());
        match outcome {
            Some(Ok(Some(status))) => {
                let code = status.code();
                self.game = None;
                self.running_version = None;
                self.busy = false;
                self.status = match code {
                    Some(code) => format!("exited ({code})"),
                    None => "exited".to_string(),
                };
            }
            Some(Err(err)) => {
                self.status = format!("process error: {err}");
            }
            _ => {}
        }
    }

    pub fn on_key(&mut self, key: KeyEvent, tx: &UnboundedSender<UiEvent>) -> KeyOutcome {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return KeyOutcome::Quit;
        }

        if !self.size_ok {
            return if key.code == KeyCode::Char('q') {
                KeyOutcome::Quit
            } else {
                KeyOutcome::Continue
            };
        }

        if self.menu.open {
            return self.on_menu_key(key, tx);
        }

        if self.help {
            if matches!(
                key.code,
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?')
            ) {
                self.help = false;
            }
            return KeyOutcome::Continue;
        }

        if self.focus == Focus::Cmdline {
            return self.on_cmdline_key(key, tx);
        }

        if self.searching {
            match key.code {
                KeyCode::Enter => {
                    self.searching = false;
                    self.refresh_cmdline(tx);
                }
                KeyCode::Esc => {
                    self.searching = false;
                    self.filter.clear();
                    self.apply_filter();
                    self.refresh_cmdline(tx);
                }
                KeyCode::Backspace => {
                    self.filter.pop();
                    self.apply_filter();
                }
                KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.filter.push(ch);
                    self.apply_filter();
                }
                _ => {}
            }
            return KeyOutcome::Continue;
        }

        if let KeyCode::Char(ch) = key.code {
            if !key.modifiers.contains(KeyModifiers::CONTROL) {
                if let Some(action) = self
                    .plugin_actions
                    .iter()
                    .find(|action| action.key == Some(ch))
                    .cloned()
                {
                    self.run_plugin_action(&action, tx);
                    return KeyOutcome::Continue;
                }
            }
        }

        match key.code {
            KeyCode::Char('q') => return KeyOutcome::Quit,
            KeyCode::Char('?') => self.help = true,
            KeyCode::Char('/') => self.searching = true,
            KeyCode::Tab | KeyCode::Char('e') => self.focus_cmdline(),
            KeyCode::Esc => self.menu.reset(),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1, tx),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1, tx),
            KeyCode::Home | KeyCode::Char('g') => self.select_and_refresh(0, tx),
            KeyCode::End | KeyCode::Char('G') => {
                if !self.versions.is_empty() {
                    let last = self.versions.len() - 1;
                    self.select_and_refresh(last, tx);
                }
            }
            KeyCode::Char('i') => self.install_selected(tx),
            KeyCode::Enter | KeyCode::Char('l') => self.launch_selected(tx),
            KeyCode::Char('s') => self.stop(),
            KeyCode::Char('r') => self.load_versions(tx),
            KeyCode::PageUp => self.scroll_logs(10),
            KeyCode::PageDown => self.scroll_logs(-10),
            KeyCode::Char('f') => self.log_offset = 0,
            _ => {}
        }
        KeyOutcome::Continue
    }

    fn on_cmdline_key(&mut self, key: KeyEvent, tx: &UnboundedSender<UiEvent>) -> KeyOutcome {
        match key.code {
            KeyCode::Enter => {
                self.focus = Focus::Versions;
                self.launch_selected(tx);
            }
            KeyCode::Esc | KeyCode::Tab => self.focus = Focus::Versions,
            KeyCode::Backspace => self.edit_backspace(),
            KeyCode::Delete => self.edit_delete(),
            KeyCode::Left => self.edit_move(-1),
            KeyCode::Right => self.edit_move(1),
            KeyCode::Home => self.cmdline_cursor = 0,
            KeyCode::End => self.cmdline_cursor = self.cmdline.chars().count(),
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.refresh_cmdline(tx);
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.cmdline.clear();
                self.cmdline_cursor = 0;
                self.cmdline_dirty = true;
            }
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.edit_insert(ch);
            }
            _ => {}
        }
        KeyOutcome::Continue
    }

    pub fn on_mouse(&mut self, mouse: MouseEvent, tx: &UnboundedSender<UiEvent>) -> KeyOutcome {
        tracing::debug!(?mouse.kind, column = mouse.column, row = mouse.row, "mouse");

        if !self.size_ok {
            return KeyOutcome::Continue;
        }

        if self.menu.open {
            let options_screen = self.menu.screen == MenuScreen::Options;
            let plugins_screen = self.menu.screen == MenuScreen::Plugins;
            let actions_screen = self.menu.screen == MenuScreen::Actions;
            match mouse.kind {
                MouseEventKind::ScrollUp => {
                    if options_screen {
                        self.menu.move_option(-1);
                    } else if plugins_screen {
                        self.menu.move_plugin(-1);
                    } else if actions_screen {
                        self.move_action(-1);
                    } else {
                        self.menu.move_selection(-1);
                    }
                }
                MouseEventKind::ScrollDown => {
                    if options_screen {
                        self.menu.move_option(1);
                    } else if plugins_screen {
                        self.menu.move_plugin(1);
                    } else if actions_screen {
                        self.move_action(1);
                    } else {
                        self.menu.move_selection(1);
                    }
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    if options_screen {
                        if self.menu.option_editing.is_none() {
                            if let Some(index) = self.option_at(mouse.column, mouse.row) {
                                self.menu.option_selected = index;
                                self.start_option_edit();
                            }
                        }
                    } else if plugins_screen {
                        if let Some(index) = self.plugin_at(mouse.column, mouse.row) {
                            if self.menu.plugin_selected == index {
                                self.toggle_plugin(tx);
                            } else {
                                self.menu.plugin_selected = index;
                            }
                        }
                    } else if actions_screen {
                        if let Some(index) = self.action_at(mouse.column, mouse.row) {
                            if self.menu.action_selected == index {
                                self.run_selected_action(tx);
                            } else {
                                self.menu.action_selected = index;
                            }
                        }
                    } else if let Some(item) = self.menu_item_at(mouse.column, mouse.row) {
                        return self.activate_menu_item(item, tx);
                    }
                }
                _ => {}
            }
            return KeyOutcome::Continue;
        }

        if self.help {
            if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
                self.help = false;
            }
            return KeyOutcome::Continue;
        }

        if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) {
            if let Some(action) = self.status_action_at(mouse.column, mouse.row) {
                return self.run_status_action(action, tx);
            }
        }

        let inside_versions = in_rect(self.areas.versions, mouse.column, mouse.row);
        let inside_cmdline = in_rect(self.areas.cmdline, mouse.column, mouse.row);
        let inside_logs = in_rect(self.areas.logs, mouse.column, mouse.row);

        match mouse.kind {
            MouseEventKind::ScrollUp => {
                if inside_versions {
                    self.move_selection(-3, tx);
                } else if inside_logs {
                    self.scroll_logs(3);
                }
            }
            MouseEventKind::ScrollDown => {
                if inside_versions {
                    self.move_selection(3, tx);
                } else if inside_logs {
                    self.scroll_logs(-3);
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if inside_cmdline {
                    self.focus_cmdline();
                    self.status = "editing command line (Enter runs, Esc leaves)".to_string();
                } else if inside_versions {
                    self.focus = Focus::Versions;
                    if let Some(index) = self.list_index_at(mouse.row) {
                        let now = Instant::now();
                        let double_click = self
                            .last_click
                            .map(|(at, last)| {
                                last == index && now.duration_since(at) < Duration::from_millis(400)
                            })
                            .unwrap_or(false);

                        self.select_and_refresh(index, tx);
                        if double_click {
                            self.last_click = None;
                            self.launch_selected(tx);
                        } else {
                            self.last_click = Some((now, index));
                            if let Some(version) = self.versions.get(index) {
                                self.status = format!("selected {}", version.id);
                            }
                        }
                    }
                } else if inside_logs {
                    // Clicking the log pane resumes follow mode.
                    self.log_offset = 0;
                }
            }
            _ => {}
        }

        KeyOutcome::Continue
    }

    fn status_action_at(&self, column: u16, row: u16) -> Option<StatusAction> {
        self.status_hits
            .iter()
            .find(|(rect, _)| in_rect(*rect, column, row))
            .map(|(_, action)| *action)
    }

    fn run_status_action(
        &mut self,
        action: StatusAction,
        tx: &UnboundedSender<UiEvent>,
    ) -> KeyOutcome {
        tracing::debug!(?action, "status action");
        match action {
            StatusAction::Menu => self.menu.reset(),
            StatusAction::Launch => self.launch_selected(tx),
            StatusAction::Install => self.install_selected(tx),
            StatusAction::EditCmd => self.focus_cmdline(),
            StatusAction::Filter => self.searching = true,
            StatusAction::Stop => self.stop(),
            StatusAction::Help => self.help = true,
            StatusAction::Quit => return KeyOutcome::Quit,
            StatusAction::Apply => {
                self.searching = false;
                self.refresh_cmdline(tx);
            }
            StatusAction::Clear => {
                self.searching = false;
                self.filter.clear();
                self.apply_filter();
                self.refresh_cmdline(tx);
            }
            StatusAction::Run => self.launch_selected(tx),
            StatusAction::Done => self.focus = Focus::Versions,
            StatusAction::ResetCmd => self.refresh_cmdline(tx),
        }
        KeyOutcome::Continue
    }

    fn on_menu_key(&mut self, key: KeyEvent, tx: &UnboundedSender<UiEvent>) -> KeyOutcome {
        if self.menu.screen == MenuScreen::Options {
            return self.on_options_key(key, tx);
        }
        if self.menu.screen == MenuScreen::Plugins {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.menu.screen = MenuScreen::Main;
                    self.menu.selected = 0;
                }
                KeyCode::Up | KeyCode::Char('k') => self.menu.move_plugin(-1),
                KeyCode::Down | KeyCode::Char('j') => self.menu.move_plugin(1),
                KeyCode::Enter | KeyCode::Char(' ') => self.toggle_plugin(tx),
                _ => {}
            }
            return KeyOutcome::Continue;
        }
        if self.menu.screen == MenuScreen::Actions {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => {
                    self.menu.screen = MenuScreen::Main;
                    self.menu.selected = 0;
                }
                KeyCode::Up | KeyCode::Char('k') => self.move_action(-1),
                KeyCode::Down | KeyCode::Char('j') => self.move_action(1),
                KeyCode::Enter => self.run_selected_action(tx),
                _ => {}
            }
            return KeyOutcome::Continue;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                if self.menu.screen == MenuScreen::Main {
                    self.menu.open = false;
                } else {
                    self.menu.screen = MenuScreen::Main;
                    self.menu.selected = 0;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => self.menu.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.menu.move_selection(1),
            KeyCode::Enter => {
                if self.menu.screen == MenuScreen::Main {
                    if let Some(item) = self.menu.current() {
                        return self.activate_menu_item(item, tx);
                    }
                } else {
                    self.menu.screen = MenuScreen::Main;
                    self.menu.selected = 0;
                }
            }
            _ => {}
        }
        KeyOutcome::Continue
    }

    fn on_options_key(&mut self, key: KeyEvent, tx: &UnboundedSender<UiEvent>) -> KeyOutcome {
        if self.menu.option_editing.is_some() {
            match key.code {
                KeyCode::Enter => self.commit_option(tx),
                KeyCode::Esc => {
                    self.menu.option_editing = None;
                    self.menu.option_error = None;
                }
                KeyCode::Backspace => {
                    self.menu.option_buffer.pop();
                    self.menu.option_error = None;
                }
                KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.menu.option_buffer.push(ch);
                    self.menu.option_error = None;
                }
                _ => {}
            }
            return KeyOutcome::Continue;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.menu.screen = MenuScreen::Main;
                self.menu.selected = 0;
            }
            KeyCode::Up | KeyCode::Char('k') => self.menu.move_option(-1),
            KeyCode::Down | KeyCode::Char('j') => self.menu.move_option(1),
            KeyCode::Enter => self.start_option_edit(),
            _ => {}
        }
        KeyOutcome::Continue
    }

    fn start_option_edit(&mut self) {
        let Some(field) = self.menu.current_option() else {
            return;
        };
        self.menu.option_buffer = match field {
            OptionField::Username => self.opts.username.clone(),
            OptionField::Memory => self.opts.memory_mb.to_string(),
            OptionField::Java => self
                .opts
                .java
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
        };
        self.menu.option_editing = Some(field);
        self.menu.option_error = None;
    }

    fn commit_option(&mut self, tx: &UnboundedSender<UiEvent>) {
        let Some(field) = self.menu.option_editing else {
            return;
        };
        let value = self.menu.option_buffer.trim().to_string();

        let error = match field {
            OptionField::Username => {
                if value.is_empty() {
                    Some("username cannot be empty".to_string())
                } else {
                    self.opts.username = value;
                    None
                }
            }
            OptionField::Memory => match value.parse::<u32>() {
                Ok(memory) if (512..=32768).contains(&memory) => {
                    self.opts.memory_mb = memory;
                    None
                }
                Ok(_) => Some("memory must be 512..32768 MB".to_string()),
                Err(_) => Some("memory must be a number".to_string()),
            },
            OptionField::Java => {
                if value.is_empty() {
                    self.opts.java = None;
                    None
                } else if Path::new(&value).is_file() {
                    self.opts.java = Some(PathBuf::from(&value));
                    None
                } else {
                    Some("java path does not exist".to_string())
                }
            }
        };

        if let Some(message) = error {
            self.menu.option_error = Some(message);
            return;
        }
        if let Err(err) = self.opts.save(&self.manager.root.root) {
            self.menu.option_error = Some(format!("save: {err}"));
            return;
        }

        self.menu.option_editing = None;
        self.menu.option_error = None;
        if !self.cmdline_dirty {
            self.refresh_cmdline(tx);
        }
    }

    fn option_at(&self, column: u16, row: u16) -> Option<usize> {
        self.menu
            .option_hits
            .iter()
            .find(|(rect, _)| in_rect(*rect, column, row))
            .map(|(_, index)| *index)
    }

    fn plugin_at(&self, column: u16, row: u16) -> Option<usize> {
        self.menu
            .plugin_hits
            .iter()
            .find(|(rect, _)| in_rect(*rect, column, row))
            .map(|(_, index)| *index)
    }

    fn toggle_plugin(&mut self, tx: &UnboundedSender<UiEvent>) {
        let Some(entry) = self.menu.current_plugin().cloned() else {
            return;
        };
        if entry.error.is_some() {
            self.menu.plugin_error = Some("cannot toggle an invalid plugin".to_string());
            return;
        }

        match self.manager.set_plugin_enabled(&entry.name, !entry.enabled) {
            Ok(()) => {
                self.menu.load_plugins(&self.manager.plugins);
                self.menu.plugin_error = None;
                self.sync_plugin_ui();
                if !self.cmdline_dirty {
                    self.refresh_cmdline(tx);
                }
            }
            Err(err) => {
                self.menu.plugin_error = Some(err.to_string());
            }
        }
    }

    fn activate_menu_item(&mut self, item: MenuItem, _tx: &UnboundedSender<UiEvent>) -> KeyOutcome {
        match item {
            MenuItem::Options => {
                self.menu.screen = MenuScreen::Options;
                self.menu.option_selected = 0;
                self.menu.option_editing = None;
                self.menu.option_error = None;
            }
            MenuItem::Plugins => {
                self.menu.load_plugins(&self.manager.plugins);
                self.menu.screen = MenuScreen::Plugins;
            }
            MenuItem::Actions => {
                self.menu.screen = MenuScreen::Actions;
                self.menu.action_selected = 0;
            }
            MenuItem::Exit => return KeyOutcome::Quit,
        }
        KeyOutcome::Continue
    }

    fn action_at(&self, column: u16, row: u16) -> Option<usize> {
        self.menu
            .action_hits
            .iter()
            .find(|(rect, _)| in_rect(*rect, column, row))
            .map(|(_, index)| *index)
    }

    fn move_action(&mut self, delta: isize) {
        if self.plugin_actions.is_empty() {
            return;
        }
        let count = self.plugin_actions.len() as isize;
        self.menu.action_selected =
            (self.menu.action_selected as isize + delta).rem_euclid(count) as usize;
    }

    fn run_selected_action(&mut self, tx: &UnboundedSender<UiEvent>) {
        let Some(action) = self.plugin_actions.get(self.menu.action_selected).cloned() else {
            self.status = "no plugin actions".to_string();
            return;
        };
        self.menu.open = false;
        self.run_plugin_action(&action, tx);
    }

    fn run_plugin_action(&mut self, action: &PluginAction, tx: &UnboundedSender<UiEvent>) {
        match action.action.as_str() {
            "message" => {
                let text = action.value.clone().unwrap_or_default();
                let message = format!("[{}] {text}", action.plugin);
                self.status = message.clone();
                self.push_system(LogLevel::Info, message);
            }
            "launch" => self.launch_selected(tx),
            "install" => self.install_selected(tx),
            "refresh" => self.load_versions(tx),
            "open-data-dir" => {
                let root = self.manager.root.root.clone();
                self.open_path(&root);
            }
            "url" => match &action.value {
                Some(url) => self.open_path(Path::new(url)),
                None => self.status = format!("[{}] url action needs a value", action.plugin),
            },
            "command" => match &action.value {
                Some(command) => self.run_shell(command),
                None => self.status = format!("[{}] command action needs a value", action.plugin),
            },
            other => self.status = format!("[{}] unknown action: {other}", action.plugin),
        }
    }

    fn open_path(&mut self, path: &Path) {
        #[cfg(target_os = "windows")]
        let spawned = {
            let mut command = std::process::Command::new("cmd");
            command.args(["/C", "start", ""]).arg(path);
            command.spawn()
        };
        #[cfg(target_os = "macos")]
        let spawned = std::process::Command::new("open").arg(path).spawn();
        #[cfg(all(unix, not(target_os = "macos")))]
        let spawned = std::process::Command::new("xdg-open").arg(path).spawn();

        match spawned {
            Ok(_) => self.status = format!("opened {}", path.display()),
            Err(err) => self.status = format!("open failed: {err}"),
        }
    }

    fn run_shell(&mut self, command: &str) {
        #[cfg(windows)]
        let spawned = std::process::Command::new("cmd")
            .args(["/C", command])
            .spawn();
        #[cfg(not(windows))]
        let spawned = std::process::Command::new("sh")
            .args(["-c", command])
            .spawn();

        match spawned {
            Ok(_) => self.status = format!("started: {command}"),
            Err(err) => self.status = format!("command failed: {err}"),
        }
    }

    fn menu_item_at(&self, column: u16, row: u16) -> Option<MenuItem> {
        self.menu
            .hits
            .iter()
            .find(|(rect, _)| in_rect(*rect, column, row))
            .map(|(_, item)| *item)
    }

    pub fn shutdown(&mut self) {
        if let Some(game) = &mut self.game {
            game.kill();
        }
    }

    pub fn focus_cmdline(&mut self) {
        self.searching = false;
        self.focus = Focus::Cmdline;
        self.cmdline_cursor = self.cmdline.chars().count();
    }

    fn selected_summary(&self) -> Option<VersionSummary> {
        self.versions.get(self.selected).cloned()
    }

    fn current_version_id(&self) -> Option<String> {
        self.versions
            .get(self.selected)
            .map(|version| version.id.clone())
    }

    fn list_index_at(&self, row: u16) -> Option<usize> {
        list_index_at(
            self.areas.versions,
            self.list_state.offset(),
            self.versions.len(),
            row,
        )
    }

    fn apply_filter(&mut self) {
        let needle = self.filter.to_lowercase();
        self.versions = self
            .all_versions
            .iter()
            .filter(|version| needle.is_empty() || version.id.to_lowercase().contains(&needle))
            .cloned()
            .collect();
        self.selected = 0;
        self.list_state
            .select((!self.versions.is_empty()).then_some(0));
    }

    fn select(&mut self, index: usize) {
        if self.versions.is_empty() {
            return;
        }
        self.selected = index.min(self.versions.len() - 1);
        self.list_state.select(Some(self.selected));
    }

    fn select_and_refresh(&mut self, index: usize, tx: &UnboundedSender<UiEvent>) {
        self.select(index);
        self.refresh_cmdline(tx);
    }

    fn move_selection(&mut self, delta: isize, tx: &UnboundedSender<UiEvent>) {
        if self.versions.is_empty() {
            return;
        }
        let max = self.versions.len() as isize - 1;
        let next = (self.selected as isize + delta).clamp(0, max) as usize;
        self.select_and_refresh(next, tx);
    }

    fn scroll_logs(&mut self, delta: isize) {
        let max = self.logs.len().saturating_sub(1) as isize;
        self.log_offset = (self.log_offset as isize + delta).clamp(0, max) as usize;
    }

    fn edit_insert(&mut self, ch: char) {
        let mut chars: Vec<char> = self.cmdline.chars().collect();
        let position = self.cmdline_cursor.min(chars.len());
        chars.insert(position, ch);
        self.cmdline = chars.into_iter().collect();
        self.cmdline_cursor = position + 1;
        self.cmdline_dirty = true;
    }

    fn edit_backspace(&mut self) {
        if self.cmdline_cursor == 0 {
            return;
        }
        let mut chars: Vec<char> = self.cmdline.chars().collect();
        let position = (self.cmdline_cursor - 1).min(chars.len());
        if position < chars.len() {
            chars.remove(position);
            self.cmdline = chars.into_iter().collect();
            self.cmdline_cursor = position;
            self.cmdline_dirty = true;
        }
    }

    fn edit_delete(&mut self) {
        let mut chars: Vec<char> = self.cmdline.chars().collect();
        if self.cmdline_cursor < chars.len() {
            chars.remove(self.cmdline_cursor);
            self.cmdline = chars.into_iter().collect();
            self.cmdline_dirty = true;
        }
    }

    fn edit_move(&mut self, delta: isize) {
        let length = self.cmdline.chars().count();
        self.cmdline_cursor =
            (self.cmdline_cursor as isize + delta).clamp(0, length as isize) as usize;
    }

    fn push_log(&mut self, line: LogLine) {
        if self.logs.len() >= MAX_LOG_LINES {
            self.logs.pop_front();
        }
        self.logs.push_back(line);
    }

    fn push_system(&mut self, level: LogLevel, text: String) {
        self.push_log(LogLine {
            ts: 0,
            level,
            source: LogSource::Wrapper,
            text,
        });
    }
}

/// Map a screen row inside the bordered list to a version index. `offset` is
/// the list's scroll offset from `ListState`.
fn list_index_at(area: Rect, offset: usize, len: usize, row: u16) -> Option<usize> {
    let top = area.y.saturating_add(1);
    let bottom = area.y.saturating_add(area.height).saturating_sub(1);
    if row < top || row >= bottom {
        return None;
    }
    let visible = (row - top) as usize;
    let index = offset + visible;
    (index < len).then_some(index)
}

fn in_rect(area: Rect, column: u16, row: u16) -> bool {
    area.width > 0
        && area.height > 0
        && column >= area.x
        && column < area.x.saturating_add(area.width)
        && row >= area.y
        && row < area.y.saturating_add(area.height)
}

fn forward_progress(
    mut rx: mpsc::UnboundedReceiver<DownloadProgress>,
    tx: UnboundedSender<UiEvent>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(progress) = rx.recv().await {
            let _ = tx.send(UiEvent::Progress(progress));
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clicks_map_to_list_rows() {
        // Border occupies rows 0 and 9; content is rows 1..=8.
        let area = Rect::new(0, 0, 40, 10);
        assert_eq!(list_index_at(area, 0, 100, 1), Some(0));
        assert_eq!(list_index_at(area, 0, 100, 8), Some(7));
        assert_eq!(list_index_at(area, 0, 100, 0), None);
        assert_eq!(list_index_at(area, 0, 100, 9), None);
        // Scrolled list: row 1 is the sixth item.
        assert_eq!(list_index_at(area, 5, 100, 1), Some(5));
        // Clicking past the end of a short list selects nothing.
        assert_eq!(list_index_at(area, 0, 3, 5), None);
    }

    #[test]
    fn rect_hit_testing() {
        let area = Rect::new(10, 5, 20, 8);
        assert!(in_rect(area, 10, 5));
        assert!(in_rect(area, 29, 12));
        assert!(!in_rect(area, 9, 5));
        assert!(!in_rect(area, 30, 5));
        assert!(!in_rect(area, 10, 13));
    }
}
