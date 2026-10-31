use crate::menu::{MenuItem, MenuScreen, OptionField, LOGO, VERSION};
use crate::{App, Focus, StatusAction};
use mc_core::LogLevel;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Gauge, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const NORMAL_HINTS: &[(&str, StatusAction)] = &[
    ("[Esc] menu", StatusAction::Menu),
    ("[Enter] launch", StatusAction::Launch),
    ("[i] install", StatusAction::Install),
    ("[e] cmdline", StatusAction::EditCmd),
    ("[/] filter", StatusAction::Filter),
    ("[s] stop", StatusAction::Stop),
    ("[?] help", StatusAction::Help),
    ("[q] quit", StatusAction::Quit),
];

const SEARCH_HINTS: &[(&str, StatusAction)] = &[
    ("[Enter] apply", StatusAction::Apply),
    ("[Esc] clear", StatusAction::Clear),
];

const CMDLINE_HINTS: &[(&str, StatusAction)] = &[
    ("[Enter] run", StatusAction::Run),
    ("[Esc] done", StatusAction::Done),
    ("[Ctrl+R] reset", StatusAction::ResetCmd),
];

/// Screen regions from the last draw, used for mouse hit-testing.
#[derive(Debug, Clone, Copy, Default)]
pub struct Areas {
    pub versions: Rect,
    pub cmdline: Rect,
    pub logs: Rect,
}

/// Minimum terminal size for the three-pane layout.
pub const MIN_WIDTH: u16 = 80;
pub const MIN_HEIGHT: u16 = 24;

pub fn is_too_small(area: Rect) -> bool {
    area.width < MIN_WIDTH || area.height < MIN_HEIGHT
}

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    if is_too_small(area) {
        app.size_ok = false;
        app.areas = Areas::default();
        app.status_hits.clear();
        app.menu.hits.clear();
        draw_too_small(frame, area);
        return;
    }
    app.size_ok = true;

    let mut constraints = vec![Constraint::Min(3)];
    if app.progress.is_some() {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Length(1));
    let rows = Layout::vertical(constraints).split(area);

    let body =
        Layout::horizontal([Constraint::Percentage(36), Constraint::Percentage(64)]).split(rows[0]);
    let left =
        Layout::vertical([Constraint::Percentage(58), Constraint::Percentage(42)]).split(body[0]);
    app.areas = Areas {
        versions: left[0],
        cmdline: left[1],
        logs: body[1],
    };

    draw_versions(frame, app, left[0]);
    draw_cmdline(frame, app, left[1]);
    draw_logs(frame, app, body[1]);

    let status_index = rows.len() - 1;
    if app.progress.is_some() {
        draw_progress(frame, app, rows[status_index - 1]);
    }
    draw_status(frame, app, rows[status_index]);

    if app.help {
        draw_help(frame, area);
    }
    if app.menu.open {
        // Dim everything behind the modal, then Clear+draw the popup at full
        // brightness (Clear resets the cell style).
        dim_background(frame);
        draw_menu(frame, app, area);
    }
}

fn draw_too_small(frame: &mut Frame, area: Rect) {
    let lines = vec![
        Line::from(Span::styled(
            "Terminal too small",
            Style::default()
                .fg(Color::LightRed)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(format!("minimum: {MIN_WIDTH} x {MIN_HEIGHT}")),
        Line::from(format!("current: {} x {}", area.width, area.height)),
        Line::from(""),
        Line::from("Resize the window to continue."),
    ];
    let height = (lines.len() as u16).min(area.height);
    let width = area.width.min(60);
    let rect = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), rect);
}

fn dim_background(frame: &mut Frame) {
    for cell in &mut frame.buffer_mut().content {
        cell.modifier.insert(Modifier::DIM);
        cell.fg = dim_fg(cell.fg);
        cell.bg = dim_bg(cell.bg);
    }
}

fn dim_fg(color: Color) -> Color {
    match color {
        Color::Reset | Color::Gray | Color::White => Color::DarkGray,
        Color::LightRed => Color::Red,
        Color::LightGreen => Color::Green,
        Color::LightYellow => Color::Yellow,
        Color::LightBlue => Color::Blue,
        Color::LightMagenta => Color::Magenta,
        Color::LightCyan => Color::Cyan,
        other => other,
    }
}

fn dim_bg(color: Color) -> Color {
    match color {
        Color::Reset | Color::Black => Color::Reset,
        _ => Color::Black,
    }
}

fn draw_menu(frame: &mut Frame, app: &mut App, area: Rect) {
    let width = area.width.saturating_sub(4).clamp(30, 52);
    let height = area.height.saturating_sub(2).clamp(12, 16);
    let popup = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );

    frame.render_widget(Clear, popup);
    let block = Block::bordered().title(" QCL ");
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    app.menu.hits.clear();

    match app.menu.screen {
        MenuScreen::Main => draw_menu_main(frame, app, inner),
        MenuScreen::Options => draw_menu_options(frame, app, inner),
        MenuScreen::Plugins => draw_menu_plugins(frame, app, inner),
        MenuScreen::Actions => draw_menu_actions(frame, app, inner),
    }
}

fn draw_menu_actions(frame: &mut Frame, app: &mut App, inner: Rect) {
    app.menu.action_hits.clear();

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("Plugin Actions ({})", app.plugin_actions.len()),
            Style::default().add_modifier(Modifier::BOLD),
        ))),
        rows[0],
    );

    if app.plugin_actions.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "no actions from enabled plugins",
                Style::default().fg(Color::DarkGray),
            ))),
            rows[1],
        );
    } else {
        let height = rows[1].height as usize;
        let offset = app
            .menu
            .action_selected
            .saturating_sub(height.saturating_sub(1));

        for (visible, index) in (offset..app.plugin_actions.len()).take(height).enumerate() {
            let action = &app.plugin_actions[index];
            let key = action
                .key
                .map(|key| format!("[{key}] "))
                .unwrap_or_default();
            let text = truncate_width(
                &format!("{key}{}  · {}", action.label, action.plugin),
                inner.width,
            );
            let selected = index == app.menu.action_selected;
            let style = if selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            let rect = Rect::new(inner.x, rows[1].y + visible as u16, inner.width, 1);
            frame.render_widget(Paragraph::new(Line::from(Span::styled(text, style))), rect);
            app.menu.action_hits.push((rect, index));
        }
    }

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "Enter run · Esc back",
            Style::default().fg(Color::DarkGray),
        )))
        .alignment(Alignment::Center),
        rows[2],
    );
}

fn draw_menu_main(frame: &mut Frame, app: &mut App, inner: Rect) {
    let rows = Layout::vertical([
        Constraint::Length(5),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .split(inner);

    let logo: Vec<Line> = LOGO
        .iter()
        .map(|line| {
            Line::from(Span::styled(
                *line,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ))
        })
        .collect();
    frame.render_widget(Paragraph::new(logo).alignment(Alignment::Center), rows[0]);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "QuarkCraftLauncher",
            Style::default().fg(Color::DarkGray),
        )))
        .alignment(Alignment::Center),
        rows[1],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("v{VERSION}"),
            Style::default().fg(Color::DarkGray),
        )))
        .alignment(Alignment::Center),
        rows[2],
    );

    let item_count = MenuItem::ALL.len() as u16;
    let start_y = rows[4].y + rows[4].height.saturating_sub(item_count) / 2;

    for (index, item) in MenuItem::ALL.iter().enumerate() {
        let selected = index == app.menu.selected;
        let text = if selected {
            format!("▸ {}", item.label())
        } else {
            format!("  {}", item.label())
        };
        let text_width = text.width() as u16;
        let x = inner.x + inner.width.saturating_sub(text_width) / 2;
        let rect = Rect::new(x, start_y + index as u16, text_width, 1);
        let style = if selected {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        frame.render_widget(Paragraph::new(Line::from(Span::styled(text, style))), rect);
        app.menu.hits.push((rect, *item));
    }

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "↑/↓ move · Enter select · Esc close",
            Style::default().fg(Color::DarkGray),
        )))
        .alignment(Alignment::Center),
        rows[5],
    );
}

fn draw_menu_options(frame: &mut Frame, app: &mut App, inner: Rect) {
    app.menu.option_hits.clear();

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("Options · v{VERSION}"),
            Style::default().add_modifier(Modifier::BOLD),
        ))),
        rows[0],
    );

    for (index, field) in OptionField::ALL.iter().enumerate() {
        let selected = index == app.menu.option_selected;
        let value = match field {
            OptionField::Username => app.opts.username.clone(),
            OptionField::Memory => format!("{} MB", app.opts.memory_mb),
            OptionField::Java => app
                .opts
                .java
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "auto".to_string()),
        };
        let marker = if selected { "▸ " } else { "  " };
        let text = truncate_width(
            &format!("{marker}{:<9} {value}", field.label()),
            inner.width,
        );
        let style = if selected {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        let rect = Rect::new(inner.x, rows[1].y + index as u16, inner.width, 1);
        frame.render_widget(Paragraph::new(Line::from(Span::styled(text, style))), rect);
        app.menu.option_hits.push((rect, index));
    }

    if let Some(field) = app.menu.option_editing {
        let label = format!("{}: ", field.label());
        let available = inner.width.saturating_sub(label.width() as u16 + 1);
        let shown = tail_width(&app.menu.option_buffer, available);
        let text = format!("{label}{shown}▌");
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                text,
                Style::default().fg(Color::Yellow),
            ))),
            Rect::new(inner.x, rows[2].y, inner.width, 1),
        );

        let (hint, style) = match &app.menu.option_error {
            Some(error) => (error.clone(), Style::default().fg(Color::LightRed)),
            None => (
                "Enter save · Esc cancel".to_string(),
                Style::default().fg(Color::DarkGray),
            ),
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(hint, style))),
            Rect::new(inner.x, rows[2].y + 1, inner.width, 1),
        );
    } else {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "↑/↓ select · Enter edit · Esc back",
                Style::default().fg(Color::DarkGray),
            )))
            .alignment(Alignment::Center),
            Rect::new(inner.x, rows[2].y, inner.width, 2),
        );
    }
}

fn tail_width(text: &str, max: u16) -> String {
    let max = max as usize;
    let mut chars: Vec<char> = Vec::new();
    let mut used = 0usize;
    for ch in text.chars().rev() {
        let width = ch.width().unwrap_or(0);
        if used + width > max {
            break;
        }
        chars.push(ch);
        used += width;
    }
    chars.reverse();
    chars.into_iter().collect()
}

fn draw_menu_plugins(frame: &mut Frame, app: &mut App, inner: Rect) {
    app.menu.plugin_hits.clear();

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(inner);

    let enabled = app
        .menu
        .plugins
        .iter()
        .filter(|plugin| plugin.enabled)
        .count();
    let total = app.menu.plugins.len();
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("Plugins ({enabled}/{total})"),
            Style::default().add_modifier(Modifier::BOLD),
        ))),
        rows[0],
    );

    if app.menu.plugins.is_empty() {
        let lines = vec![
            Line::from("no plugins found"),
            Line::from(Span::styled(
                app.manager.root.plugins().display().to_string(),
                Style::default().fg(Color::DarkGray),
            )),
        ];
        frame.render_widget(Paragraph::new(lines), rows[1]);
    } else {
        let height = rows[1].height as usize;
        let offset = app
            .menu
            .plugin_selected
            .saturating_sub(height.saturating_sub(1));

        for (visible, index) in (offset..app.menu.plugins.len()).take(height).enumerate() {
            let plugin = &app.menu.plugins[index];
            let marker = if plugin.error.is_some() {
                "!"
            } else if plugin.enabled {
                "x"
            } else {
                " "
            };
            let text = truncate_width(
                &format!(
                    "[{marker}] {:<12} {:<8} {}",
                    plugin.name,
                    plugin.version,
                    plugin.capabilities()
                ),
                inner.width,
            );
            let selected = index == app.menu.plugin_selected;
            let style = if selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else if plugin.error.is_some() {
                Style::default().fg(Color::LightRed)
            } else if plugin.enabled {
                Style::default().fg(Color::Gray)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let rect = Rect::new(inner.x, rows[1].y + visible as u16, inner.width, 1);
            frame.render_widget(Paragraph::new(Line::from(Span::styled(text, style))), rect);
            app.menu.plugin_hits.push((rect, index));
        }
    }

    let (hint, style) = match &app.menu.plugin_error {
        Some(error) => (error.clone(), Style::default().fg(Color::LightRed)),
        None => (
            "Space/Enter toggle · Esc back".to_string(),
            Style::default().fg(Color::DarkGray),
        ),
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(hint, style))).alignment(Alignment::Center),
        rows[2],
    );
}

fn draw_versions(frame: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = app
        .versions
        .iter()
        .map(|version| {
            let running = app.running_version.as_deref() == Some(version.id.as_str());
            let marker = if running { "▶ " } else { "  " };
            ListItem::new(Line::from(vec![
                Span::styled(marker, Style::default().fg(Color::Green)),
                Span::raw(version.id.clone()),
            ]))
        })
        .collect();

    let title = if app.filter.is_empty() {
        format!(" VersionTable ({}) ", app.versions.len())
    } else {
        format!(
            " VersionTable {}/{} ",
            app.versions.len(),
            app.all_versions.len()
        )
    };
    let list = List::new(items)
        .block(Block::bordered().title(title))
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("");

    frame.render_stateful_widget(list, area, &mut app.list_state);
}

fn draw_cmdline(frame: &mut Frame, app: &App, area: Rect) {
    let inner_width = (area.width.saturating_sub(2) as usize).max(1);
    let inner_height = (area.height.saturating_sub(2) as usize).max(1);

    let mut title = " cmdline".to_string();
    if app.focus == Focus::Cmdline {
        title.push_str(" [EDIT]");
    }
    if app.cmdline_dirty {
        title.push('*');
    }
    title.push(' ');
    let block = Block::bordered().title(title);

    if app.cmdline.is_empty() {
        let hint = Paragraph::new("select a version to build the command line")
            .block(block)
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(hint, area);
        return;
    }

    let layout = wrap_cmdline(&app.cmdline, app.cmdline_cursor, inner_width);
    let show_cursor = app.focus == Focus::Cmdline;
    // Preview shows the command from the start; editing keeps the cursor in
    // view (which is usually at the end).
    let offset = if show_cursor {
        layout
            .cursor_row
            .saturating_sub(inner_height.saturating_sub(1))
    } else {
        0
    };

    let mut lines: Vec<Line> = Vec::new();
    for (row_index, row) in layout
        .rows
        .iter()
        .enumerate()
        .skip(offset)
        .take(inner_height)
    {
        if show_cursor && row_index == layout.cursor_row {
            let col = layout.cursor_col.min(row.len());
            let before: String = row[..col].iter().collect();
            let at: String = row
                .get(col)
                .map(|ch| ch.to_string())
                .unwrap_or_else(|| " ".to_string());
            let after: String = if col < row.len() {
                row[col + 1..].iter().collect()
            } else {
                String::new()
            };
            let mut spans = Vec::new();
            if !before.is_empty() {
                spans.push(Span::raw(before));
            }
            spans.push(Span::styled(
                at,
                Style::default().add_modifier(Modifier::REVERSED),
            ));
            if !after.is_empty() {
                spans.push(Span::raw(after));
            }
            lines.push(Line::from(spans));
        } else {
            lines.push(Line::from(row.iter().collect::<String>()));
        }
    }

    frame.render_widget(Paragraph::new(lines).block(block), area);
}

struct CmdlineLayout {
    rows: Vec<Vec<char>>,
    cursor_row: usize,
    cursor_col: usize,
}

fn wrap_cmdline(text: &str, cursor: usize, width: usize) -> CmdlineLayout {
    let chars: Vec<char> = text.chars().collect();
    let width = width.max(1);
    let cursor = cursor.min(chars.len());
    let mut rows: Vec<Vec<char>> = vec![Vec::new()];
    let mut col = 0usize;
    let mut cursor_pos = (0usize, 0usize);

    for (index, &ch) in chars.iter().enumerate() {
        let w = ch.width().unwrap_or(0);
        if col + w > width {
            rows.push(Vec::new());
            col = 0;
        }
        if index == cursor {
            cursor_pos = (rows.len() - 1, col);
        }
        rows.last_mut().expect("row exists").push(ch);
        col += w;
    }

    if cursor >= chars.len() {
        cursor_pos = (rows.len() - 1, col);
    }
    if cursor_pos.1 >= width {
        rows.push(Vec::new());
        cursor_pos = (rows.len() - 1, 0);
    }

    CmdlineLayout {
        rows,
        cursor_row: cursor_pos.0,
        cursor_col: cursor_pos.1,
    }
}

fn draw_logs(frame: &mut Frame, app: &App, area: Rect) {
    let inner_height = (area.height.saturating_sub(2) as usize).max(1);
    let total = app.logs.len();
    let end = total.saturating_sub(app.log_offset);
    let start = end.saturating_sub(inner_height);
    let mut lines: Vec<Line> = app
        .logs
        .iter()
        .skip(start)
        .take(end - start)
        .map(|log| {
            Line::from(Span::styled(
                log.text.clone(),
                Style::default().fg(level_color(log.level)),
            ))
        })
        .collect();

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "select a version and press Enter to launch",
            Style::default().fg(Color::DarkGray),
        )));
    }

    let mut title = match &app.running_version {
        Some(version) => format!(" logs · {version} "),
        None => " logs ".to_string(),
    };
    if app.log_offset > 0 {
        title.push_str(&format!("· paused ({} lines back) ", app.log_offset));
    }

    let paragraph = Paragraph::new(lines)
        .block(Block::bordered().title(title))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn draw_progress(frame: &mut Frame, app: &App, area: Rect) {
    let Some(progress) = &app.progress else {
        return;
    };
    let ratio = if progress.total == 0 {
        0.0
    } else {
        progress.done as f64 / progress.total as f64
    };
    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(Color::Green).bg(Color::DarkGray))
        .ratio(ratio.clamp(0.0, 1.0))
        .label(format!(
            "{} {}/{}",
            progress.label, progress.done, progress.total
        ));
    frame.render_widget(gauge, area);
}

fn draw_status(frame: &mut Frame, app: &mut App, area: Rect) {
    let (left, style, hints) = if app.focus == Focus::Cmdline {
        (
            format!(" {} ", app.status),
            Style::default().fg(Color::Black).bg(Color::Magenta),
            CMDLINE_HINTS,
        )
    } else if app.searching {
        (
            format!(" /{}▌ ", app.filter),
            Style::default().fg(Color::Black).bg(Color::Yellow),
            SEARCH_HINTS,
        )
    } else {
        (
            format!(" {} ", app.status),
            Style::default().fg(Color::Black).bg(Color::Cyan),
            NORMAL_HINTS,
        )
    };

    // Right-align the hint bar so button positions never depend on how long
    // the status message happens to be.
    let hints_width = hints
        .iter()
        .map(|(label, _)| label.width() as u16 + 2)
        .sum::<u16>()
        .saturating_sub(2);
    let hints_x = area.x + area.width.saturating_sub(hints_width);
    let chip_max = hints_x.saturating_sub(area.x).saturating_sub(2);
    let main = truncate_width(&left, chip_max);
    let main_width = main.width() as u16;

    let plugin_text = if app.searching || app.focus == Focus::Cmdline {
        String::new()
    } else {
        app.plugin_status_text()
    };
    let plugin_available = chip_max.saturating_sub(main_width);
    let plugin = if !plugin_text.is_empty() && plugin_available > 3 {
        truncate_width(&format!(" ◆ {plugin_text}"), plugin_available)
    } else {
        String::new()
    };

    let mut spans = Vec::new();
    let mut hits = Vec::new();
    let mut x = area.x;

    spans.push(Span::styled(main, style));
    x = x.saturating_add(main_width);
    if !plugin.is_empty() {
        let width = plugin.width() as u16;
        spans.push(Span::styled(plugin, Style::default().fg(Color::Magenta)));
        x = x.saturating_add(width);
    }

    let gap = hints_x.saturating_sub(x);
    if gap > 0 {
        spans.push(Span::raw(" ".repeat(gap as usize)));
        x = x.saturating_add(gap);
    }

    for (label, action) in hints {
        let width = label.width() as u16;
        if x.saturating_add(width) > area.x.saturating_add(area.width) {
            break;
        }
        hits.push((Rect::new(x, area.y, width, 1), *action));
        spans.push(Span::styled(*label, Style::default().fg(Color::DarkGray)));
        x = x.saturating_add(width);

        if x.saturating_add(2) <= area.x.saturating_add(area.width) {
            spans.push(Span::raw("  "));
            x = x.saturating_add(2);
        }
    }

    app.status_hits = hits;
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn truncate_width(text: &str, max: u16) -> String {
    let max = max as usize;
    if text.width() <= max {
        return text.to_string();
    }

    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let width = ch.width().unwrap_or(0);
        if used + width + 1 > max {
            break;
        }
        out.push(ch);
        used += width;
    }
    out.push('…');
    out
}

fn draw_help(frame: &mut Frame, area: Rect) {
    let popup = centered_rect(56, 70, area);
    frame.render_widget(Clear, popup);

    let lines = vec![
        Line::from(Span::styled(
            "QCL — keyboard",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("j / k / ↑ / ↓    move selection"),
        Line::from("g / G            first / last"),
        Line::from("/                filter versions (Esc clears)"),
        Line::from("Enter / l        launch selected version"),
        Line::from("i                install selected version"),
        Line::from("Tab / e          focus the command line"),
        Line::from("  editing        type to edit · Enter runs · Esc leaves"),
        Line::from("Ctrl+R           rebuild the command line"),
        Line::from("Ctrl+U           clear the command line"),
        Line::from("s                stop the running game"),
        Line::from("PgUp / PgDn      scroll logs (f = follow tail)"),
        Line::from("mouse            click select · double-click launch"),
        Line::from("                 wheel scrolls list/logs"),
        Line::from("Shift + drag     terminal text selection"),
        Line::from("r                reload version list"),
        Line::from("Esc              open the QCL menu"),
        Line::from("?                toggle this help"),
        Line::from("q / Ctrl+C       quit"),
        Line::from(""),
        Line::from(Span::styled(
            "offline profile: set with --username",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let paragraph = Paragraph::new(lines)
        .block(Block::bordered().title(" help "))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, popup);
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(vertical[1])[1]
}

fn level_color(level: LogLevel) -> Color {
    match level {
        LogLevel::Error => Color::LightRed,
        LogLevel::Warn => Color::Yellow,
        LogLevel::Info => Color::Gray,
        LogLevel::Debug => Color::DarkGray,
        LogLevel::Trace => Color::DarkGray,
    }
}

#[cfg(test)]
mod tests {
    use super::{is_too_small, truncate_width, wrap_cmdline, MIN_HEIGHT, MIN_WIDTH};
    use ratatui::layout::Rect;

    #[test]
    fn rejects_small_terminals() {
        assert!(is_too_small(Rect::new(0, 0, MIN_WIDTH - 1, MIN_HEIGHT)));
        assert!(is_too_small(Rect::new(0, 0, MIN_WIDTH, MIN_HEIGHT - 1)));
        assert!(!is_too_small(Rect::new(0, 0, MIN_WIDTH, MIN_HEIGHT)));
        assert!(!is_too_small(Rect::new(0, 0, 200, 60)));
    }

    #[test]
    fn truncates_to_width() {
        assert_eq!(truncate_width("abc", 5), "abc");
        assert_eq!(truncate_width("abcdef", 4), "abc…");
        assert_eq!(truncate_width("abc", 0), "…");
    }

    #[test]
    fn wraps_and_tracks_cursor() {
        let layout = wrap_cmdline("abcdef", 3, 4);
        assert_eq!(layout.rows[0], vec!['a', 'b', 'c', 'd']);
        assert_eq!(layout.rows[1], vec!['e', 'f']);
        assert_eq!((layout.cursor_row, layout.cursor_col), (0, 3));

        // Cursor at a wrap boundary moves to the next line.
        let layout = wrap_cmdline("abcdefgh", 4, 4);
        assert_eq!((layout.cursor_row, layout.cursor_col), (1, 0));

        // Cursor at the end of a full line gets its own line.
        let layout = wrap_cmdline("abcd", 4, 4);
        assert_eq!(layout.rows.len(), 2);
        assert_eq!((layout.cursor_row, layout.cursor_col), (1, 0));
    }
}
