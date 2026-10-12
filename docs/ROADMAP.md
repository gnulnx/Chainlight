# Chainlight roadmap

Chainlight currently uses Rust and Ratatui. Keep that implementation as the
working product: the dashboard, themes, launcher and RPC boundaries already
work. The Go prototype remains available on the `go-prototype` branch.

The frontend language does not choose the Bitcoin or Lightning backend. Bitcoin
Core remains a separate service. A future Lightning service should likewise run
independently of the dashboard, so closing or updating the TUI does not interrupt
node operation or payment handling.

## First installable release

- Complete first-run connection/setup for existing Core nodes and new users.
- Explain storage choices and show real setup/sync status.
- Ship the native Omarchy launcher, pixel icon and System theme default.
- Publish versioned source/release artifacts with checksums.
- Turn the existing local PKGBUILD into a published AUR package using public,
  versioned sources, then validate install, update and removal on a clean system.
- Document a short Omarchy installation and launch path.

Core configuration and service management need an explicit setup step; opening
the dashboard must not silently alter an existing node.

## Receiving payments

Start with an on-chain receive screen: a new address, QR code, incoming payment
status and confirmations, backed by an explicitly selected wallet. Choose key
ownership, backup/recovery and wallet permissions before implementing that flow.

Add Lightning through a separate backend adapter: node/channel status, invoices,
expiry, settlement events and receiving capacity. LND and Core Lightning remain
candidates; Rust can communicate with their APIs. BDK and LDK are possible tools
if later requirements justify building a custom wallet or Lightning service.
No backend selection or wallet implementation is implied by this roadmap.

References:

- [LND invoice API](https://api.lightning.community/api/lnd/lightning/add-invoice/index.html)
- [Core Lightning RPC](https://docs.corelightning.org/reference/lightningd-rpc)
- [Bitcoin Dev Kit](https://bitcoindevkit.org/)
- [LDK Node](https://docs.rs/ldk-node/latest/ldk_node/)
