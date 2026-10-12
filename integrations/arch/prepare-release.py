#!/usr/bin/env python3
"""Assemble a Linux x86_64 binary bundle, source, AUR recipe and checksums."""
import hashlib
from pathlib import Path
import platform
import shutil
import subprocess
import tarfile
import tomllib

root = Path(__file__).resolve().parents[2]
version = tomllib.loads((root / 'Cargo.toml').read_text())['package']['version']
if platform.system() != 'Linux' or platform.machine() != 'x86_64':
    raise SystemExit('This binary bundle targets Linux x86_64; build from source elsewhere.')
binary = root / 'target/release/chainlight'
if subprocess.check_output([str(binary), '--version'], text=True).strip() != f'chainlight {version}':
    raise SystemExit('Build the current release binary before packaging.')
subprocess.run(['python3', str(root / 'integrations/arch/prepare-source.py')], check=True)
output = root / 'build/release'
output.mkdir(parents=True, exist_ok=True)
source = root / f'integrations/arch/chainlight-{version}.tar.gz'
shutil.copy2(source, output / source.name)
shutil.copy2(root / 'integrations/arch/PKGBUILD', output / 'PKGBUILD')
bundle_name = f'chainlight-{version}-linux-x86_64'
bundle = output / (bundle_name + '.tar.gz')
with tarfile.open(source) as src, tarfile.open(bundle, 'w:gz') as dst:
    for info in src.getmembers():
        info.name = info.name.replace(f'chainlight-{version}/', bundle_name + '/', 1)
        dst.addfile(info, src.extractfile(info) if info.isfile() else None)
    dst.add(binary, arcname=bundle_name + '/chainlight')
artifacts = [output / source.name, bundle, output / 'PKGBUILD']
(output / 'SHA256SUMS').write_text(''.join(
    hashlib.sha256(path.read_bytes()).hexdigest() + '  ' + path.name + '\n'
    for path in artifacts))
print('Prepared release artifacts in ' + str(output))
