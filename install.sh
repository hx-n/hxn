#!/usr/bin/env bash
set -euo pipefail

fail() {
    printf 'hxn: %s\n' "$*" >&2
    exit 1
}

main() {
    if [[ "${1:-}" == --help || "${1:-}" == -h ]]; then
        printf '%s\n' 'Install the hxn release binary for this machine.' \
            'Usage: bash install.sh' \
            '  HXN_VERSION       Release tag to install (default: latest).' \
            '  HXN_INSTALL_DIR   Destination (default: $HOME/.local/bin).' \
            'Windows requires Git Bash; otherwise download the release zip.'
        return
    fi
    [[ $# == 0 ]] || fail 'unexpected arguments; use --help for usage'

    local kernel arch target binary extension extractor
    kernel=$(uname -s)
    arch=$(uname -m)
    case "$arch" in
        x86_64|amd64|AMD64) arch=x86_64 ;;
        aarch64|arm64|ARM64) arch=aarch64 ;;
        *) fail "unsupported architecture: $arch; see https://github.com/hx-n/hxn/releases" ;;
    esac
    binary=hxn
    extension=tar.gz
    extractor=tar
    case "$kernel" in
        Darwin) target="$arch-apple-darwin" ;;
        Linux) target="$arch-unknown-linux-musl" ;;
        MINGW*|MSYS*|CYGWIN*)
            target="$arch-pc-windows-msvc"
            binary=hxn.exe
            extension=zip
            extractor=unzip
            ;;
        *) fail "unsupported system: $kernel; see https://github.com/hx-n/hxn/releases" ;;
    esac
    local tool checksum_tool
    for tool in curl "$extractor" mktemp install; do
        command -v "$tool" >/dev/null 2>&1 || fail "required command not found: $tool"
    done
    if command -v sha256sum >/dev/null 2>&1; then
        checksum_tool=sha256sum
    elif command -v shasum >/dev/null 2>&1; then
        checksum_tool=shasum
    else
        fail 'checksum verification requires sha256sum or shasum'
    fi

    local releases version latest directory
    releases=https://github.com/hx-n/hxn/releases
    version=${HXN_VERSION:-}
    if [[ -z "$version" ]]; then
        latest=$(curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 \
            --output /dev/null --write-out '%{url_effective}' "$releases/latest") \
            || fail 'could not find the latest release'
        version=${latest##*/}
    fi
    [[ "$version" =~ ^v[0-9][a-zA-Z0-9._-]*$ ]] || fail "invalid release tag: $version"
    directory=${HXN_INSTALL_DIR:-${HOME:?HOME is not set}/.local/bin}

    local temporary
    temporary=$(mktemp -d "${TMPDIR:-/tmp}/hxn-install.XXXXXXXX")
    # Capture the quoted path while main's locals are still in scope.
    trap "$(printf 'rm -rf -- %q' "$temporary")" EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM

    local name archive url expected actual
    name="hxn-$version-$target"
    archive="$name.$extension"
    url="$releases/download/$version"
    printf 'Downloading hxn %s for %s…\n' "$version" "$target"
    curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 \
        --output "$temporary/$archive" "$url/$archive" || fail "could not download $archive"
    curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 \
        --output "$temporary/SHA256SUMS" "$url/SHA256SUMS" || fail 'could not download SHA256SUMS'
    expected=$(awk -v name="$archive" '$2 == name || $2 == "*" name { print $1 }' "$temporary/SHA256SUMS")
    [[ "$expected" =~ ^[[:xdigit:]]{64}$ ]] || fail "no unique SHA-256 checksum for $archive"
    if [[ "$checksum_tool" == sha256sum ]]; then
        actual=$(sha256sum "$temporary/$archive")
    else
        actual=$(shasum -a 256 "$temporary/$archive")
    fi
    [[ "${actual%% *}" == "$expected" ]] || fail "checksum mismatch for $archive; nothing installed"

    if [[ "$extension" == zip ]]; then
        unzip -q "$temporary/$archive" "$name/$binary" -d "$temporary"
    else
        tar -xzf "$temporary/$archive" -C "$temporary" "$name/$binary"
    fi
    [[ -f "$temporary/$name/$binary" && ! -L "$temporary/$name/$binary" ]] \
        || fail 'the release archive does not contain a regular CLI binary'
    mkdir -p -- "$directory"
    install -m 755 "$temporary/$name/$binary" "$directory/$binary"
    printf 'Installed hxn %s to %s/%s\n' "$version" "$directory" "$binary"
    case ":$PATH:" in
        *":$directory:"*) printf 'Run `hxn version` to check your installation.\n' ;;
        *)
            printf 'Add this directory to your PATH, then run `hxn version`:\n'
            printf '  %s\n' "$directory"
            ;;
    esac
    rm -rf -- "$temporary"
    trap - EXIT INT TERM
}

main "$@"
