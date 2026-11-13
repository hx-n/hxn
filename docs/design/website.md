# Website deployment

[Design](../design.md) · [Conventions](../conventions.md)

`site/` is the static landing page and Quick Start, served by Cloudflare
Workers Static Assets. `wrangler.jsonc` points at that directory; there is no
build step or Worker script. Missing paths return 404. The Worker named
`hxn-site` serves `hxn.dev` through a Custom Domain route, with its
`workers.dev` address disabled. The page's canonical URL, social preview URLs,
structured data and sitemap all use `https://hxn.dev/`.

`site/og-hero.png` is a browser capture of the hero, framed at 1200 by
630 CSS pixels and saved at twice that resolution. Open Graph, Twitter
cards and the page's structured data use it as the preview image.

With Node.js 24 installed, these commands run from the repository root:

```
npx --yes wrangler@4.161.0 dev
npx --yes wrangler@4.161.0 deploy --dry-run --outdir target/site-deploy
npx --yes wrangler@4.161.0 login
npx --yes wrangler@4.161.0 deploy
```

Cloudflare Workers Builds deploys the site directly from GitHub. Connect
the repository under the Worker's Settings > Builds, using these settings:

| Setting | Value |
|---|---|
| Worker name | `hxn-site` |
| Production branch | `main` |
| Root directory | repository root (`/`) |
| Build command | empty |
| Deploy command | `npx --yes wrangler@4.161.0 deploy` |

The root directory contains `wrangler.jsonc`, which selects `site/` as the
assets directory. Cloudflare manages deployment credentials through its
Git integration. `.github/workflows/site.yml` only validates the static
deployment on pull requests and changes on `main`, or when run by hand;
it needs no Cloudflare credentials and does not publish the site.
