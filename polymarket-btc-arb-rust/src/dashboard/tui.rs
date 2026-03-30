// src/dashboard/tui.rs — Real-time TUI dashboard (ratatui + crossterm)
//
// Layout (exactly as in the video):
// ┌──────────────────────────── Header Stats Bar ────────────────────────────────┐
// │  PnL (Daily)  |  PnL (Total)  |  Win Rate  |  Latency  |  Orders  |  BTC   │
// ├──────────── Left (60%) ────────────┬────────── Right (40%) ──────────────────┤
// │                                    │                                        │
// │        EQUITY CURVE CHART          │        EXECUTION LOG (scrolling)       │
// │        (Braille line graph)        │                                        │
// │                                    │                                        │
// ├────────────────────────────────────┴────────────────────────────────────────┤
// │  [q] Quit   [p] Pause/Resume   [r] Reset Daily   Mode: DRY_RUN            │
// └────────────────────────────────────────────────────────────────────────────┘

use crate::state::AppState;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{
        Axis, Block, Borders, Chart, Dataset, GraphType, List, ListItem, Paragraph,
    },
    Terminal,
};
use std::io;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

// ── Colour palette ───────────────────────────────────────────────────────────
const CYAN: Color   = Color::Rgb(0, 217, 255);
const GREEN: Color  = Color::Rgb(0, 255, 136);
const RED: Color    = Color::Rgb(255, 51, 102);
const AMBER: Color  = Color::Rgb(255, 184, 0);
const DIM: Color    = Color::Rgb(100, 100, 120);
const BG: Color     = Color::Rgb(10, 10, 10);

// ── Main TUI loop ────────────────────────────────────────────────────────────

pub fn run_tui(state: AppState, dry_run: bool) -> io::Result<()> {
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    crossterm::execute!(
        stdout,
        crossterm::terminal::EnterAlternateScreen,
        crossterm::cursor::Hide,
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let tick_rate = Duration::from_millis(33); // ~30 fps

    loop {
        let tick_start = Instant::now();

        if state.is_shutdown.load(Ordering::Relaxed) {
            break;
        }

        terminal.draw(|f| draw_frame(f, &state, dry_run))?;

        // Handle input (non-blocking)
        let timeout = tick_rate.saturating_sub(tick_start.elapsed());
        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        state.is_shutdown.store(true, Ordering::Relaxed);
                        break;
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        state.is_shutdown.store(true, Ordering::Relaxed);
                        break;
                    }
                    KeyCode::Char('p') => {
                        let paused = state.is_paused.load(Ordering::Relaxed);
                        state.is_paused.store(!paused, Ordering::Relaxed);
                    }
                    KeyCode::Char('r') => {
                        *state.daily_pnl.write() = 0.0;
                        state.daily_cap_hit.store(false, Ordering::Relaxed);
                        state.is_paused.store(false, Ordering::Relaxed);
                        state.push_log(crate::state::LogLevel::Info, "Daily counters reset by user");
                    }
                    _ => {}
                }
            }
        }
    }

    // Restore terminal
    crossterm::terminal::disable_raw_mode()?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::cursor::Show,
    )?;
    Ok(())
}

// ── Draw full frame ──────────────────────────────────────────────────────────

fn draw_frame(f: &mut ratatui::Frame, state: &AppState, dry_run: bool) {
    let size = f.area();

    // 3 rows: header(3), body(fill), footer(1)
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(1),
        ])
        .split(size);

    draw_header(f, main_chunks[0], state, dry_run);
    draw_body(f, main_chunks[1], state);
    draw_footer(f, main_chunks[2], state, dry_run);
}

// ── Header: stats bar ────────────────────────────────────────────────────────

fn draw_header(f: &mut ratatui::Frame, area: Rect, state: &AppState, dry_run: bool) {
    let daily = *state.daily_pnl.read();
    let total = *state.total_pnl.read();
    let win_rate = state.win_rate();
    let latency_us = state.last_latency_us.load(Ordering::Relaxed);
    let orders = state.orders.load(Ordering::Relaxed);
    let btc = state.best_btc_price();
    let balance = *state.balance.read();
    let trades = state.total_trades();

    let pnl_color = |v: f64| if v >= 0.0 { GREEN } else { RED };
    let lat_color = if latency_us < 50_000 { GREEN } else if latency_us < 100_000 { AMBER } else { RED };

    let mode_str = if dry_run { "DRY RUN" } else { "LIVE" };
    let mode_color = if dry_run { AMBER } else { GREEN };

    let paused = state.is_paused.load(Ordering::Relaxed);
    let pause_span = if paused {
        Span::styled(" ⏸ PAUSED ", Style::default().fg(RED).add_modifier(Modifier::BOLD))
    } else {
        Span::styled(" ● ACTIVE ", Style::default().fg(GREEN).add_modifier(Modifier::BOLD))
    };

    let header_line = Line::from(vec![
        pause_span,
        Span::styled(" │ ", Style::default().fg(DIM)),
        Span::styled("Balance: ", Style::default().fg(DIM)),
        Span::styled(format!("${:.2}", balance), Style::default().fg(CYAN).add_modifier(Modifier::BOLD)),
        Span::styled(" │ ", Style::default().fg(DIM)),
        Span::styled("Daily: ", Style::default().fg(DIM)),
        Span::styled(
            format!("{}{:.2}", if daily >= 0.0 { "+" } else { "" }, daily),
            Style::default().fg(pnl_color(daily)).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" │ ", Style::default().fg(DIM)),
        Span::styled("Total: ", Style::default().fg(DIM)),
        Span::styled(
            format!("{}{:.2}", if total >= 0.0 { "+" } else { "" }, total),
            Style::default().fg(pnl_color(total)).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" │ ", Style::default().fg(DIM)),
        Span::styled("Win: ", Style::default().fg(DIM)),
        Span::styled(
            format!("{:.1}% ({}/{})", win_rate, state.wins.load(Ordering::Relaxed), trades),
            Style::default().fg(if win_rate >= 60.0 { GREEN } else { AMBER }),
        ),
        Span::styled(" │ ", Style::default().fg(DIM)),
        Span::styled("Lat: ", Style::default().fg(DIM)),
        Span::styled(
            format!("{:.1}ms", latency_us as f64 / 1000.0),
            Style::default().fg(lat_color),
        ),
        Span::styled(" │ ", Style::default().fg(DIM)),
        Span::styled("Orders: ", Style::default().fg(DIM)),
        Span::styled(format!("{}", orders), Style::default().fg(CYAN)),
        Span::styled(" │ ", Style::default().fg(DIM)),
        Span::styled("BTC: ", Style::default().fg(DIM)),
        Span::styled(
            format!("${:.2}", btc),
            Style::default().fg(AMBER).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" │ ", Style::default().fg(DIM)),
        Span::styled(mode_str, Style::default().fg(mode_color).add_modifier(Modifier::BOLD)),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(CYAN))
        .title(Span::styled(
            " ⚡ POLYMARKET BTC ARB ",
            Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(BG));

    let paragraph = Paragraph::new(header_line).block(block);
    f.render_widget(paragraph, area);
}

// ── Body: equity chart (left) + execution log (right) ────────────────────────

fn draw_body(f: &mut ratatui::Frame, area: Rect, state: &AppState) {
    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(area);

    draw_equity_chart(f, body_chunks[0], state);
    draw_exec_log(f, body_chunks[1], state);
}

fn draw_equity_chart(f: &mut ratatui::Frame, area: Rect, state: &AppState) {
    let equity = state.equity.read();

    if equity.is_empty() {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(CYAN))
            .title(" Equity Curve ")
            .style(Style::default().bg(BG));
        let msg = Paragraph::new("Waiting for data...")
            .style(Style::default().fg(DIM))
            .block(block);
        f.render_widget(msg, area);
        return;
    }

    // Build data points: (x=seconds_from_start, y=equity)
    let first_ts = equity.front().unwrap().ts.timestamp() as f64;
    let data: Vec<(f64, f64)> = equity
        .iter()
        .map(|p| (p.ts.timestamp() as f64 - first_ts, p.equity))
        .collect();

    // Compute bounds
    let x_min = data.first().map(|d| d.0).unwrap_or(0.0);
    let x_max = data.last().map(|d| d.0).unwrap_or(1.0).max(x_min + 1.0);
    let y_min = data.iter().map(|d| d.1).fold(f64::MAX, f64::min);
    let y_max = data.iter().map(|d| d.1).fold(f64::MIN, f64::max);
    let y_pad = (y_max - y_min).max(1.0) * 0.05;

    let dataset = Dataset::default()
        .name("Equity")
        .marker(symbols::Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(CYAN))
        .data(&data);

    let chart = Chart::new(vec![dataset])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(CYAN))
                .title(Span::styled(
                    " 📈 Equity Curve ",
                    Style::default().fg(GREEN).add_modifier(Modifier::BOLD),
                ))
                .style(Style::default().bg(BG)),
        )
        .x_axis(
            Axis::default()
                .title("Time")
                .style(Style::default().fg(DIM))
                .bounds([x_min, x_max])
                .labels(vec![
                    Span::raw("start"),
                    Span::raw("now"),
                ]),
        )
        .y_axis(
            Axis::default()
                .title("$")
                .style(Style::default().fg(DIM))
                .bounds([y_min - y_pad, y_max + y_pad])
                .labels(vec![
                    Span::raw(format!("{:.0}", y_min)),
                    Span::raw(format!("{:.0}", (y_min + y_max) / 2.0)),
                    Span::raw(format!("{:.0}", y_max)),
                ]),
        );

    f.render_widget(chart, area);
}

fn draw_exec_log(f: &mut ratatui::Frame, area: Rect, state: &AppState) {
    let log = state.log.read();

    // Show the most recent entries that fit in the area
    let max_visible = (area.height as usize).saturating_sub(2); // borders
    let start = log.len().saturating_sub(max_visible);

    let items: Vec<ListItem> = log
        .iter()
        .skip(start)
        .map(|entry| {
            let time_str = entry.ts.format("%H:%M:%S%.3f").to_string();
            let line = Line::from(vec![
                Span::styled(
                    format!("[{}]", time_str),
                    Style::default().fg(DIM),
                ),
                Span::raw(" "),
                Span::styled(
                    format!("[{}]", entry.level.label()),
                    Style::default().fg(entry.level.color()).add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(
                    entry.message.clone(),
                    Style::default().fg(Color::White),
                ),
            ]);
            ListItem::new(line)
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(CYAN))
        .title(Span::styled(
            " 📋 Execution Log ",
            Style::default().fg(AMBER).add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(BG));

    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

// ── Footer ───────────────────────────────────────────────────────────────────

fn draw_footer(f: &mut ratatui::Frame, area: Rect, state: &AppState, dry_run: bool) {
    let markets_count = state.markets.len();
    let footer = Line::from(vec![
        Span::styled(" [q]", Style::default().fg(CYAN).add_modifier(Modifier::BOLD)),
        Span::styled(" Quit  ", Style::default().fg(DIM)),
        Span::styled("[p]", Style::default().fg(CYAN).add_modifier(Modifier::BOLD)),
        Span::styled(" Pause  ", Style::default().fg(DIM)),
        Span::styled("[r]", Style::default().fg(CYAN).add_modifier(Modifier::BOLD)),
        Span::styled(" Reset Daily  ", Style::default().fg(DIM)),
        Span::styled("│ ", Style::default().fg(DIM)),
        Span::styled(format!("Markets: {} ", markets_count), Style::default().fg(CYAN)),
        Span::styled("│ ", Style::default().fg(DIM)),
        Span::styled(
            if dry_run { "🔒 DRY RUN" } else { "🔴 LIVE TRADING" },
            Style::default().fg(if dry_run { AMBER } else { RED }).add_modifier(Modifier::BOLD),
        ),
    ]);

    let paragraph = Paragraph::new(footer).style(Style::default().bg(BG));
    f.render_widget(paragraph, area);
}
