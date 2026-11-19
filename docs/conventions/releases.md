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

`install.sh` and `install.ps1` verify the selected archive before extracting
the CLI. hxn.dev serves identical copies from `site/`; update both copies
when changing an installer. `tests/site/check.py` rejects differing copies.
Serve them as plain text with a five-minute cache lifetime.

Both installers default to the latest release. Override the tag with
`HXN_VERSION` or the destination with `HXN_INSTALL_DIR`. Unix uses
`~/.local/bin`; PowerShell uses `%LOCALAPPDATA%\hxn\bin` and adds it to the
current and persistent user `PATH`. Neither installer requires administrator
access for its default directory.

`tests/install/run.sh` and `tests/install/run.ps1` exercise platforms,
checksum failures and archive errors with local fixtures. CI runs Bash on
Linux and macOS, and Windows PowerShell and PowerShell 7 on Windows.

## Agent skills

Documentation links in skills use absolute HTTPS URLs, so they work from
installed packages and the discovery site as well as source checkouts.

Each hxn library keeps its agent skill in `skills/<name>/SKILL.md`, and its
release puts `skills/` in the haxelib. hxn.dev serves copies of all of them
from `site/.well-known/agent-skills/`, with the discovery index beside them,
and lists them in `site/llms.txt`. After a library's skill changes on main,
run `python3 scripts/sync_site_skills.py --github` (without `--github` it
reads checkouts beside this one) and commit `site/`. `tests/site/check.py`
fails when a copy's digest, name or description disagrees with the index, or
when `site/`'s hxn skill differs from `skills/hxn/SKILL.md`.

Core logging has its own `skills/hxn-log/SKILL.md` discovery entry while its
API types remain under the hxn core reference. hxn-file and hxn-assets share
the hxn-assets repository; register each released haxelib version and its
package source URL in `scripts/build_api_docs.py`. Sync both monorepo skills
and regenerate both package references after publication. Source publication
alone does not make a package a released haxelib reference.
Register additional core capability skills in
`scripts/sync_site_skills.py` and `site/llms.txt` so they are discoverable.

## API reference

hxn.dev's API reference, `site/api/`, documents each library at the version
`scripts/build_api_docs.py` lists in `LIBRARIES`. After a library's release
reaches haxelib, set its version there, run
`python3 scripts/build_api_docs.py`, and commit `site/`. The script installs
each version into `target/api-haxelib`, describes it with `haxe -xml`, writes
a page per library and per type, and lists the pages in `site/sitemap.xml`
and `site/llms.txt`. Include optional haxelibs in a library's `libs` entry
when they enable public integration types, and document that requirement.

See [CLI usage](../cli.md) and [packaging](../packaging.md).

Redistribution includes `third-party/` with pinned dependency license and
notice texts. Refresh the CLI inventory with
`python3 scripts/third_party_notices.py --package hxn-cli` when Cargo
manifests or the lock file change; the release workflow checks its recorded input hashes before packaging. Preserve
upstream files byte-for-byte with the attributes in `.gitattributes`. An
undeclared upstream license is recorded as `NOASSERTION` and prevents release
until its terms are resolved; retaining a manifest does not grant a license.
