Chainlight 0.3.0 is an early Bitcoin Core dashboard release.

- Full-screen pixel dashboard with verification ETA and estimated completion time.
- Color-coded bandwidth history, graph modes and observed node activity.
- Omarchy System theme plus five experimental palettes.
- Native launcher and pixel chain/lightning icon.
- First-run connection check and saved local node settings (`--setup` to revisit).

Bitcoin Core must be installed and started separately. Setup provides guidance;
it does not install Core or change its configuration. Wallets and Lightning are
planned and are not implemented in this version.

The CI-built executable targets Linux x86_64 and requires glibc 2.35 or newer.
Other architectures can build from source with Rust 1.88 or newer.
Verify downloaded archives using SHA256SUMS, then follow docs/INSTALL.md.

The source package was built in a clean Arch Linux container and checked through
pacman installation, update and removal. Interactive setup and RPC reads were
tested against a local test server; live dashboard and Omarchy launcher behavior
were separately verified on an Omarchy mini. Container checks do not qualify
other architectures or every desktop environment.
