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

## Published Linux x86_64 bundle

Download `chainlight-VERSION-linux-x86_64.tar.gz` and `SHA256SUMS` from the
[release page](https://github.com/gnulnx/Chainlight/releases). In the download
directory, verify the archive before extracting it:

```sh
sha256sum --ignore-missing --check SHA256SUMS
tar -xzf chainlight-VERSION-linux-x86_64.tar.gz
cd chainlight-VERSION-linux-x86_64
python3 integrations/install-user.py ./chainlight --omarchy
```

Replace VERSION with the release version. Omit `--omarchy` for other desktops.
For updates, add `--replace`. The bundle requires Linux x86_64 with glibc 2.35 or
newer and Python 3 for installation; Rust and Cargo are not needed to run it.

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

On first launch, if the default node's cookie is absent and no connection flags
were supplied, Chainlight opens a connection screen. Tab selects a field,
Ctrl+U clears it; enter the absolute Core data directory and loopback RPC URL.
Enter tests the node; a second Enter saves and opens the dashboard. Esc cancels
without saving. F1 opens the new-node guide.

Run `chainlight --setup` to review a connection later or correct invalid settings.
Settings live in `${XDG_CONFIG_HOME:-$HOME/.config}/chainlight/config.toml`, mode
0600. Only the directory and URL are saved, never the cookie or a password.
Connection flags override saved settings. Snapshot/render commands never open
setup or write settings. Existing default nodes open directly into the dashboard.
Core installation and startup remain separate; see [NODE.md](NODE.md).

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

## Arch package

The PKGBUILD builds a local source archive using Cargo and the committed lockfile.
The recipe targets a versioned GitHub release source archive. Download the
release's `PKGBUILD` into an empty directory and run `makepkg -si` there.
This compiles the pinned source and installs the resulting package through
pacman. No AUR account is required to use the recipe. AUR listing is a separate
publication step; do not assume `yay -S chainlight` works until it is listed.

For local development before a matching release is published, generate the
archive first; makepkg uses the local file when present.

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
Before AUR publication, publish the matching release and test package
installation, update, launcher behavior and removal on clean Arch.

## Release preparation

From a repository checkout, with Python 3.11 or newer,
`cargo build --locked --release` followed by
`python3 integrations/arch/prepare-release.py` assembles a Linux x86_64 binary
bundle, deterministic source archive, PKGBUILD and SHA256SUMS in `build/release`.
Extract the binary bundle and run
`python3 integrations/install-user.py ./chainlight --omarchy` from its directory.

The GitHub release workflow validates and builds on Ubuntu 22.04, uploads the
artifacts, and creates a **draft** release for a version tag matching Cargo.toml.
Drafts require review and publication before their download URL works publicly.
Manual workflow runs only upload review artifacts. CI-built binaries require
glibc 2.35 or newer; local builds inherit their build host requirements.
Generate AUR metadata with `makepkg --printsrcinfo > .SRCINFO` once the archive
checksum is final. Packaging preparation does not submit a package to the AUR.

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
