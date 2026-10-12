#!/usr/bin/env python3
"""Install Chainlight, its icon and desktop entry without root privileges."""
import argparse
import os
from pathlib import Path
import shutil
import tempfile

SIZES = (16, 32, 48, 64, 128, 256)


def desktop_quote(path):
    # Desktop Exec quoting differs from shell quoting (Desktop Entry spec).
    value = str(path).replace('%', '%%')
    for ch in ('\\', '"', '`', '$'):
        value = value.replace(ch, '\\' + ch)
    return '"' + value + '"'


def atomic_install(source, target, mode):
    """Replace the directory entry, never truncate an executable in use."""
    target.parent.mkdir(parents=True, exist_ok=True)
    fd, name = tempfile.mkstemp(prefix='.' + target.name + '-', dir=target.parent)
    temp = Path(name)
    try:
        with os.fdopen(fd, 'wb') as dest:
            if isinstance(source, Path):
                with source.open('rb') as src:
                    shutil.copyfileobj(src, dest)
            else:
                dest.write(source.encode())
        temp.chmod(mode)
        os.replace(temp, target)
    finally:
        temp.unlink(missing_ok=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=Path, help='built chainlight executable')
    parser.add_argument('--omarchy', action='store_true')
    parser.add_argument('--replace', action='store_true', help='replace an existing installation')
    args = parser.parse_args()
    source = args.binary.resolve()
    if not source.is_file() or not os.access(source, os.X_OK):
        parser.error('binary must be an existing executable file')
    root = Path(__file__).resolve().parent
    assets = root.parent / 'assets'
    bindir = Path.home() / '.local/bin'
    datadir = Path(os.environ.get('XDG_DATA_HOME', str(Path.home() / '.local/share')))
    command = bindir / ('chainlight-omarchy' if args.omarchy else 'chainlight')
    layer = 'omarchy' if args.omarchy else 'desktop'
    desktop = (root / layer / 'chainlight.desktop').read_text()
    desktop = desktop.replace('Exec=chainlight-omarchy\n', 'Exec=' + desktop_quote(command) + '\n')
    desktop = desktop.replace('Exec=chainlight\n', 'Exec=' + desktop_quote(command) + '\n')
    files = [(source, bindir / 'chainlight', 0o755),
             (desktop, datadir / 'applications/chainlight.desktop', 0o644),
             (assets / 'chainlight.svg', datadir / 'icons/hicolor/scalable/apps/chainlight.svg', 0o644)]
    if args.omarchy:
        files.append((root / 'omarchy/chainlight-omarchy', command, 0o755))
    for size in SIZES:
        files.append((assets / f'chainlight-{size}.png',
                      datadir / f'icons/hicolor/{size}x{size}/apps/chainlight.png', 0o644))
    for src, dst, mode in files:
        if isinstance(src, Path) and not src.is_file():
            parser.error('missing installation asset: ' + str(src))
        if dst.is_dir():
            parser.error('destination is a directory: ' + str(dst))
    existing = [dst for src, dst, mode in files if dst.exists() or dst.is_symlink()]
    if existing and not args.replace:
        parser.error('existing files: ' + ', '.join(map(str, existing)) + '; use --replace to update')
    for src, dst, mode in files:
        atomic_install(src, dst, mode)
    print('Installed ' + ', '.join(str(dst) for src, dst, mode in files))


if __name__ == '__main__':
    main()
