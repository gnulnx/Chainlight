use crate::{
    app::App,
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
    widgets::{Block, Borders, Gauge, Paragraph, Row, Sparkline, Table, Tabs, Wrap},
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
                "← → / Tab pages   r refresh   ? help   q quit"
            } else {
                "1 2 3 pages · r · ? help · q quit"
            }),
            Line::from(observed),
        ])
        .style(Style::default().fg(palette.muted)),
        chunks[3],
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
    // Every cube is one percent of Core's estimated work, not one block.
    let progress = s.chain.verificationprogress;
    if !progress.is_finite() {
        return;
    }
    let filled = (progress.clamp(0.0, 1.0) * 100.0).floor() as usize;
    let rows = if area.height >= 24 { 10 } else { 5 };
    let columns = 100 / rows;
    if usize::from(area.width) < columns * 3 {
        return;
    }
    let mut lines = vec![Line::from(Span::styled(
        "VERIFICATION / WORK GRID",
        Style::default().fg(p.accent),
    ))];
    lines.push(Line::from(""));
    for row in 0..rows {
        for layer in 0..2 {
            let spans = (0..columns)
                .map(|col| {
                    let done = row * columns + col < filled;
                    let cell = match (done, layer) {
                        (true, 0) => "▄█ ",
                        (true, _) => "▀▓ ",
                        (false, 0) => "·  ",
                        _ => "   ",
                    };
                    Span::styled(
                        cell,
                        Style::default().fg(if done { p.green } else { p.muted }),
                    )
                })
                .collect::<Vec<_>>();
            lines.push(Line::from(spans));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "1 cube = 1% estimated work",
        Style::default().fg(p.muted),
    )));
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
    let rows = Layout::vertical([Constraint::Length(9), Constraint::Min(5)]).split(area);
    chain(frame, rows[0], app, s, p);
    if area.width >= 95 {
        let cards = Layout::horizontal([Constraint::Percentage(62), Constraint::Percentage(38)])
            .spacing(1)
            .split(rows[1]);
        network(frame, cards[0], app, s, p);
        let card = block("RESOURCES", p);
        let inner = card.inner(cards[1]);
        frame.render_widget(card, cards[1]);
        frame.render_widget(
            Paragraph::new(vec![
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
            ]),
            Rect::new(inner.x, inner.y, inner.width, inner.height.min(5)),
        );
        if inner.height >= 16 && inner.width >= 36 {
            voxel_work(
                frame,
                Rect::new(inner.x, inner.y + 7, inner.width, inner.height - 7),
                s,
                p,
            );
        }
    } else {
        network(frame, rows[1], app, s, p)
    }
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
    let card = block("NETWORK", p);
    let inner = card.inner(area);
    frame.render_widget(card, area);
    let text = vec![
        kv(
            "Peers",
            format!(
                "{} connected · {} out · {} in",
                s.network.connections, s.network.connections_out, s.network.connections_in
            ),
            p,
        ),
        kv("Transfer", rate(app), p),
        kv(
            "Received by Core",
            s.traffic
                .as_ref()
                .map(|t| format!("{} since node start", human_bytes(t.received)))
                .unwrap_or_else(|| "Unavailable".into()),
            p,
        ),
    ];
    frame.render_widget(Paragraph::new(text), inner);
    if inner.height >= 4 {
        let graph = Rect::new(
            inner.x,
            inner.y + 3,
            inner.width,
            (inner.height - 3).min(10),
        );
        if app.error.is_some() || s.traffic.is_none() {
            frame.render_widget(
                Paragraph::new("Network activity unavailable").style(Style::default().fg(p.muted)),
                graph,
            )
        } else if app.metrics.rates.len() < 2 {
            frame.render_widget(
                Paragraph::new("Collecting network activity…").style(Style::default().fg(p.muted)),
                graph,
            )
        } else {
            let rates: Vec<u64> = app
                .metrics
                .rates
                .iter()
                .rev()
                .take(usize::from(inner.width))
                .copied()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            frame.render_widget(
                Sparkline::default()
                    .data(&rates)
                    .style(Style::default().fg(p.accent)),
                graph,
            )
        }
    }
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
    frame.render_widget(Paragraph::new(vec![Line::from("Tab / ← → / h l    Switch pages"),Line::from("1 2 3              Jump to a page"),Line::from("r                  Refresh now"),Line::from("? / Escape         Show / close help"),Line::from("q / Ctrl+C         Quit; Core keeps running"),Line::from(""),Line::from(Span::styled("WHAT SYNC MEANS",Style::default().fg(p.accent))),Line::from("Headers establish the best-work chain. Blocks are downloaded and verified locally."),Line::from("Verification % estimates work completed, not bytes downloaded. Pruning removes older verified blocks."),Line::from("Header pre-sync can advance before Core reports accepted headers."),Line::from("Completion estimates use measured verification progress; the rate may change.")]).wrap(Wrap{trim:true}).block(block("HELP",p)),area);
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
