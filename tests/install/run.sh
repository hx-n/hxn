#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

python3 - <<'PY'
import hashlib
import io
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import zipfile

installer = Path('install.sh').resolve()
payload = b'#!/bin/sh\nprintf "fixture hxn\\n"\n'
platforms = [
    ('Darwin', 'x86_64', 'x86_64-apple-darwin', 'hxn'),
    ('Darwin', 'arm64', 'aarch64-apple-darwin', 'hxn'),
    ('Linux', 'x86_64', 'x86_64-unknown-linux-musl', 'hxn'),
    ('Linux', 'aarch64', 'aarch64-unknown-linux-musl', 'hxn'),
    ('MINGW64_NT-10.0', 'x86_64', 'x86_64-pc-windows-msvc', 'hxn.exe'),
    ('MSYS_NT-10.0', 'aarch64', 'aarch64-pc-windows-msvc', 'hxn.exe'),
]

with tempfile.TemporaryDirectory(prefix='hxn-installer-test-') as temporary:
    root = Path(temporary)
    tools = root / 'tools'
    tools.mkdir()
    fixtures = root / 'releases'
    fixtures.mkdir()
    downloads = root / 'downloads.log'
    scratch = root / 'scratch'
    scratch.mkdir()
    (tools / 'uname').write_text('''#!/bin/bash
case "$1" in
    -s) printf '%s\\n' "$TEST_KERNEL" ;;
    -m) printf '%s\\n' "$TEST_ARCH" ;;
    *) exit 1 ;;
esac
''')
    (tools / 'curl').write_text('''#!/bin/bash
set -eu
output=
while [[ $# -gt 0 ]]; do
    case "$1" in
        --output) output=$2; shift 2 ;;
        --write-out|--proto) shift 2 ;;
        --*) shift ;;
        *) url=$1; shift ;;
    esac
done
printf '%s\\n' "$url" >> "$TEST_DOWNLOADS"
if [[ "$url" == https://github.com/hx-n/hxn/releases/latest ]]; then
    printf 'https://github.com/hx-n/hxn/releases/tag/v0.2.0'
else
    path=${url#https://github.com/hx-n/hxn/releases/download/}
    [[ "$path" != "$url" ]] || exit 1
    cp "$TEST_FIXTURES/$path" "$output"
fi
''')
    for tool in tools.iterdir():
        tool.chmod(0o755)

    def release(version, checksum_error=False, missing_binary=False):
        directory = fixtures / version
        directory.mkdir(exist_ok=True)
        sums = []
        for _, _, target, binary in platforms:
            name = f'hxn-{version}-{target}'
            member = f'{name}/{"missing" if missing_binary else binary}'
            if binary.endswith('.exe'):
                archive = directory / f'{name}.zip'
                with zipfile.ZipFile(archive, 'w') as output:
                    output.writestr(member, payload)
            else:
                archive = directory / f'{name}.tar.gz'
                with tarfile.open(archive, 'w:gz') as output:
                    entry = tarfile.TarInfo(member)
                    entry.size = len(payload)
                    entry.mode = 0o755
                    output.addfile(entry, io.BytesIO(payload))
            digest = '0' * 64 if checksum_error else hashlib.sha256(archive.read_bytes()).hexdigest()
            sums.append(f'{digest}  {archive.name}\n')
        (directory / 'SHA256SUMS').write_text(''.join(sums))

    def run(kernel='Linux', arch='x86_64', version=None, directory=None, arguments=()):
        env = os.environ.copy()
        env.pop('HXN_VERSION', None)
        env.pop('HXN_INSTALL_DIR', None)
        env.update(PATH=f'{tools}{os.pathsep}{env["PATH"]}', HOME=str(root / 'home'),
                   TMPDIR=str(scratch), TEST_KERNEL=kernel, TEST_ARCH=arch,
                   TEST_FIXTURES=str(fixtures), TEST_DOWNLOADS=str(downloads))
        if version is not None:
            env['HXN_VERSION'] = version
        if directory is not None:
            env['HXN_INSTALL_DIR'] = str(directory)
        result = subprocess.run(['bash', str(installer), *arguments], env=env, cwd=root,
                                text=True, capture_output=True, timeout=10)
        assert not list(scratch.iterdir()), result.stderr
        return result

    release('v0.2.0')
    for i, (kernel, arch, target, binary) in enumerate(platforms):
        directory = root / f'install with spaces {i}'
        result = run(kernel, arch, directory=directory)
        assert result.returncode == 0, result.stderr
        assert (directory / binary).read_bytes() == payload, result.stdout
        assert (directory / binary).stat().st_mode & 0o111
        assert target in result.stdout
    result = run()
    assert result.returncode == 0, result.stderr
    assert (root / 'home/.local/bin/hxn').read_bytes() == payload

    release('v0.1.0')
    downloads.write_text('')
    directory = root / 'pinned'
    result = run(version='v0.1.0', directory=directory)
    assert result.returncode == 0, result.stderr
    assert '/latest' not in downloads.read_text()
    assert '/download/v0.1.0/' in downloads.read_text()

    release('v9.0.0', checksum_error=True)
    result = run(version='v9.0.0', directory=directory)
    assert result.returncode != 0 and 'checksum mismatch' in result.stderr
    assert (directory / 'hxn').read_bytes() == payload

    for version, checksums in [('v9.0.1', ''), ('v9.0.2', 'duplicate')]:
        release(version)
        sums = fixtures / version / 'SHA256SUMS'
        sums.write_text(sums.read_text() * 2 if checksums else '')
        result = run(version=version, directory=directory)
        assert result.returncode != 0 and 'no unique SHA-256' in result.stderr
        assert (directory / 'hxn').read_bytes() == payload

    release('v9.0.3', missing_binary=True)
    result = run(version='v9.0.3', directory=directory)
    assert result.returncode != 0
    assert (directory / 'hxn').read_bytes() == payload
    for options in [dict(kernel='FreeBSD'), dict(arch='riscv64'), dict(version='../../bad')]:
        result = run(**options)
        assert result.returncode != 0, result.stdout
    result = run(version='v99.0.0')
    assert result.returncode != 0 and 'could not download' in result.stderr
    assert run(arguments=['--help']).returncode == 0
    assert run(arguments=['unexpected']).returncode != 0

print('PASS: six release targets, latest and pinned versions, default and spaced paths,')
print('      checksum failures, missing assets, unsupported platforms, and temporary cleanup.')
PY
