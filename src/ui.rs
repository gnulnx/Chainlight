use crate::{
    app::App,
    metrics::GraphKind,
    theme::Palette,
    types::Snapshot,
    utils::{duration, human_bytes, number, safe},
};
use chrono::{DateTime, Utc};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph, Row, Table, Tabs, Wrap},
};

pub fn draw(frame: &mut Frame, app: &App, palette: &Palette) {
    let area = frame.area();
    let base = Style::default()
        .fg(palette.foreground)
        .bg(palette.background);
    frame.render_widget(Block::default().style(base), area);
    if area.width < 30 || area.height < 10 {
        frame.render_widget(
            Paragraph::new("CHAINLIGHT\nEnlarge terminal to 40 × 14\nq quit").style(base),
            area,
        );
        return;
    }
    let padding = if area.width >= 70 { 2 } else { 1 };
    let inner = Rect::new(
        area.x + padding,
        area.y,
        area.width.saturating_sub(padding * 2),
        area.height,
    );
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(if area.width >= 100 && area.height >= 32 {
                8
            } else {
                3
            }),
            Constraint::Length(2),
            Constraint::Min(1),
            Constraint::Length(if area.height >= 36 { 4 } else { 0 }),
            Constraint::Length(2),
        ])
        .split(inner);
    let status = status(app);
    let status_color = if app.error.is_some() {
        palette.red
    } else if app
        .current
        .as_ref()
        .is_some_and(|s| !s.chain.initialblockdownload && s.network.networkactive)
    {
        palette.green
    } else {
        palette.yellow
    };
    if area.width >= 100 && area.height >= 32 {
        pixel_header(frame, chunks[0], app, palette, status_color);
    } else {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(vec![
                    Span::styled(
                        "₿  CHAINLIGHT",
                        Style::default()
                            .fg(palette.accent)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        if area.width > 65 {
                            "   Your chain. Your verification."
                        } else {
                            "   BITCOIN NODE"
                        },
                        Style::default().fg(palette.muted),
                    ),
                ]),
                Line::from(vec![
                    Span::styled(format!("● {status}"), Style::default().fg(status_color)),
                    Span::styled(
                        app.current
                            .as_ref()
                            .map(|s| format!("   {}", safe(&s.chain.chain)))
                            .unwrap_or_default(),
                        Style::default().fg(palette.muted),
                    ),
                ]),
            ])
            .style(base),
            chunks[0],
        );
    }
    frame.render_widget(
        Tabs::new(["1 Overview", "2 Peers", "3 Storage"])
            .select(app.tab.min(2))
            .padding("", "  ")
            .divider(" ")
            .style(Style::default().fg(palette.muted))
            .highlight_style(
                Style::default()
                    .fg(palette.accent)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            ),
        chunks[1],
    );
    let mut body = chunks[2];
    if let Some(error) = &app.error {
        let pieces = Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).split(body);
        frame.render_widget(
            Paragraph::new(safe(error))
                .style(Style::default().fg(palette.yellow))
                .wrap(Wrap { trim: true }),
            pieces[0],
        );
        body = pieces[1];
    } else if let Some(s) = &app.current {
        let mut warnings = s.notes.iter().map(|n| safe(n)).collect::<Vec<_>>();
        match &s.chain.warnings {
            serde_json::Value::String(w) if !w.is_empty() => {
                warnings.push(format!("Core: {}", safe(w)))
            }
            serde_json::Value::Array(values) => warnings.extend(
                values
                    .iter()
                    .filter_map(|v| v.as_str())
                    .filter(|v| !v.is_empty())
                    .map(|v| format!("Core: {}", safe(v))),
            ),
            _ => {}
        }
        if !warnings.is_empty() {
            let pieces = Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).split(body);
            frame.render_widget(
                Paragraph::new(warnings.join(" · "))
                    .style(Style::default().fg(palette.yellow))
                    .wrap(Wrap { trim: true }),
                pieces[0],
            );
            body = pieces[1];
        }
    }
    if app.help {
        help(frame, body, palette)
    } else if let Some(snapshot) = &app.current {
        if app.tab == 1 {
            peers(frame, body, snapshot, palette)
        } else if app.tab == 2 {
            storage(frame, body, snapshot, palette)
        } else if body.height < 14 || body.width < 60 {
            compact(frame, body, app, snapshot, palette)
        } else {
            overview(frame, body, app, snapshot, palette)
        }
    } else {
        startup(frame, body, palette)
    }
    activity(frame, chunks[3], app, palette);
    let observed = app
        .current
        .as_ref()
        .map(|s| {
            let stale = if app.error.is_some() {
                "LAST SUCCESS"
            } else {
                "Observed"
            };
            format!(
                "{stale} {} UTC · 2s refresh",
                s.observed_at.format("%H:%M:%S")
            )
        })
        .unwrap_or_else(|| "Connecting to local Bitcoin Core".to_string());
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(if area.width > 65 {
                "← → / Tab pages   g graph   u upload   r refresh   ? help   q quit"
            } else {
                "1 2 3 pages · g graph · ? · q quit"
            }),
            Line::from(observed),
        ])
        .style(Style::default().fg(palette.muted)),
        chunks[4],
    );
}
// Five-row bitmap letters use ordinary terminal cells, including over SSH.
fn pixel_header(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    p: &Palette,
    status_color: ratatui::style::Color,
) {
    let glyphs = [
        ["01110", "10000", "10000", "10000", "01110"], // C
        ["10001", "10001", "11111", "10001", "10001"], // H
        ["01110", "10001", "11111", "10001", "10001"], // A
        ["111", "010", "010", "010", "111"],           // I
        ["10001", "11001", "10101", "10011", "10001"], // N
        ["10000", "10000", "10000", "10000", "11111"], // L
        ["111", "010", "010", "010", "111"],           // I
        ["01111", "10000", "10111", "10001", "01110"], // G
        ["10001", "10001", "11111", "10001", "10001"], // H
        ["11111", "00100", "00100", "00100", "00100"], // T
    ];
    let scale = if area.width >= 120 { 2 } else { 1 };
    let mut lines = Vec::new();
    for row in 0..5 {
        let mut spans = Vec::new();
        for (i, glyph) in glyphs.iter().enumerate() {
            let pixels = glyph[row]
                .chars()
                .map(|bit| {
                    if bit == '1' {
                        "█".repeat(scale)
                    } else {
                        " ".repeat(scale)
                    }
                })
                .collect::<String>();
            let color = if i < 5 {
                p.accent
            } else if i < 8 {
                p.green
            } else {
                p.yellow
            };
            spans.push(Span::styled(pixels, Style::default().fg(color)));
            spans.push(Span::raw(" "));
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::from(Span::styled(
        "CHAINLIGHT  /  YOUR CHAIN. YOUR VERIFICATION.",
        Style::default().fg(p.muted),
    )));
    lines.push(Line::from(vec![
        Span::styled(
            format!("◆ {}", status(app)),
            Style::default().fg(status_color),
        ),
        Span::styled(
            app.current
                .as_ref()
                .map(|s| format!("   / {}", safe(&s.chain.chain)))
                .unwrap_or_default(),
            Style::default().fg(p.muted),
        ),
    ]));
    frame.render_widget(Paragraph::new(lines), area);
}

fn voxel_work(frame: &mut Frame, area: Rect, s: &Snapshot, p: &Palette) {
    let progress = s.chain.verificationprogress.clamp(0.0, 1.0);
    let filled = (progress * 100.0).floor() as usize;
    let partial = progress * 100.0 > filled as f64;
    let rows = if area.width >= 40 && area.height < 13 {
        5
    } else {
        10
    };
    let columns = 100 / rows;
    let mut lines = vec![
        Line::from(Span::styled(
            "THE VERIFICATION JOURNEY",
            Style::default().fg(p.accent).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "1 cell = 1% estimated work",
            Style::default().fg(p.muted),
        )),
    ];
    for row in 0..rows {
        lines.push(Line::from(
            (0..columns)
                .map(|col| {
                    let cell = row * columns + col;
                    let (glyph, color) = if cell < filled {
                        ("■ ", p.green)
                    } else if cell == filled && partial {
                        ("◧ ", p.yellow)
                    } else {
                        ("□ ", p.muted)
                    };
                    Span::styled(glyph, Style::default().fg(color))
                })
                .collect::<Vec<_>>(),
        ));
    }
    lines.push(Line::from(Span::styled(
        "Work, not equal block counts.",
        Style::default().fg(p.muted),
    )));
    frame.render_widget(Paragraph::new(lines), area);
}

fn activity(frame: &mut Frame, area: Rect, app: &App, p: &Palette) {
    if area.height == 0 {
        return;
    }
    let mut lines = vec![Line::from(Span::styled(
        "NODE ACTIVITY / OBSERVED CHANGES · UTC",
        Style::default().fg(p.accent).add_modifier(Modifier::BOLD),
    ))];
    if app.events.is_empty() {
        lines.push(Line::from(Span::styled(
            "Waiting for a node observation…",
            Style::default().fg(p.muted),
        )));
    }
    for event in app
        .events
        .iter()
        .rev()
        .take(usize::from(area.height.saturating_sub(1)))
    {
        lines.push(Line::from(vec![
            Span::styled(
                format!("{}  ", event.at.format("%H:%M:%S")),
                Style::default().fg(p.muted),
            ),
            Span::styled(
                format!("{:<19} ", event.kind),
                Style::default().fg(if event.kind == "CONNECTION LOST" {
                    p.yellow
                } else {
                    p.green
                }),
            ),
            Span::raw(safe(&event.detail)),
        ]));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn status(app: &App) -> &'static str {
    if app.error.is_some() {
        return if app.current.is_some() {
            "CONNECTION LOST · STALE DATA"
        } else {
            "WAITING FOR CORE"
        };
    }
    let Some(s) = &app.current else {
        return "CONNECTING";
    };
    if !s.network.networkactive {
        "NETWORK DISABLED"
    } else if !s.chain.initialblockdownload {
        "LIVE NODE"
    } else if s.chain.headers == 0 && presync(s) > 0 {
        "HEADER PRE-SYNC"
    } else if s.chain.blocks == 0 {
        "DISCOVERING HEADERS"
    } else {
        "VERIFYING BLOCKS"
    }
}
fn presync(s: &Snapshot) -> u64 {
    s.peers
        .as_ref()
        .map(|v| {
            v.iter()
                .map(|p| p.presynced_headers.max(0) as u64)
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0)
}
fn date(timestamp: i64) -> String {
    if timestamp <= 0 {
        "Waiting for blocks".into()
    } else {
        DateTime::<Utc>::from_timestamp(timestamp, 0)
            .map(|d| d.format("%d %b %Y · %H:%M UTC").to_string())
            .unwrap_or_else(|| "Unavailable".into())
    }
}
fn optional(n: Option<u64>) -> String {
    n.map(human_bytes).unwrap_or_else(|| "Unavailable".into())
}
fn rate(app: &App) -> String {
    if app.error.is_some() {
        return "Unavailable · connection lost".into();
    }
    if app.current.as_ref().is_some_and(|s| s.traffic.is_none()) {
        return "Unavailable".into();
    }
    match (app.metrics.down, app.metrics.up) {
        (Some(d), Some(u)) => format!(
            "{} /s ↓   {} /s ↑",
            human_bytes(d.max(0.0) as u64),
            human_bytes(u.max(0.0) as u64)
        ),
        _ => "Measuring…".into(),
    }
}
fn block<'a>(title: &'a str, p: &Palette) -> Block<'a> {
    Block::default()
        .title(Line::from(Span::styled(
            format!(" {title} "),
            Style::default().fg(p.accent).add_modifier(Modifier::BOLD),
        )))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(p.muted))
        .style(Style::default().fg(p.foreground))
        .border_type(ratatui::widgets::BorderType::Rounded)
}
fn kv(label: &str, value: String, p: &Palette) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<20}"), Style::default().fg(p.muted)),
        Span::styled(value, Style::default().fg(p.foreground)),
    ])
}
fn overview(frame: &mut Frame, area: Rect, app: &App, s: &Snapshot, p: &Palette) {
    if area.width < 95 || area.height < 22 {
        let rows = Layout::vertical([Constraint::Length(9), Constraint::Min(5)]).split(area);
        chain(frame, rows[0], app, s, p);
        network(frame, rows[1], app, s, p);
        return;
    }
    let hero_height = if s.chain.initialblockdownload {
        ((area.height * 55) / 100).max(15).min(area.height - 9)
    } else {
        15.min(area.height - 9)
    };
    let rows = Layout::vertical([Constraint::Length(hero_height), Constraint::Min(8)])
        .spacing(1)
        .split(area);
    hero(frame, rows[0], app, s, p);
    let cards = Layout::horizontal([Constraint::Percentage(72), Constraint::Percentage(28)])
        .spacing(1)
        .split(rows[1]);
    network(frame, cards[0], app, s, p);
    let card = block("NODE HEALTH", p);
    let inner = card.inner(cards[1]);
    frame.render_widget(card, cards[1]);
    let mut lines = vec![
        kv(
            "Peers",
            format!(
                "{} · {} out / {} in",
                s.network.connections, s.network.connections_out, s.network.connections_in
            ),
            p,
        ),
        kv(
            "Network",
            if s.network.networkactive {
                "Active"
            } else {
                "Disabled"
            }
            .into(),
            p,
        ),
        kv(
            "Mode",
            if s.chain.pruned { "Pruned" } else { "Archival" }.into(),
            p,
        ),
        kv("Databases", optional(s.datadir_bytes), p),
        kv("Disk available", optional(s.disk_available_bytes), p),
        kv("Available RAM", optional(s.memory_available_bytes), p),
        kv(
            "Core uptime",
            s.uptime_seconds
                .map(duration)
                .unwrap_or_else(|| "Unavailable".into()),
            p,
        ),
    ];
    if inner.height > 10 {
        lines.extend([
            Line::from(""),
            Line::from(Span::styled(
                "YOUR NODE / YOUR RULES",
                Style::default().fg(p.accent),
            )),
            Line::from(Span::styled(
                "Core keeps running when you quit.",
                Style::default().fg(p.muted),
            )),
        ]);
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

fn pixel_number(frame: &mut Frame, area: Rect, text: &str, color: ratatui::style::Color) {
    let scale = if area.width >= text.len() as u16 * 8 {
        2
    } else {
        1
    };
    let mut lines = Vec::new();
    for row in 0..5 {
        let mut line = String::new();
        for c in text.chars() {
            let glyph = match c {
                '0' => ["111", "101", "101", "101", "111"],
                '1' => ["010", "110", "010", "010", "111"],
                '2' => ["111", "001", "111", "100", "111"],
                '3' => ["111", "001", "111", "001", "111"],
                '4' => ["101", "101", "111", "001", "001"],
                '5' => ["111", "100", "111", "001", "111"],
                '6' => ["111", "100", "111", "101", "111"],
                '7' => ["111", "001", "010", "010", "010"],
                '8' => ["111", "101", "111", "101", "111"],
                '9' => ["111", "101", "111", "001", "111"],
                '.' => ["000", "000", "000", "000", "010"],
                '-' => ["000", "000", "111", "000", "000"],
                '%' => ["101", "001", "010", "100", "101"],
                _ => ["000", "000", "000", "000", "000"],
            };
            for bit in glyph[row].chars() {
                line.push_str(&if bit == '1' {
                    "█".repeat(scale)
                } else {
                    " ".repeat(scale)
                });
            }
            line.push(' ');
        }
        lines.push(Line::from(line));
    }
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().fg(color)),
        area,
    );
}

fn hero(frame: &mut Frame, area: Rect, app: &App, s: &Snapshot, p: &Palette) {
    let ibd = s.chain.initialblockdownload;
    let card = block(
        if ibd {
            "CHAIN VERIFICATION / INITIAL BLOCK DOWNLOAD"
        } else {
            "LIVE NODE / INDEPENDENTLY VERIFIED"
        },
        p,
    );
    let inner = card.inner(area);
    frame.render_widget(card, area);
    let columns = Layout::horizontal([Constraint::Percentage(65), Constraint::Percentage(35)])
        .spacing(3)
        .split(inner);
    let left = columns[0];
    if ibd && s.chain.headers == 0 && presync(s) > 0 {
        chain(frame, left, app, s, p);
    } else if ibd {
        let progress = s.chain.verificationprogress.clamp(0.0, 1.0);
        pixel_number(
            frame,
            Rect::new(left.x, left.y, left.width, 5),
            &format!("{:.2}%", progress * 100.0),
            p.accent,
        );
        frame.render_widget(
            Paragraph::new(format!(
                "{:.2}% ESTIMATED VERIFICATION WORK",
                progress * 100.0
            ))
            .style(Style::default().fg(p.muted)),
            Rect::new(left.x, left.y + 5, left.width, 1),
        );
        if left.height >= 13 {
            let meter = Rect::new(left.x, left.y + 7, left.width, 2);
            pixel_meter(frame, meter, progress, p);
            let eta = if app.error.is_some() {
                "Estimate unavailable · stale data".into()
            } else {
                app.metrics
                    .eta()
                    .unwrap_or_else(|| "Measuring · needs 2 minutes of continuous progress".into())
            };
            if left.height >= 22 {
                let remaining = app.metrics.eta_seconds().filter(|_| app.error.is_none());
                let minutes = remaining.is_some_and(|n| n < 3600);
                frame.render_widget(
                    Paragraph::new(if minutes {
                        "ESTIMATED MINUTES REMAINING"
                    } else {
                        "ESTIMATED HOURS REMAINING"
                    })
                    .style(Style::default().fg(p.muted)),
                    Rect::new(left.x, left.y + 10, left.width, 1),
                );
                let amount = remaining
                    .map(|n| n.div_ceil(if minutes { 60 } else { 3600 }).to_string())
                    .unwrap_or_else(|| "--".into());
                pixel_number(
                    frame,
                    Rect::new(left.x, left.y + 11, left.width, 5),
                    &amount,
                    p.yellow,
                );
                frame.render_widget(
                    Paragraph::new(eta)
                        .style(Style::default().fg(p.yellow).add_modifier(Modifier::BOLD)),
                    Rect::new(left.x, left.y + 16, left.width, 1),
                );
            } else {
                frame.render_widget(
                    Paragraph::new(vec![
                        Line::from(Span::styled(
                            "ESTIMATED TIME REMAINING",
                            Style::default().fg(p.muted),
                        )),
                        Line::from(Span::styled(
                            eta,
                            Style::default().fg(p.yellow).add_modifier(Modifier::BOLD),
                        )),
                    ]),
                    Rect::new(left.x, left.y + 10, left.width, 2),
                );
            }
        }
        if left.height >= 16 {
            frame.render_widget(
                Paragraph::new(vec![
                    kv("Verified blocks", number(s.chain.blocks), p),
                    kv("Validated headers", number(s.chain.headers), p),
                    kv("Chain history", date(s.chain.time), p),
                    kv(
                        "Validation rate",
                        app.metrics
                            .blocks_per_second
                            .filter(|_| app.error.is_none())
                            .map(|n| format!("{n:.1} blocks/s"))
                            .unwrap_or_else(|| "Measuring…".into()),
                        p,
                    ),
                ]),
                Rect::new(
                    left.x,
                    left.y + if left.height >= 22 { 18 } else { 13 },
                    left.width,
                    if left.height >= 22 {
                        left.height - 18
                    } else {
                        left.height - 13
                    },
                ),
            );
        } else {
            frame.render_widget(
                Paragraph::new(format!(
                    "{} blocks · {} headers · {}",
                    number(s.chain.blocks),
                    number(s.chain.headers),
                    date(s.chain.time)
                ))
                .style(Style::default().fg(p.muted)),
                Rect::new(left.x, left.y + left.height - 1, left.width, 1),
            );
        }
        voxel_work(frame, columns[1], s, p);
    } else {
        pixel_number(
            frame,
            Rect::new(left.x, left.y, left.width, 5),
            &s.chain.blocks.to_string(),
            p.green,
        );
        let age = tip_age(s, app.error.is_some());
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    format!("VERIFIED TIP #{}", number(s.chain.blocks)),
                    Style::default().fg(p.green).add_modifier(Modifier::BOLD),
                )),
                kv("Time since block", age, p),
                kv("Block timestamp", date(s.chain.time), p),
                kv(
                    "Mempool",
                    s.mempool
                        .as_ref()
                        .map(|m| {
                            format!("{} transactions · {}", number(m.size), human_bytes(m.bytes))
                        })
                        .unwrap_or_else(|| "Unavailable".into()),
                    p,
                ),
                kv(
                    "Minimum fee",
                    s.mempool
                        .as_ref()
                        .map(|m| format!("{:.2} sat/vB", m.mempoolminfee * 100_000.0))
                        .unwrap_or_else(|| "Unavailable".into()),
                    p,
                ),
                Line::from(Span::styled(
                    "Block timestamps are miner-reported; age is approximate.",
                    Style::default().fg(p.muted),
                )),
            ]),
            Rect::new(
                left.x,
                left.y + 6,
                left.width,
                left.height.saturating_sub(6),
            ),
        );
        fee_view(frame, columns[1], s, p);
    }
}
fn tip_age(s: &Snapshot, stale: bool) -> String {
    let now = if stale {
        s.observed_at.timestamp()
    } else {
        Utc::now().timestamp()
    };
    if s.chain.time <= 0 {
        "Unavailable".into()
    } else if s.chain.time > now {
        "Future block timestamp".into()
    } else {
        format!(
            "{}{}",
            duration((now - s.chain.time) as u64),
            if stale { " at last observation" } else { "" }
        )
    }
}
fn pixel_meter(frame: &mut Frame, area: Rect, progress: f64, p: &Palette) {
    let cells = area.width / 3;
    let done = (progress * f64::from(cells)).floor() as u16;
    for i in 0..cells {
        let color = if i < done {
            p.green
        } else if progress > 0.0 && i == done {
            p.yellow
        } else {
            p.muted
        };
        frame.render_widget(
            Paragraph::new(if i <= done && progress > 0.0 {
                "██\n██"
            } else {
                "░░\n░░"
            })
            .style(Style::default().fg(color)),
            Rect::new(area.x + i * 3, area.y, 2, area.height),
        );
    }
}
fn fee_view(frame: &mut Frame, area: Rect, s: &Snapshot, p: &Palette) {
    let mut lines = vec![
        Line::from(Span::styled(
            "FEE OUTLOOK / sat/vB",
            Style::default().fg(p.accent).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];
    let peak = s
        .fee_estimates
        .as_ref()
        .into_iter()
        .flatten()
        .filter_map(|f| f.sat_per_vbyte)
        .fold(0.0_f64, f64::max)
        .max(1.0);
    for (index, target) in [2, 6, 12].iter().enumerate() {
        let estimate = s
            .fee_estimates
            .as_ref()
            .and_then(|fees| fees.iter().find(|f| f.target == *target));
        let color = [p.yellow, p.green, p.accent][index];
        match estimate.and_then(|f| f.sat_per_vbyte) {
            Some(fee) => {
                lines.push(Line::from(Span::styled(
                    format!("{target:>2} block target   {fee:>7.2}"),
                    Style::default().fg(color),
                )));
                let width = (f64::from(area.width.saturating_sub(2)) * fee / peak).round() as usize;
                lines.push(Line::from(Span::styled(
                    "▀".repeat(width),
                    Style::default().fg(color),
                )));
                if let Some(actual) = estimate.and_then(|f| f.blocks).filter(|n| n != target) {
                    lines.push(Line::from(format!("Core used {actual} blocks")));
                }
            }
            None => {
                lines.push(Line::from(format!(
                    "{target:>2} block target   Unavailable"
                )));
                lines.push(Line::from(""));
            }
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Core estimates · refreshed every 60s",
        Style::default().fg(p.muted),
    )));
    if let Some(at) = s.fees_observed_at {
        lines.push(Line::from(format!(
            "Observed {} UTC",
            at.format("%H:%M:%S")
        )));
    }
    frame.render_widget(Paragraph::new(lines), area);
}
fn chain(frame: &mut Frame, area: Rect, app: &App, s: &Snapshot, p: &Palette) {
    let title = if s.chain.initialblockdownload {
        "INDEPENDENT VERIFICATION"
    } else {
        "UP TO DATE"
    };
    let card = block(title, p);
    let inner = card.inner(area);
    frame.render_widget(card, area);
    let progress = if s.chain.verificationprogress.is_finite() {
        s.chain.verificationprogress.clamp(0.0, 1.0)
    } else {
        0.0
    };
    if s.chain.initialblockdownload && s.chain.headers == 0 && presync(s) > 0 {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    format!("{} headers examined", number(presync(s))),
                    Style::default().fg(p.accent).add_modifier(Modifier::BOLD),
                )),
                Line::from("Discovering chain history before accepting headers."),
                Line::from("Block verification has not started yet."),
                kv("Verified blocks", number(s.chain.blocks), p),
                kv("Validated headers", number(s.chain.headers), p),
                kv("Chain history", date(s.chain.time), p),
            ]),
            inner,
        );
    } else if s.chain.initialblockdownload {
        let rows = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("Verification  ", Style::default().fg(p.muted)),
                Span::styled(
                    format!("{:.2}%", progress * 100.0),
                    Style::default().fg(p.accent).add_modifier(Modifier::BOLD),
                ),
                Span::styled("   estimated work completed", Style::default().fg(p.muted)),
            ])),
            rows[0],
        );
        frame.render_widget(
            Gauge::default()
                .ratio(progress)
                .label("")
                .gauge_style(Style::default().fg(p.accent)),
            rows[1],
        );
        let eta = if app.error.is_some() {
            "Estimate unavailable · stale data".into()
        } else {
            app.metrics
                .eta()
                .unwrap_or_else(|| "Measuring verification rate…".into())
        };
        frame.render_widget(
            Paragraph::new(eta).style(Style::default().fg(p.muted)),
            rows[2],
        );
        frame.render_widget(
            Paragraph::new(vec![
                kv("Verified blocks", number(s.chain.blocks), p),
                kv("Validated headers", number(s.chain.headers), p),
                kv("Chain history", date(s.chain.time), p),
            ]),
            rows[3],
        );
    } else {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "Your node independently verifies Bitcoin.",
                    Style::default().fg(p.green),
                )),
                kv("Block height", number(s.chain.blocks), p),
                kv("Latest block", date(s.chain.time), p),
                kv(
                    "Mempool",
                    s.mempool
                        .as_ref()
                        .map(|m| {
                            format!("{} transactions · {}", number(m.size), human_bytes(m.bytes))
                        })
                        .unwrap_or_else(|| "Unavailable".into()),
                    p,
                ),
                kv(
                    "Minimum fee",
                    s.mempool
                        .as_ref()
                        .map(|m| format!("{:.1} sat/vB", m.mempoolminfee * 100_000.0))
                        .unwrap_or_else(|| "Unavailable".into()),
                    p,
                ),
            ]),
            inner,
        );
    }
}
fn network(frame: &mut Frame, area: Rect, app: &App, s: &Snapshot, p: &Palette) {
    let title = format!("{} / 120s WINDOW · g CYCLE", app.graph.title());
    let card = block(&title, p);
    let inner = card.inner(area);
    frame.render_widget(card, area);
    let current = if app.error.is_some() {
        None
    } else {
        match app.graph {
            GraphKind::Bandwidth => app.metrics.down,
            GraphKind::Blocks => app.metrics.blocks_per_second,
            GraphKind::Transactions => app.metrics.transactions_per_second,
        }
    };
    let mut text = vec![kv(
        "Current",
        current
            .map(|v| graph_rate(app.graph, v))
            .unwrap_or_else(|| {
                if app.error.is_some() {
                    "Unavailable · connection lost".into()
                } else {
                    "Unavailable · waiting for fresh samples".into()
                }
            }),
        p,
    )];
    if app.graph == GraphKind::Bandwidth {
        text.push(kv(
            "Upload",
            app.metrics
                .up
                .filter(|_| app.error.is_none())
                .map(|v| {
                    format!(
                        "{} · u {} overlay",
                        graph_rate(GraphKind::Bandwidth, v),
                        if app.show_upload { "hide" } else { "show" }
                    )
                })
                .unwrap_or_else(|| "Unavailable".into()),
            p,
        ));
        text.push(kv(
            "Received by Core",
            s.traffic
                .as_ref()
                .map(|t| format!("{} since node start", human_bytes(t.received)))
                .unwrap_or_else(|| "Unavailable".into()),
            p,
        ));
    } else {
        text.push(kv(
            "Verified tip",
            format!(
                "#{} · {} peers",
                number(s.chain.blocks),
                s.network.connections
            ),
            p,
        ));
        text.push(Line::from(Span::styled(
            if app.graph == GraphKind::Blocks {
                "Accepted height change / elapsed observation time"
            } else {
                "Verified chain transaction count change / elapsed time"
            },
            Style::default().fg(p.muted),
        )));
    }
    frame.render_widget(Paragraph::new(text), inner);
    if inner.height >= 6 {
        pixel_histogram(
            frame,
            Rect::new(inner.x, inner.y + 3, inner.width, inner.height - 3),
            app,
            p,
        );
    }
}
fn graph_rate(kind: GraphKind, value: f64) -> String {
    match kind {
        GraphKind::Bandwidth => format!("{}/s", human_bytes(value.max(0.0) as u64)),
        GraphKind::Blocks => format!("{value:.2} blocks/s"),
        GraphKind::Transactions => format!("{value:.1} tx/s"),
    }
}
struct TimePlot {
    end: DateTime<Utc>,
    bins: Vec<Option<(f64, Option<f64>)>>,
}
fn time_plot(app: &App, columns: u16) -> Option<TimePlot> {
    let samples = &app.metrics.samples[app.graph as usize];
    let end = samples
        .iter()
        .rev()
        .find(|s| s.value(app.graph).is_some())?
        .end;
    let start = end - chrono::Duration::seconds(120);
    let step = 120_000.0 / f64::from(columns);
    let mut bins = Vec::new();
    for column in 0..columns {
        let left = f64::from(column) * step;
        let right = left + step;
        let (mut total, mut seconds, mut up_total, mut up_seconds) = (0.0, 0.0, 0.0, 0.0);
        for sample in samples {
            let a = (sample.start - start).num_milliseconds() as f64;
            let b = (sample.end - start).num_milliseconds() as f64;
            let overlap = b.min(right) - a.max(left);
            if overlap > 0.0
                && let Some(value) = sample.value(app.graph)
            {
                total += value * overlap;
                seconds += overlap;
                if let Some(up) = sample.upload {
                    up_total += up * overlap;
                    up_seconds += overlap;
                }
            }
        }
        bins.push(if seconds > 0.0 {
            Some((
                total / seconds,
                if up_seconds > 0.0 {
                    Some(up_total / up_seconds)
                } else {
                    None
                },
            ))
        } else {
            None
        });
    }
    Some(TimePlot { end, bins })
}
fn pixel_histogram(frame: &mut Frame, area: Rect, app: &App, p: &Palette) {
    let spacious = area.width >= 70 && area.height >= 12;
    let pixel_width = if spacious { 2 } else { 1 };
    let column_step = if spacious { 3 } else { 1 };
    let row_step = if spacious { 2 } else { 1 };
    let chart_height = area.height.saturating_sub(3);
    let rows = chart_height.div_ceil(row_step);
    let columns = area.width / column_step;
    if rows == 0 || columns == 0 {
        return;
    }
    let Some(plot) = time_plot(app, columns) else {
        frame.render_widget(
            Paragraph::new(if app.error.is_some() {
                "Network activity unavailable · reconnecting"
            } else {
                "Collecting activity · two fresh observations needed"
            })
            .style(Style::default().fg(p.muted)),
            area,
        );
        return;
    };
    let peak = plot
        .bins
        .iter()
        .flatten()
        .map(|v| v.0)
        .fold(0.0_f64, f64::max);
    let upload = app.show_upload && app.graph == GraphKind::Bandwidth;
    let scale = if upload {
        plot.bins
            .iter()
            .flatten()
            .filter_map(|v| v.1)
            .fold(peak, f64::max)
    } else {
        peak
    };
    for (column, bin) in plot.bins.iter().enumerate() {
        let Some((value, up)) = bin else {
            continue;
        };
        let ratio = if scale == 0.0 { 0.0 } else { value / scale };
        let filled = (ratio * f64::from(rows)).ceil() as u16;
        let color = if ratio <= 0.33 {
            p.accent
        } else if ratio <= 0.66 {
            p.green
        } else {
            p.yellow
        };
        let x = area.x + column as u16 * column_step;
        for row in 0..filled.min(rows) {
            let y = area.y + chart_height - 1 - row * row_step;
            frame.render_widget(
                Paragraph::new("█".repeat(usize::from(pixel_width)))
                    .style(Style::default().fg(color)),
                Rect::new(x, y, pixel_width, 1),
            );
        }
        if *value == 0.0 {
            frame.render_widget(
                Paragraph::new("·").style(Style::default().fg(p.muted)),
                Rect::new(x, area.y + chart_height - 1, 1, 1),
            );
        }
        if upload
            && let Some(up) = up
            && *up > 0.0
            && scale > 0.0
        {
            let level = (up / scale * f64::from(rows)).ceil() as u16;
            let y = area.y + chart_height - 1 - level.saturating_sub(1).min(rows - 1) * row_step;
            frame.render_widget(
                Paragraph::new("◆").style(Style::default().fg(p.foreground)),
                Rect::new(x, y, 1, 1),
            );
        }
    }
    let current = match app.graph {
        GraphKind::Bandwidth => app.metrics.down,
        GraphKind::Blocks => app.metrics.blocks_per_second,
        GraphKind::Transactions => app.metrics.transactions_per_second,
    };
    let held = app.error.is_some() || current.is_none();
    let axis = Rect::new(area.x, area.y + chart_height, area.width, 1);
    let left = (plot.end - chrono::Duration::seconds(120))
        .format("%H:%M:%S")
        .to_string();
    let middle = (plot.end - chrono::Duration::seconds(60))
        .format("%H:%M:%S")
        .to_string();
    let right = plot.end.format("%H:%M:%S").to_string();
    frame.render_widget(
        Paragraph::new(format!("{left}  -120s")).style(Style::default().fg(p.muted)),
        axis,
    );
    if area.width > 60 {
        frame.render_widget(
            Paragraph::new(format!("{middle}  -60s")).style(Style::default().fg(p.muted)),
            Rect::new(axis.x + axis.width / 2 - 7, axis.y, 15, 1),
        );
    }
    frame.render_widget(
        Paragraph::new(format!("{right} UTC"))
            .alignment(ratatui::layout::Alignment::Right)
            .style(Style::default().fg(p.muted)),
        Rect::new(
            axis.x + axis.width.saturating_sub(13),
            axis.y,
            axis.width.min(13),
            1,
        ),
    );
    let legend = Line::from(vec![
        Span::styled(
            if upload {
                format!(
                    "Peak {} · scale {}   ",
                    graph_rate(app.graph, peak),
                    graph_rate(app.graph, scale)
                )
            } else {
                format!("Chart peak {}   ", graph_rate(app.graph, peak))
            },
            Style::default()
                .fg(p.foreground)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("■ 0–33%  ", Style::default().fg(p.accent)),
        Span::styled("■ 33–66%  ", Style::default().fg(p.green)),
        Span::styled("■ 66–100% of scale", Style::default().fg(p.yellow)),
    ]);
    frame.render_widget(
        Paragraph::new(legend),
        Rect::new(area.x, area.y + chart_height + 1, area.width, 1),
    );
    let state = if app.error.is_some() {
        "RECONNECTING · HISTORY HELD"
    } else if held {
        "WAITING FOR SAMPLES · HISTORY HELD"
    } else if upload {
        "LIVE · blank = no sample · ◆ upload / shared scale"
    } else {
        "LIVE · blank = no sample · dots = measured zero"
    };
    frame.render_widget(
        Paragraph::new(state).style(Style::default().fg(if held { p.yellow } else { p.muted })),
        Rect::new(area.x, area.y + chart_height + 2, area.width, 1),
    );
}
fn compact(frame: &mut Frame, area: Rect, app: &App, s: &Snapshot, p: &Palette) {
    let mut lines = Vec::new();
    if s.chain.initialblockdownload && s.chain.headers == 0 && presync(s) > 0 {
        lines.push(Line::from(Span::styled(
            format!("Pre-sync: {} headers examined", number(presync(s))),
            Style::default().fg(p.accent),
        )));
    } else if s.chain.initialblockdownload {
        let progress = if s.chain.verificationprogress.is_finite() {
            s.chain.verificationprogress.clamp(0.0, 1.0)
        } else {
            0.0
        };
        lines.push(Line::from(Span::styled(
            format!("Verification  {:.2}% estimated work", progress * 100.0),
            Style::default().fg(p.accent).add_modifier(Modifier::BOLD),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "Independent verification · up to date",
            Style::default().fg(p.green),
        )));
    }
    lines.push(Line::from(format!(
        "Blocks {} · Headers {}",
        number(s.chain.blocks),
        number(s.chain.headers)
    )));
    lines.push(Line::from(format!(
        "{} peers · {}",
        s.network.connections,
        rate(app)
    )));
    lines.push(Line::from(format!("Chain: {}", date(s.chain.time))));
    lines.push(Line::from(format!(
        "Disk free: {}",
        optional(s.disk_available_bytes)
    )));
    if !s.chain.initialblockdownload {
        lines.push(Line::from(format!(
            "Mempool: {}",
            s.mempool
                .as_ref()
                .map(|m| format!("{} transactions", number(m.size)))
                .unwrap_or_else(|| "Unavailable".into())
        )));
    } else {
        lines.push(Line::from(if app.error.is_some() {
            "Estimate unavailable · stale data".into()
        } else {
            app.metrics
                .eta()
                .unwrap_or_else(|| "Measuring verification rate…".into())
        }));
    }
    frame.render_widget(Paragraph::new(lines), area);
}
fn peers(frame: &mut Frame, area: Rect, s: &Snapshot, p: &Palette) {
    let Some(peers) = &s.peers else {
        frame.render_widget(
            Paragraph::new(format!(
                "Peer details unavailable · {} peers connected",
                s.network.connections
            ))
            .block(block("CONNECTED PEERS", p)),
            area,
        );
        return;
    };
    if peers.is_empty() {
        frame.render_widget(
            Paragraph::new("No peers reported. Core will seek outbound connections.")
                .wrap(Wrap { trim: true })
                .block(block("CONNECTED PEERS", p)),
            area,
        );
        return;
    }
    let wide = area.width >= 75;
    let rows = peers.iter().map(|peer| {
        let direction = if peer.inbound { "IN" } else { "OUT" };
        let ping = if peer.pingtime.is_finite() && peer.pingtime > 0.0 {
            format!("{:.0} ms", peer.pingtime * 1000.0)
        } else {
            "—".into()
        };
        let mut fields = vec![direction.to_string(), safe(&peer.addr), ping];
        if wide {
            fields.push(safe(&peer.subver));
        }
        Row::new(fields)
    });
    let widths = if wide {
        vec![
            Constraint::Length(4),
            Constraint::Percentage(50),
            Constraint::Length(9),
            Constraint::Min(1),
        ]
    } else {
        vec![
            Constraint::Length(4),
            Constraint::Min(1),
            Constraint::Length(8),
        ]
    };
    let headers = if wide {
        vec!["Dir", "Endpoint", "Ping", "Client"]
    } else {
        vec!["Dir", "Endpoint", "Ping"]
    };
    frame.render_widget(
        Table::new(rows, widths)
            .header(
                Row::new(headers)
                    .style(Style::default().fg(p.muted))
                    .bottom_margin(1),
            )
            .column_spacing(1)
            .block(block("CONNECTED PEERS", p)),
        area,
    );
}
fn storage(frame: &mut Frame, area: Rect, s: &Snapshot, p: &Palette) {
    let card = block("STORAGE & RESOURCES", p);
    let inner = card.inner(area);
    frame.render_widget(card, area);
    let mut rows = vec![
        kv(
            "Mode",
            if s.chain.pruned { "Pruned" } else { "Archival" }.into(),
            p,
        ),
        kv("Block & undo files", human_bytes(s.chain.size_on_disk), p),
        kv("Core databases", optional(s.datadir_bytes), p),
        kv("Filesystem free", optional(s.disk_available_bytes), p),
        kv("Filesystem total", optional(s.disk_total_bytes), p),
    ];
    if s.chain.pruned {
        rows.push(kv(
            "Block prune target",
            human_bytes(s.chain.prune_target_size),
            p,
        ));
    }
    rows.extend([
        kv("Available RAM", optional(s.memory_available_bytes), p),
        kv(
            "Core uptime",
            s.uptime_seconds
                .map(duration)
                .unwrap_or_else(|| "Unavailable".into()),
            p,
        ),
        kv("Core version", safe(&s.network.subversion), p),
    ]);
    if inner.height >= 12 {
        rows.extend([
            Line::from(""),
            Line::from(Span::styled(
                if s.chain.pruned {
                    "All blocks are verified; older block files are removed."
                } else {
                    "Historic blocks are retained by this archival node."
                },
                Style::default().fg(p.muted),
            )),
            Line::from(Span::styled(
                "Closing Chainlight leaves Bitcoin Core running.",
                Style::default().fg(p.muted),
            )),
        ]);
    }
    frame.render_widget(Paragraph::new(rows), inner);
}
fn startup(frame: &mut Frame, area: Rect, p: &Palette) {
    frame.render_widget(
        Paragraph::new(vec![
            Line::from("Connecting to Bitcoin Core’s local RPC interface."),
            Line::from("Chainlight retries automatically every two seconds."),
            Line::from(""),
            Line::from("Start Core: systemctl --user start bitcoin-node"),
            Line::from("Logs: journalctl --user -u bitcoin-node -n 30"),
        ])
        .wrap(Wrap { trim: true })
        .block(block("GETTING YOUR NODE READY", p)),
        area,
    );
}
fn help(frame: &mut Frame, area: Rect, p: &Palette) {
    frame.render_widget(Paragraph::new(vec![Line::from("Tab / ← → / h l    Switch pages"),Line::from("1 2 3              Jump to a page"),Line::from("g                  Bandwidth / blocks/sec / verified tx/sec"),Line::from("u                  Toggle upload on bandwidth graph"),Line::from("r                  Refresh now"),Line::from("? / Escape         Show / close help"),Line::from("q / Ctrl+C         Quit; Core keeps running"),Line::from(""),Line::from(Span::styled("WHAT SYNC MEANS",Style::default().fg(p.accent))),Line::from("Headers establish the best-work chain. Blocks are downloaded and verified locally."),Line::from("Verification % estimates work completed, not bytes downloaded. Pruning removes older verified blocks."),Line::from("Header pre-sync can advance before Core reports accepted headers."),Line::from("Completion estimates use measured verification progress; the rate may change.")]).wrap(Wrap{trim:true}).block(block("HELP",p)),area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ChainInfo, Mempool, NetworkInfo, Peer, Traffic};
    use ratatui::{Terminal, backend::TestBackend};
    fn snapshot() -> Snapshot {
        Snapshot {
            observed_at: Utc::now(),
            chain: ChainInfo {
                chain: "main".into(),
                blocks: 211_172,
                headers: 900_001,
                verificationprogress: 0.348,
                time: 1_520_000_000,
                initialblockdownload: true,
                pruned: true,
                ..Default::default()
            },
            network: NetworkInfo {
                networkactive: true,
                connections: 10,
                connections_out: 10,
                ..Default::default()
            },
            traffic: Some(Traffic {
                received: 1024,
                sent: 100,
            }),
            peers: Some(vec![]),
            mempool: Some(Mempool {
                size: 42,
                ..Default::default()
            }),
            disk_available_bytes: Some(1024 * 1024 * 1024),
            disk_total_bytes: None,
            datadir_bytes: None,
            memory_available_bytes: None,
            uptime_seconds: None,
            verified_transactions: None,
            fee_estimates: None,
            fees_observed_at: None,
            notes: vec![],
        }
    }
    fn render(app: &App, w: u16, h: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal
            .draw(|frame| draw(frame, app, &Palette::default()))
            .unwrap();
        let buffer = terminal.backend().buffer();
        (0..h)
            .map(|y| (0..w).map(|x| buffer[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }
    #[test]
    fn time_axis_has_real_gaps_and_uses_time_weighted_bins() {
        let end = Utc::now();
        let mut app = App::default();
        for (a, b, value) in [(120, 110, 100.0), (10, 0, 200.0)] {
            app.metrics.samples[0].push_back(crate::metrics::RateSample {
                start: end - chrono::Duration::seconds(a),
                end: end - chrono::Duration::seconds(b),
                download: Some(value),
                upload: Some(value / 2.0),
                blocks: None,
                transactions: None,
            });
        }
        let plot = time_plot(&app, 12).unwrap();
        assert_eq!(plot.bins[0], Some((100.0, Some(50.0))));
        assert_eq!(plot.bins[11], Some((200.0, Some(100.0))));
        assert!(plot.bins[1..11].iter().all(Option::is_none));
        app.metrics.pause();
        assert_eq!(time_plot(&app, 12).unwrap().end, end);
        app.graph = GraphKind::Transactions;
        assert!(time_plot(&app, 12).is_none());
    }

    #[test]
    fn large_layout_changes_priorities_with_core_ibd_state() {
        let mut app = App::default();
        app.apply(Ok(snapshot()));
        let ibd = render(&app, 160, 60);
        assert!(ibd.contains("CHAIN VERIFICATION / INITIAL BLOCK DOWNLOAD"));
        assert!(ibd.contains("34.80% ESTIMATED VERIFICATION WORK"));
        assert_eq!(
            ibd.matches('■').count() + ibd.matches('□').count() + ibd.matches('◧').count(),
            100
        );
        assert!(ibd.contains("ESTIMATED HOURS REMAINING"));
        let tiled = render(&app, 100, 40);
        assert_eq!(
            tiled.matches('■').count() + tiled.matches('□').count() + tiled.matches('◧').count(),
            100
        );
        assert!(ibd.contains("NODE ACTIVITY"));
        assert!(!ibd.contains("FEE OUTLOOK"));
        let mut live = snapshot();
        live.chain.initialblockdownload = false;
        live.chain.time = Utc::now().timestamp() - 600;
        live.fee_estimates = Some(vec![crate::types::FeeEstimate {
            target: 2,
            blocks: Some(2),
            sat_per_vbyte: Some(1.5),
        }]);
        app.apply(Ok(live));
        let synced = render(&app, 160, 60);
        assert!(synced.contains("LIVE NODE / INDEPENDENTLY VERIFIED"));
        assert!(synced.contains("VERIFIED TIP #211,172"));
        assert!(synced.contains("Time since block"));
        assert!(synced.contains("FEE OUTLOOK / sat/vB"));
        assert!(synced.contains("1.50"));
        assert!(synced.contains("SYNC COMPLETE"));
        assert!(!synced.contains("THE VERIFICATION JOURNEY"));
        assert!(!synced.contains("ESTIMATED HOURS REMAINING"));
    }

    #[test]
    fn histogram_survives_rpc_failure_and_missing_traffic() {
        fn network_screen(app: &App) -> Vec<String> {
            let mut terminal = Terminal::new(TestBackend::new(100, 26)).unwrap();
            terminal
                .draw(|frame| {
                    network(
                        frame,
                        frame.area(),
                        app,
                        app.current.as_ref().unwrap(),
                        &Palette::default(),
                    );
                })
                .unwrap();
            let buffer = terminal.backend().buffer();
            (0..26)
                .map(|y| (0..100).map(|x| buffer[(x, y)].symbol()).collect())
                .collect()
        }
        let mut app = App {
            current: Some(snapshot()),
            ..Default::default()
        };
        let end = app.current.as_ref().unwrap().observed_at;
        for (index, rate) in [1000.0, 3000.0, 0.0, 2000.0].iter().enumerate() {
            app.metrics.samples[0].push_back(crate::metrics::RateSample {
                start: end - chrono::Duration::seconds(8 - index as i64 * 2),
                end: end - chrono::Duration::seconds(6 - index as i64 * 2),
                download: Some(*rate),
                upload: Some(10.0),
                blocks: Some(1.0),
                transactions: Some(5.0),
            });
        }
        app.metrics.down = Some(2000.0);
        let before = network_screen(&app);
        app.apply(Err("RPC unavailable".into()));
        let failed = network_screen(&app);
        assert_eq!(&before[4..24], &failed[4..24]);
        assert!(failed[24].contains("RECONNECTING · HISTORY HELD"));
        let mut missing = snapshot();
        missing.traffic = None;
        app.apply(Ok(missing));
        let unavailable = network_screen(&app);
        assert_eq!(&before[4..24], &unavailable[4..24]);
        assert!(unavailable[24].contains("WAITING FOR SAMPLES · HISTORY HELD"));
    }
    #[test]
    fn startup_and_core_error_are_visible() {
        let app = App::default();
        assert!(render(&app, 80, 24).contains("GETTING YOUR NODE READY"));
        let app = App {
            error: Some("RPC authentication failed".into()),
            ..Default::default()
        };
        let screen = render(&app, 80, 24);
        assert!(screen.contains("WAITING FOR CORE"));
        assert!(screen.contains("RPC authentication failed"));
    }
    #[test]
    fn estimated_verification_is_not_block_height_ratio() {
        let app = App {
            current: Some(snapshot()),
            ..Default::default()
        };
        let screen = render(&app, 80, 24);
        assert!(screen.contains("34.80%"));
        assert!(screen.contains("211,172"));
        assert!(!screen.contains("23.46%"));
    }
    #[test]
    fn headers_presync_before_accepted_headers() {
        let mut s = snapshot();
        s.chain.headers = 0;
        s.chain.blocks = 0;
        s.peers = Some(vec![Peer {
            presynced_headers: 910_000,
            ..Default::default()
        }]);
        let app = App {
            current: Some(s),
            ..Default::default()
        };
        let screen = render(&app, 80, 24);
        assert!(screen.contains("HEADER PRE-SYNC"));
        assert!(screen.contains("910,000 headers examined"));
        assert!(screen.contains("verification has not started"));
    }
    #[test]
    fn synced_node_has_mempool_and_missing_fields_are_unavailable() {
        let mut s = snapshot();
        s.chain.initialblockdownload = false;
        s.mempool = None;
        s.traffic = None;
        let app = App {
            current: Some(s),
            ..Default::default()
        };
        let screen = render(&app, 100, 32);
        assert!(screen.contains("LIVE NODE"));
        assert!(screen.contains("Mempool"));
        assert!(screen.matches("Unavailable").count() >= 4);
    }
    #[test]
    fn lost_rpc_marks_retained_data_stale_and_hides_live_rate() {
        let mut app = App {
            current: Some(snapshot()),
            error: Some("RPC timed out".into()),
            ..Default::default()
        };
        app.metrics.down = Some(999_000.0);
        app.metrics.up = Some(500.0);
        let screen = render(&app, 100, 32);
        assert!(screen.contains("STALE DATA"));
        assert!(screen.contains("211,172"));
        assert!(screen.contains("Unavailable · connection lost"));
        assert!(!screen.contains("975.6 KiB"));
    }
    #[test]
    fn compact_and_tiny_terminals_keep_exit_and_state_visible() {
        let app = App {
            current: Some(snapshot()),
            ..Default::default()
        };
        for (w, h) in [(40, 14), (80, 24), (120, 40), (30, 10), (10, 4), (1, 1)] {
            let screen = render(&app, w, h);
            assert_eq!(screen.lines().count(), usize::from(h));
        }
        let screen = render(&app, 40, 14);
        assert!(screen.contains("34.80%"));
        assert!(screen.contains("q quit"));
        assert!(screen.contains("211,172"));
    }
    #[test]
    fn peer_details_sanitized_and_all_pages_render() {
        let mut s = snapshot();
        s.peers = Some(vec![Peer {
            addr: "remote\x1b[31m\npeer".into(),
            subver: "client\u{202e}".into(),
            ..Default::default()
        }]);
        let mut app = App {
            current: Some(s),
            ..Default::default()
        };
        for tab in [1, 2] {
            app.tab = tab;
            let screen = render(&app, 80, 24);
            assert!(!screen.contains('\x1b'));
            assert!(!screen.contains('\u{202e}'));
        }
        app.help = true;
        assert!(render(&app, 80, 24).contains("WHAT SYNC MEANS"));
    }
}
