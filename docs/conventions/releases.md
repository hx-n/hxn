# Releases and installation

[Design](../design.md) · [Conventions](../conventions.md)

- Tag a release as `v<version>` using the workspace version after CI passes
  on that commit. `hxn new` depends on the tag of the version it was
  built at, so the version is raised before the tag, together with the tag
  the examples' manifests and the readme name.
- Write the release notes in the tag message: a title followed by changes
  relevant to library authors.
- `.github/workflows/release.yml` publishes it. For the tag it builds
  `hxn` for Linux (static, musl), macOS and Windows, on x86-64 and ARM, and
  attaches the archives and their `SHA256SUMS` to the tag's GitHub release.
  Run by hand, it builds the archives without publishing them.
- `install.sh` installs a release binary, selecting one of the workflow's
  target triples and verifying the archive against that release's
  `SHA256SUMS` before extracting it. It defaults to the latest release and
  `~/.local/bin`; `HXN_VERSION` selects a tag and `HXN_INSTALL_DIR` a
  directory. Its platform and failure paths are checked by
  `tests/install/run.sh` with local release archives and no network access.

See also: [CLI builds and packaging](../design/cli.md).
