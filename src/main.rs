#![forbid(unsafe_code)]

mod app;
mod metrics;
mod rpc;
mod theme;
mod types;
mod ui;
mod utils;

use app::App;
use clap::Parser;
use crossterm::{
    cursor::Show,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{
        EnterAlternateScreen, LeaveAlternateScreen, SetTitle, disable_raw_mode, enable_raw_mode,
    },
};
use ratatui::{
    Terminal,
    backend::{CrosstermBackend, TestBackend},
};
use std::{
    io::{self, IsTerminal},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

#[derive(Parser, Debug)]
#[command(version, about = "Your Bitcoin chain. Your independent verification.")]
struct Args {
    /// Bitcoin Core data directory containing the node's RPC cookie
    #[arg(long)]
    datadir: Option<PathBuf>,
    /// Loopback RPC URL; remote nodes require an SSH tunnel
    #[arg(long, default_value = "http://127.0.0.1:8332")]
    rpc_url: String,
    /// Optional Omarchy colors.toml palette, reloaded while running
    #[arg(long)]
    theme_file: Option<PathBuf>,
    /// Print one live JSON observation and exit
    #[arg(long, conflicts_with = "render")]
    snapshot: bool,
    /// Print one live text dashboard and exit
    #[arg(long)]
    render: bool,
    /// Width for one-shot text rendering
    #[arg(long, default_value_t = 100, value_parser = clap::value_parser!(u16).range(20..=300))]
    width: u16,
    /// Height for one-shot text rendering
    #[arg(long, default_value_t = 34, value_parser = clap::value_parser!(u16).range(10..=120))]
    height: u16,
}

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, Show);
}
struct TerminalGuard;
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore_terminal();
    }
}

fn run(args: Args) -> Result<(), String> {
    let datadir = args.datadir.unwrap_or_else(|| {
        PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".bitcoin")
    });
    let mut client = rpc::Client::new(&args.rpc_url, datadir)?;
    if args.snapshot || args.render {
        let mut app = App::default();
        let snapshot = client.snapshot()?;
        if args.snapshot {
            serde_json::to_writer_pretty(io::stdout().lock(), &snapshot)
                .map_err(|_| "Could not write snapshot".to_string())?;
            println!();
        } else {
            app.apply(Ok(snapshot));
            let mut terminal = Terminal::new(TestBackend::new(args.width, args.height))
                .map_err(|e| e.to_string())?;
            let palette = theme::Palette::load(args.theme_file.as_deref());
            terminal
                .draw(|frame| ui::draw(frame, &app, &palette))
                .map_err(|e| e.to_string())?;
            let buffer = terminal.backend().buffer();
            for y in 0..args.height {
                let line: String = (0..args.width).map(|x| buffer[(x, y)].symbol()).collect();
                println!("{}", line.trim_end());
            }
        }
        return Ok(());
    }
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("Use a terminal (ssh -t for SSH), or use --snapshot / --render".into());
    }
    enable_raw_mode().map_err(|e| e.to_string())?;
    let _guard = TerminalGuard;
    execute!(
        io::stdout(),
        EnterAlternateScreen,
        SetTitle("Chainlight — Bitcoin Node")
    )
    .map_err(|e| e.to_string())?;
    let old_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore_terminal();
        old_hook(info);
    }));

    let stop = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        signal_hook::flag::register(signal, stop.clone()).map_err(|e| e.to_string())?;
    }
    let (results_tx, results_rx) = mpsc::sync_channel(1);
    let (refresh_tx, refresh_rx) = mpsc::sync_channel(1);
    let worker_stop = stop.clone();
    thread::spawn(move || {
        while !worker_stop.load(Ordering::Relaxed) {
            if results_tx.send(client.snapshot()).is_err() {
                break;
            }
            if matches!(
                refresh_rx.recv_timeout(Duration::from_secs(2)),
                Err(mpsc::RecvTimeoutError::Disconnected)
            ) {
                break;
            }
        }
    });
    let mut terminal =
        Terminal::new(CrosstermBackend::new(io::stdout())).map_err(|e| e.to_string())?;
    let mut app = App::default();
    while !stop.load(Ordering::Relaxed) {
        while let Ok(result) = results_rx.try_recv() {
            app.apply(result);
        }
        let palette = theme::Palette::load(args.theme_file.as_deref());
        terminal
            .draw(|frame| ui::draw(frame, &app, &palette))
            .map_err(|e| e.to_string())?;
        if event::poll(Duration::from_millis(150)).map_err(|e| e.to_string())?
            && let Event::Key(key) = event::read().map_err(|e| e.to_string())?
            && key.kind == KeyEventKind::Press
            && handle_key(&mut app, key.code, key.modifiers, &refresh_tx)
        {
            break;
        }
    }
    stop.store(true, Ordering::Relaxed);
    Ok(())
}

fn handle_key(
    app: &mut App,
    key: KeyCode,
    modifiers: KeyModifiers,
    refresh: &mpsc::SyncSender<()>,
) -> bool {
    match key {
        KeyCode::Char('q') => return true,
        KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => return true,
        KeyCode::Char('?') => app.help = !app.help,
        KeyCode::Esc => app.help = false,
        KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => app.tab = (app.tab + 1) % 3,
        KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => app.tab = (app.tab + 2) % 3,
        KeyCode::Char('1') => app.tab = 0,
        KeyCode::Char('2') => app.tab = 1,
        KeyCode::Char('3') => app.tab = 2,
        KeyCode::Char('r') => {
            let _ = refresh.try_send(());
        }
        _ => {}
    }
    false
}
fn main() {
    if let Err(message) = run(Args::parse()) {
        eprintln!("Chainlight: {message}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn navigation_and_quitting_only_affect_the_dashboard() {
        let (tx, rx) = mpsc::sync_channel(1);
        let mut app = App::default();
        assert!(!handle_key(&mut app, KeyCode::Tab, KeyModifiers::NONE, &tx));
        assert_eq!(app.tab, 1);
        handle_key(&mut app, KeyCode::Char('?'), KeyModifiers::NONE, &tx);
        assert!(app.help);
        handle_key(&mut app, KeyCode::Esc, KeyModifiers::NONE, &tx);
        assert!(!app.help);
        handle_key(&mut app, KeyCode::Char('r'), KeyModifiers::NONE, &tx);
        handle_key(&mut app, KeyCode::Char('r'), KeyModifiers::NONE, &tx);
        assert_eq!(rx.try_recv(), Ok(()));
        assert!(rx.try_recv().is_err());
        assert!(handle_key(
            &mut app,
            KeyCode::Char('q'),
            KeyModifiers::NONE,
            &tx
        ));
        assert!(handle_key(
            &mut app,
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
            &tx
        ));
    }
}
