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

## API reference

hxn.dev's API reference, `site/api/`, documents each library at the version
`scripts/build_api_docs.py` lists in `LIBRARIES`. After a library's release
reaches haxelib, set its version there, run
`python3 scripts/build_api_docs.py`, and commit `site/`. The script installs
each version into `target/api-haxelib`, describes it with `haxe -xml`, writes
a page per library and per type, and lists the pages in `site/sitemap.xml`
and `site/llms.txt`.

See [CLI usage](../cli.md) and [packaging](../packaging.md).

Redistribution includes `third-party/` with pinned dependency license and
notice texts. Refresh it when Cargo manifests or the lock file change; the
release workflow checks its recorded input hashes before packaging.
