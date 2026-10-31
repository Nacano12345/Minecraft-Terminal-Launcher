use mc_core::{CrashInfo, LogLevel, LogLine, LogSource};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn line(raw: &str, source: LogSource) -> Option<LogLine> {
    let text = strip_ansi(raw);
    if text.trim().is_empty() {
        return None;
    }
    Some(LogLine {
        ts: now_ms(),
        level: detect_level(&text),
        source,
        text,
    })
}

pub fn detect_level(text: &str) -> LogLevel {
    let upper = text.to_ascii_uppercase();
    if upper.contains("EXCEPTION IN THREAD")
        || upper.contains("FATAL")
        || upper.contains("ERROR]")
        || upper.contains("ERROR:")
        || upper.contains("OUTOFMEMORYERROR")
    {
        LogLevel::Error
    } else if upper.contains("WARN") {
        LogLevel::Warn
    } else if upper.contains("DEBUG") {
        LogLevel::Debug
    } else if upper.contains("TRACE") {
        LogLevel::Trace
    } else {
        LogLevel::Info
    }
}

/// Best-effort crash detection: JVM fatal logs and Forge/NeoForge reports.
pub fn detect_crash(exit_code: Option<i32>, game_dir: &Path) -> Option<CrashInfo> {
    let hs_err = newest_with_prefix(game_dir, "hs_err_pid", "log");
    let crash_report = newest_in_dir(&game_dir.join("crash-reports"), "txt");

    if hs_err.is_none() && crash_report.is_none() && exit_code.unwrap_or(0) == 0 {
        return None;
    }
    if hs_err.is_none() && crash_report.is_none() && exit_code.unwrap_or(0) != 0 {
        // A non-zero exit without artifacts is still worth surfacing.
        return Some(CrashInfo {
            exit_code,
            reason: "process exited with a non-zero status".to_string(),
            hs_err: None,
            crash_report: None,
        });
    }

    Some(CrashInfo {
        exit_code,
        reason: if hs_err.is_some() {
            "JVM crashed (hs_err log found)".to_string()
        } else {
            "game crashed (crash report found)".to_string()
        },
        hs_err,
        crash_report,
    })
}

fn newest_with_prefix(dir: &Path, prefix: &str, extension: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.starts_with(prefix) && name.ends_with(extension))
                .unwrap_or(false)
        })
        .max_by_key(|path| path.metadata().and_then(|m| m.modified()).ok())
}

fn newest_in_dir(dir: &Path, extension: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some(extension))
        .max_by_key(|path| path.metadata().and_then(|m| m.modified()).ok())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

/// Remove ANSI escape sequences so log text is safe to store and re-render.
pub fn strip_ansi(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for next in chars.by_ref() {
                if ('\u{40}'..='\u{7e}').contains(&next) {
                    break;
                }
            }
            continue;
        }
        output.push(ch);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_colors() {
        assert_eq!(strip_ansi("\u{1b}[32mhello\u{1b}[0m"), "hello");
    }

    #[test]
    fn levels() {
        assert_eq!(detect_level("[12:00:00] [main/WARN]: x"), LogLevel::Warn);
        assert_eq!(
            detect_level("Exception in thread \"main\" java.lang.RuntimeException"),
            LogLevel::Error
        );
        assert_eq!(
            detect_level("[12:00:00] [main/INFO]: hello"),
            LogLevel::Info
        );
    }
}
