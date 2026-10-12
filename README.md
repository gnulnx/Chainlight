# Chainlight — Go prototype

Preserved Go / Bubble Tea iteration of the Bitcoin Core dashboard. The Rust
iteration lives on `main`; this branch remains available for comparison and
continued Go development. Bitcoin Core runs separately and is not restarted
when either dashboard is changed.

This source was preserved just before the Rust port, after applying the
Chainlight name. It includes sync/header pre-sync, peers, storage, network
rates, conservative ETA, connection-loss handling and theme integration.

## Build and install

Requires Go 1.24 or newer:

```sh
go build -trimpath -buildvcs=false -o build/chainlight .
go test -race ./...
go vet ./...
```

For a static Linux executable, prefix the build command with `CGO_ENABLED=0`.

```sh
python3 integrations/install-user.py build/chainlight --omarchy
~/.local/bin/chainlight-omarchy
```

Use `--replace` only when intentionally replacing an existing Chainlight
installation. This prototype and the Rust version share the same command name
and launcher; use separate binary paths for side-by-side comparisons.

For direct terminal/SSH usage, run `build/chainlight`. Defaults are mainnet RPC
at `http://127.0.0.1:8332` and the cookie in `~/.bitcoin`. Flags include
`--datadir`, `--rpc-url`, `--theme-file`, `--snapshot`, and `--render`.

The installer places the application launcher and custom icon under user-local
XDG paths without editing Omarchy defaults. Press Super+Alt+Space on Omarchy
4.0.4 and select Chainlight. Tab/arrows select pages; ? opens help; q quits the
TUI while Core keeps running.

[Node configuration](docs/NODE.md). MIT licensed, see LICENSE.
