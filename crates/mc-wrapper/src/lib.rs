//! Wrapper: the java process host.
//!
//! Owns process creation, stdio bridging and lifecycle. It knows nothing about
//! versions or downloads; it just executes a [`LaunchPlan`].

pub mod classify;

use mc_core::{LaunchPlan, ProcessState, Result};
use mc_ipc::Event;
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::Notify;

pub struct RunningGame {
    pub session: String,
    child: Child,
    stdin: Option<ChildStdin>,
    tx: UnboundedSender<Event>,
    /// Number of stdout/stderr pumps still running.
    pumps_left: Arc<AtomicUsize>,
    pump_notify: Arc<Notify>,
    /// Set once the child has been reaped, so `try_wait` can wait for pumps.
    exit_seen: Option<tokio::time::Instant>,
    exit_status: Option<std::process::ExitStatus>,
}

impl RunningGame {
    /// Write one line to the game's stdin (chat for the client, console for a
    /// server).
    pub async fn send(&mut self, line: &str) -> Result<()> {
        if let Some(stdin) = &mut self.stdin {
            stdin.write_all(line.as_bytes()).await?;
            if !line.ends_with('\n') {
                stdin.write_all(b"\n").await?;
            }
            stdin.flush().await?;
        }
        Ok(())
    }

    /// Graceful stop, then kill after `grace`.
    pub async fn stop(&mut self, command: &str, grace: Duration) -> Result<()> {
        let _ = self.tx.send(Event::ProcessState {
            session: self.session.clone(),
            state: ProcessState::Stopping,
        });
        let _ = self.send(command).await;
        match tokio::time::timeout(grace, self.child.wait()).await {
            Ok(status) => {
                tracing::info!(?status, "process stopped");
            }
            Err(_) => {
                tracing::warn!("graceful stop timed out, killing");
                let _ = self.child.kill().await;
            }
        }
        Ok(())
    }

    /// Wait for the process, then for both output pumps to flush. This
    /// guarantees every buffered log line has been queued as an event before
    /// the caller observes the exit code.
    pub async fn wait(&mut self) -> Result<Option<i32>> {
        let status = self.child.wait().await?;
        let code = status.code();

        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while self.pumps_left.load(Ordering::SeqCst) > 0 {
            if tokio::time::timeout_at(deadline, self.pump_notify.notified())
                .await
                .is_err()
            {
                tracing::warn!("timed out waiting for output pumps to flush");
                break;
            }
        }
        Ok(code)
    }

    /// Non-blocking variant for UI loops: returns `Some(status)` only after
    /// the process exited *and* the output pumps flushed (2s cap).
    pub fn try_wait(&mut self) -> Result<Option<std::process::ExitStatus>> {
        if self.exit_seen.is_none() {
            if let Some(status) = self.child.try_wait()? {
                self.exit_seen = Some(tokio::time::Instant::now());
                self.exit_status = Some(status);
            }
        }

        let Some(seen_at) = self.exit_seen else {
            return Ok(None);
        };
        if self.pumps_left.load(Ordering::SeqCst) == 0
            || seen_at.elapsed() >= Duration::from_secs(2)
        {
            return Ok(self.exit_status);
        }
        Ok(None)
    }

    pub fn kill(&mut self) {
        let _ = self.child.start_kill();
    }
}

/// Spawn the game and stream its output as [`Event`]s.
pub async fn spawn(
    plan: &LaunchPlan,
    session: impl Into<String>,
    tx: UnboundedSender<Event>,
) -> Result<RunningGame> {
    let parts = plan.command_parts();
    let Some((program, args)) = parts.split_first() else {
        return Err(mc_core::Error::Other("empty launch plan".to_string()));
    };
    spawn_raw(program, args, &plan.working_dir, &plan.env, session, tx).await
}

/// Spawn an arbitrary program: used when the TUI command line has been edited.
pub async fn spawn_raw(
    program: &str,
    args: &[String],
    working_dir: &std::path::Path,
    env: &std::collections::BTreeMap<String, String>,
    session: impl Into<String>,
    tx: UnboundedSender<Event>,
) -> Result<RunningGame> {
    let session = session.into();
    let _ = tx.send(Event::ProcessState {
        session: session.clone(),
        state: ProcessState::Starting,
    });

    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(working_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    for (key, value) in env {
        command.env(key, value);
    }

    tracing::info!(
        program,
        cwd = %working_dir.display(),
        "spawning game"
    );

    let mut child = command.spawn()?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdin = child.stdin.take();

    let pumps_left = Arc::new(AtomicUsize::new(
        (stdout.is_some() as usize) + (stderr.is_some() as usize),
    ));
    let pump_notify = Arc::new(Notify::new());

    if let Some(stdout) = stdout {
        let tx = tx.clone();
        let session = session.clone();
        let pumps_left = pumps_left.clone();
        let pump_notify = pump_notify.clone();
        tokio::spawn(async move {
            pump(
                stdout,
                mc_core::LogSource::Game,
                session,
                tx,
                pumps_left,
                pump_notify,
            )
            .await;
        });
    }
    if let Some(stderr) = stderr {
        let tx = tx.clone();
        let session = session.clone();
        let pumps_left = pumps_left.clone();
        let pump_notify = pump_notify.clone();
        tokio::spawn(async move {
            pump(
                stderr,
                mc_core::LogSource::Jvm,
                session,
                tx,
                pumps_left,
                pump_notify,
            )
            .await;
        });
    }

    let _ = tx.send(Event::ProcessState {
        session: session.clone(),
        state: ProcessState::Running,
    });

    Ok(RunningGame {
        session,
        child,
        stdin,
        tx,
        pumps_left,
        pump_notify,
        exit_seen: None,
        exit_status: None,
    })
}

async fn pump<R>(
    reader: R,
    source: mc_core::LogSource,
    session: String,
    tx: UnboundedSender<Event>,
    pumps_left: Arc<AtomicUsize>,
    pump_notify: Arc<Notify>,
) where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut lines = BufReader::new(reader).lines();
    loop {
        match lines.next_line().await {
            Ok(Some(raw)) => {
                if let Some(line) = classify::line(&raw, source) {
                    let _ = tx.send(Event::LogLine {
                        session: session.clone(),
                        line,
                    });
                }
            }
            Ok(None) => break,
            Err(err) => {
                tracing::debug!(%err, "stream read error");
                break;
            }
        }
    }

    if pumps_left.fetch_sub(1, Ordering::SeqCst) == 1 {
        pump_notify.notify_one();
    }
}
