#!/usr/bin/env python3
"""Create a deterministic local source archive and pin its PKGBUILD checksum."""
import gzip
import hashlib
from pathlib import Path
import re
import tarfile

root = Path(__file__).resolve().parents[2]
packaging = root / 'integrations/arch'
pkgbuild = packaging / 'PKGBUILD'
recipe = pkgbuild.read_text()
version = re.search(r'^pkgver=([0-9.]+)$', recipe, re.MULTILINE).group(1)
name = 'chainlight-' + version
required = [root / n for n in ('Cargo.toml', 'Cargo.lock', 'LICENSE', 'README.md', 'assets/chainlight.svg')]
required.extend(root / f'assets/chainlight-{size}.png' for size in (16, 32, 48, 64, 128, 256))
for path in required:
    if not path.is_file():
        raise SystemExit('Missing required source file: ' + str(path))
files = list((root / 'src').rglob('*.rs')) + required
for folder in ('docs', 'assets', 'integrations/desktop', 'integrations/omarchy'):
    files.extend(p for p in (root / folder).rglob('*')
                 if p.is_file() and p != root / 'docs/VALIDATION.md')
files.append(root / 'integrations/install-user.py')
archive = packaging / (name + '.tar.gz')
with archive.open('wb') as output:
    with gzip.GzipFile(filename='', mode='wb', fileobj=output, mtime=0) as compressed:
        with tarfile.open(fileobj=compressed, mode='w') as tar:
            for path in sorted(set(files)):
                info = tar.gettarinfo(str(path), arcname=name + '/' + str(path.relative_to(root)))
                info.mtime = info.uid = info.gid = 0
                info.uname = info.gname = ''
                with path.open('rb') as source:
                    tar.addfile(info, source)
checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
recipe = re.sub(r'^sha256sums=.*$', "sha256sums=('" + checksum + "')", recipe, flags=re.MULTILINE)
pkgbuild.write_text(recipe)
print(str(archive) + '\nSHA-256: ' + checksum)
