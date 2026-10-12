# Install Chainlight

Chainlight is a Rust terminal dashboard for an existing Bitcoin Core node. It
reads local Core RPC using the data directory's authentication cookie. It does
not start or stop Core. See [NODE.md](NODE.md) for the node configuration example.

## Build from source

Install Rust 1.88 or newer with Cargo, then build from the project directory:

```sh
cargo build --locked --release
python3 integrations/install-user.py target/release/chainlight
~/.local/bin/chainlight
```

The installer requires Python 3; the installed dashboard does not. It installs
`~/.local/bin/chainlight`, a **Chainlight** application entry under
`${XDG_DATA_HOME:-$HOME/.local/share}/applications`, and the Chainlight SVG and
PNG icons into the user hicolor icon theme. It refuses existing destinations
unless you add `--replace`. Updates atomically replace the executable, allowing
a running dashboard to continue using its previous binary until you restart it.

## Omarchy

Build as above, then install the separate Omarchy launcher:

```sh
python3 integrations/install-user.py target/release/chainlight --omarchy
```

On Omarchy 4.0.4, press **Super + Alt + Space**, choose the Apps menu entry for
**Chainlight**, and launch it. You can also run:

```sh
~/.local/bin/chainlight-omarchy
```

The launcher delegates to Omarchy's own
`omarchy-launch-tui --app-id=org.chainlight.App`, which selects your configured
terminal and font. Chainlight reads the current
`~/.local/state/omarchy/current/theme/colors.toml`, respecting `XDG_STATE_HOME`.
The palette is reloaded while running; a missing palette falls back to the
terminal's ANSI colors. Terminal font changes may require restarting the window.

The desktop entry, wrapper and icons are user-local files. Installing them does
not modify Omarchy's packaged defaults or create a competing menu. They remain
in place across normal Omarchy updates. Omarchy may change its launcher API or
shortcuts in a future release; integration compatibility must then be checked.

No custom keybinding is required. If you want one, first check existing bindings
and choose an unused combination in your own Hyprland configuration. Bind the
absolute path to `chainlight-omarchy`; do not edit packaged default bindings.

`chainlight-omarchy` opens a graphical window and needs an Omarchy desktop
session. Run `chainlight` directly within terminals and SSH sessions.

## SSH and RPC configuration

For a node running on another machine:

```sh
ssh -t USER@NODE_HOST /home/USER/.local/bin/chainlight
```

For a nondefault mainnet data directory or local RPC port:

```sh
chainlight --datadir /path/to/bitcoin-data --rpc-url http://127.0.0.1:8332
```

The data directory must contain the running node's `.cookie`. Do not copy the
cookie into source control or expose Core RPC to the network. Run Chainlight on
the node over SSH instead. `q` exits the dashboard while Core keeps running.

Diagnostic commands:

```sh
chainlight --snapshot
chainlight --render --width 100
```

They collect live data once and require a reachable node. Use `chainlight --help`
for all flags.

## Local Arch package

The PKGBUILD builds a local source archive using Cargo and the committed lockfile.
It is a development packaging recipe; it has not been submitted to the AUR and
there is no package release download to install.

From the project root:

```sh
python3 integrations/arch/prepare-source.py
cd integrations/arch
makepkg
```

Arch's `rust` package provides Cargo. The build may download dependencies pinned
by `Cargo.lock`. Install the generated package with normal Arch package tools.
The packaged application menu entry uses the portable terminal launcher. On
Omarchy, invoke `chainlight-omarchy` or install the user desktop integration above.

The preparation script creates a deterministic source archive and pins its
SHA-256 checksum in the PKGBUILD. Re-run it after source changes. The archive
includes Rust source, lockfile, icons and public installation/node documentation;
it excludes node data, downloaded Core, build output and local validation notes.
Before AUR publication, use a published versioned source URL and test package
installation, launcher behavior and removal on clean Arch.

## Remove a user installation

Remove these files only if you installed Chainlight there:

```sh
rm ~/.local/bin/chainlight ~/.local/bin/chainlight-omarchy
rm "${XDG_DATA_HOME:-$HOME/.local/share}/applications/chainlight.desktop"
rm "${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/scalable/apps/chainlight.svg"
for size in 16 32 48 64 128 256; do
  rm "${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/${size}x${size}/apps/chainlight.png"
done
```

Remove a package installation through pacman. Dashboard removal leaves Core, its
service, chain data and wallets intact. The example Core service retains the name
`bitcoin-node.service`; changing dashboard versions does not restart Core.
