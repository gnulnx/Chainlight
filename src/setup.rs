use crate::{rpc, theme::Palette};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    Terminal,
    backend::Backend,
    layout::{Constraint, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Connection {
    pub datadir: PathBuf,
    pub rpc_url: String,
}

impl Connection {
    pub fn validate(&self) -> Result<(), String> {
        if !self.datadir.is_absolute() {
            return Err(
                "Use an absolute data directory path (expand ~ to your home directory).".into(),
            );
        }
        rpc::Client::new(&self.rpc_url, self.datadir.clone()).map(|_| ())
    }
}

pub fn config_path() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
    {
        return Ok(path.join("chainlight/config.toml"));
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .map(|p| p.join(".config/chainlight/config.toml"))
        .ok_or_else(|| {
            "Set HOME or an absolute XDG_CONFIG_HOME to locate Chainlight settings".into()
        })
}

pub fn load(path: &Path) -> Result<Option<Connection>, String> {
    match fs::read_to_string(path) {
        Ok(text) => {
            let connection: Connection = toml::from_str(&text).map_err(|_| {
                "Invalid Chainlight config.toml; correct it or move it aside to run setup again"
                    .to_string()
            })?;
            connection.validate()?;
            Ok(Some(connection))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("Could not read Chainlight config.toml".into()),
    }
}

pub fn save(path: &Path, connection: &Connection) -> Result<(), String> {
    connection.validate()?;
    let parent = path.parent().ok_or("Invalid settings path")?;
    fs::create_dir_all(parent).map_err(|_| "Could not create Chainlight settings directory")?;
    let temporary = parent.join(format!(".config-{}.tmp", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(|_| "Could not create temporary Chainlight settings")?;
    let result = (|| {
        let text = toml::to_string_pretty(connection)
            .map_err(|_| "Could not serialize Chainlight settings")?;
        file.write_all(text.as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|_| "Could not write Chainlight settings")?;
        fs::rename(&temporary, path).map_err(|_| "Could not save Chainlight settings")?;
        Ok(())
    })();
    drop(file);
    let _ = fs::remove_file(&temporary);
    result
}

pub struct Form {
    fields: [String; 2],
    selected: usize,
    message: String,
    verified: bool,
    busy: bool,
    guide: bool,
}
impl Form {
    fn new(connection: Connection) -> Self {
        Self {
            fields: [
                connection.datadir.to_string_lossy().into(),
                connection.rpc_url,
            ],
            selected: 0,
            message: "Choose your existing node, then test the connection.".into(),
            verified: false,
            busy: false,
            guide: false,
        }
    }
    fn connection(&self) -> Connection {
        Connection {
            datadir: PathBuf::from(&self.fields[0]),
            rpc_url: self.fields[1].clone(),
        }
    }
    pub fn draw(&self, frame: &mut ratatui::Frame, palette: &Palette) {
        let base = Style::default()
            .fg(palette.foreground)
            .bg(palette.background);
        frame.render_widget(Block::default().style(base), frame.area());
        let areas = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(3),
        ])
        .margin(1)
        .split(frame.area());
        frame.render_widget(
            Paragraph::new("CHAINLIGHT  /  YOUR CHAIN. YOUR VERIFICATION.")
                .style(base.fg(palette.accent).add_modifier(Modifier::BOLD)),
            areas[0],
        );
        let lines = if self.guide {
            vec![
                Line::from("A NEW BITCOIN NODE"),
                Line::from(""),
                Line::from(
                    "1. Install Bitcoin Core from bitcoincore.org, or your trusted package manager.",
                ),
                Line::from(
                    "2. Enable local RPC with server=1 in bitcoin.conf. Keep RPC on loopback.",
                ),
                Line::from(
                    "3. Choose storage: prune=20480 targets 20 GiB of block files; databases need additional room.",
                ),
                Line::from(
                    "   Pruning still checks all history. An archive retains all blocks and needs much more disk space.",
                ),
                Line::from(
                    "4. Start Core. Initial sync can take days; Chainlight shows progress once RPC is ready.",
                ),
                Line::from(""),
                Line::from("Detailed configuration and user service example:"),
                Line::from("github.com/gnulnx/Chainlight/blob/ux-companion/docs/NODE.md"),
                Line::from(""),
                Line::from(
                    "Chainlight connects to Core; this screen does not install or start it.",
                ),
                Line::from("n: Return to connection setup"),
            ]
        } else {
            let mut lines = vec![
                Line::from("CONNECT TO BITCOIN CORE"),
                Line::from(""),
                Line::from("Local cookie authentication · no passwords saved"),
                Line::from(""),
            ];
            for (index, label) in ["Data directory", "RPC URL"].iter().enumerate() {
                let style = if self.selected == index {
                    base.fg(palette.accent).add_modifier(Modifier::BOLD)
                } else {
                    base
                };
                lines.push(Line::from(Span::styled(
                    format!("{} {label}", if self.selected == index { "▶" } else { " " }),
                    style,
                )));
                lines.push(Line::from(Span::styled(self.fields[index].clone(), style)));
                lines.push(Line::from(""));
            }
            lines.push(Line::from(Span::styled(
                self.message.clone(),
                base.fg(if self.verified {
                    palette.green
                } else {
                    palette.yellow
                }),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(
                "F1: New node? Read the setup guide · Ctrl+U: Clear field",
            ));
            lines
        };
        frame.render_widget(
            Paragraph::new(lines)
                .style(base)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" FIRST RUN / CONNECTION "),
                )
                .wrap(Wrap { trim: false }),
            areas[1],
        );
        frame.render_widget(
            Paragraph::new(if self.verified {
                "Enter: Save and open dashboard · Tab: Switch field · Esc: Cancel"
            } else {
                "Enter: Test connection · Tab: Switch field · Esc: Cancel"
            })
            .style(base)
            .wrap(Wrap { trim: false }),
            areas[2],
        );
    }
}

pub fn run<B: Backend>(
    terminal: &mut Terminal<B>,
    connection: Connection,
    path: &Path,
    theme: Option<&Path>,
    stop: &Arc<AtomicBool>,
) -> Result<Option<Connection>, String> {
    let mut form = Form::new(connection);
    let (tx, rx) = mpsc::channel();
    while !stop.load(Ordering::Relaxed) {
        if let Ok(result) = rx.try_recv() {
            form.busy = false;
            match result {
                Ok(chain) => {
                    form.verified = true;
                    form.message = format!(
                        "Connected to Bitcoin {chain}. Enter to save and open the dashboard."
                    );
                }
                Err(error) => form.message = error,
            }
        }
        terminal
            .draw(|frame| form.draw(frame, &Palette::load(theme)))
            .map_err(|e| e.to_string())?;
        if !event::poll(Duration::from_millis(150)).map_err(|e| e.to_string())? {
            continue;
        }
        if let Event::Key(key) = event::read().map_err(|e| e.to_string())? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if key.code == KeyCode::Esc
                || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
            {
                return Ok(None);
            }
            if form.busy {
                continue;
            }
            match key.code {
                KeyCode::Tab | KeyCode::BackTab => form.selected = 1 - form.selected,
                KeyCode::Char('n') if form.guide => form.guide = false,
                KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::ALT) => {
                    form.guide = !form.guide
                }
                KeyCode::F(1) => form.guide = !form.guide,
                KeyCode::Enter if !form.guide => {
                    let connection = form.connection();
                    if form.verified {
                        match save(path, &connection) {
                            Ok(()) => return Ok(Some(connection)),
                            Err(error) => {
                                form.message = error;
                                continue;
                            }
                        }
                    }
                    match connection.validate() {
                        Err(error) => form.message = error,
                        Ok(()) => {
                            form.busy = true;
                            form.message =
                                "Checking Core… Esc cancels. No node settings are changed.".into();
                            let sender = tx.clone();
                            std::thread::spawn(move || {
                                let result =
                                    rpc::Client::new(&connection.rpc_url, connection.datadir)
                                        .and_then(|mut client| client.snapshot())
                                        .map(|snapshot| snapshot.chain.chain);
                                let _ = sender.send(result);
                            });
                        }
                    }
                }
                KeyCode::Backspace if !form.guide => {
                    form.fields[form.selected].pop();
                    form.verified = false;
                }
                KeyCode::Char('u')
                    if key.modifiers.contains(KeyModifiers::CONTROL) && !form.guide =>
                {
                    form.fields[form.selected].clear();
                    form.verified = false;
                }
                KeyCode::Char(ch)
                    if !form.guide
                        && !ch.is_control()
                        && !key
                            .modifiers
                            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                        && form.fields[form.selected].len() < 4096 =>
                {
                    form.fields[form.selected].push(ch);
                    form.verified = false;
                }
                _ => {}
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn config_round_trip_has_no_cookie_and_rejects_unsafe_endpoints() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("chainlight/config.toml");
        assert_eq!(load(&path).unwrap(), None);
        let connection = Connection {
            datadir: temp.path().join("node with spaces"),
            rpc_url: "http://127.0.0.1:8332".into(),
        };
        save(&path, &connection).unwrap();
        assert_eq!(load(&path).unwrap(), Some(connection.clone()));
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let original = fs::read(&path).unwrap();
        for endpoint in [
            "http://192.168.1.1:8332",
            "http://user:secret@127.0.0.1:8332",
        ] {
            let mut invalid = connection.clone();
            invalid.rpc_url = endpoint.into();
            assert!(save(&path, &invalid).is_err());
            assert_eq!(fs::read(&path).unwrap(), original);
        }
        assert!(!String::from_utf8(original).unwrap().contains("cookie"));
    }
    #[test]
    fn corrupt_settings_are_not_silently_ignored() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config.toml");
        fs::write(&path, "not toml").unwrap();
        assert!(load(&path).is_err());
    }
    #[test]
    fn setup_explains_storage_and_connection() {
        let mut form = Form::new(Connection {
            datadir: PathBuf::from("/tmp/bitcoin"),
            rpc_url: "http://127.0.0.1:8332".into(),
        });
        let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(100, 32)).unwrap();
        terminal
            .draw(|frame| form.draw(frame, &Palette::default()))
            .unwrap();
        assert!(
            terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|c| c.symbol())
                .collect::<String>()
                .contains("CONNECT TO BITCOIN CORE")
        );
        form.guide = true;
        terminal
            .draw(|frame| form.draw(frame, &Palette::default()))
            .unwrap();
        assert!(
            terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|c| c.symbol())
                .collect::<String>()
                .contains("Pruning still checks all history")
        );
    }
}
