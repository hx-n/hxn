# Releases and installation

[Design](../design.md) · [Conventions](../conventions.md)

1. Raise the workspace version, the version in `haxelib.json`, and the
   dependency tags in the examples and README.
2. Wait for CI on that commit, then tag `v<version>`. `hxn new` uses that tag.
3. Put library-author release notes in the tag message.

`.github/workflows/release.yml` publishes x86-64 and ARM binaries for Linux
(static musl), macOS and Windows, with `SHA256SUMS`, and submits the `hxn`
haxelib as `darmie`; it fails if `haxelib.json`, the crates and the tag
name different versions. Manual runs build only.

`install.sh` verifies the selected archive before extracting it. Defaults are
the latest release and `~/.local/bin`; override with `HXN_VERSION` and
`HXN_INSTALL_DIR`. `tests/install/run.sh` checks platforms and failure paths
using local archives without network access.

See [CLI usage](../cli.md) and [packaging](../packaging.md).
