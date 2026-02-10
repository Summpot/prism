# prism-docs

This is the Prism documentation site, built with [Docusaurus](https://docusaurus.io/).

## Requirements

- Node.js >= 20
- pnpm (recommended via Corepack)

## Install

```text
corepack enable
corepack prepare pnpm@latest --activate
pnpm install --frozen-lockfile
```

## Local development

```text
pnpm start
```

## Build

```text
pnpm build
```

This produces a static site in `build/`.

## i18n

The default language is English.

- English docs: `docs/`
- Simplified Chinese docs: `i18n/zh-CN/docusaurus-plugin-content-docs/current/`

## Deployment (Cloudflare Pages)

This repo includes a GitHub Actions workflow that builds `prism-docs` and deploys it to Cloudflare Pages.

You need to configure these repository secrets:

- `CLOUDFLARE_API_TOKEN`
- `CLOUDFLARE_ACCOUNT_ID`

And you must create a Cloudflare Pages project (default name in the workflow: `prism-docs`).
