# Website deployment

[CLI usage](cli.md) · [Design](design.md)

Cloudflare Workers Static Assets serves `site/` through `hxn-site` at
`https://hxn.dev/`. `wrangler.jsonc` selects the assets; there is no build
step or Worker script. Missing paths return 404; `workers.dev` is disabled.

With Node.js 24, run from the repository root:

```
npx --yes wrangler@4.161.0 dev
npx --yes wrangler@4.161.0 deploy --dry-run --outdir target/site-deploy
npx --yes wrangler@4.161.0 login
npx --yes wrangler@4.161.0 deploy
```

## GitHub deployment

Connect the repository under the Worker's **Settings → Builds**:

| Setting | Value |
|---|---|
| Worker | `hxn-site` |
| Production branch | `main` |
| Root directory | `/` |
| Build command | Empty. |
| Deploy command | `npx --yes wrangler@4.161.0 deploy` |

Cloudflare supplies Git integration credentials. `.github/workflows/site.yml`
validates the configuration without deploying or needing Cloudflare credentials.

## Social previews

Canonical URLs, social metadata, structured data and sitemap use `hxn.dev`.
`site/og-hero.png` captures the hero at 1200 × 630 CSS pixels, saved at 2×.
Use it for Open Graph, Twitter and structured-data previews.
