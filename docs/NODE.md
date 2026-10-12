# Bitcoin Core node

The initial mini deployment uses a mainnet Bitcoin Core node with a 20 GiB
pruned-block target. Pruning removes older block files after validation; it
still downloads and validates the full chain during initial synchronization.
The target applies to retained block files, not total node disk usage. The HDD
can make validation slow; the dashboard reports observed progress rather than
promising a completion deadline.

Configuration in `~/.bitcoin/bitcoin.conf`:

```ini
server=1
disablewallet=1
prune=20480
dbcache=256
maxmempool=100
maxconnections=32
rpcbind=127.0.0.1
rpcallowip=127.0.0.1
listen=1
natpmp=0
```

RPC remains on loopback and uses Core's generated cookie. The dashboard uses
read-only RPC requests. The node has no wallet enabled in this first deployment;
receiving Bitcoin or Lightning tips is a later feature.

The user service is named `bitcoin-node.service`. Useful commands:

```sh
systemctl --user status bitcoin-node.service
journalctl --user -u bitcoin-node.service -n 50
~/.local/bin/bitcoin-cli -datadir="$HOME/.bitcoin" getblockchaininfo
systemctl --user stop bitcoin-node.service
systemctl --user start bitcoin-node.service
```

Stopping Core should be graceful so it can flush its databases. Wait for service
shutdown to finish before powering down. A user service runs automatically after
login; boot without login requires user lingering to be enabled by a system
administrator. Inspect `loginctl show-user "$USER" -p Linger` before assuming
unattended boot works.

Verification percentage estimates validation work and differs from block-height
percentage. Early progress, headers discovery, network downloads and database
validation may advance at different rates. During Core's initial anti-spam header
presynchronization, `presynced_headers` can advance while validated `headers` and
`blocks` remain zero; this is normal work, not a stalled download. Headers are
processed again for validation before block download can proceed. Network totals are cumulative since
Core started; a transfer rate requires consecutive samples. Small mempool values
are normal while the node catches up. A pruned node fully validates but cannot
serve all historical block files to other peers.

No inbound router changes are needed to connect to peers and synchronize. Keep
RPC port 8332 private. Inbound peer connectivity on 8333 can be considered after
basic synchronization is healthy.
