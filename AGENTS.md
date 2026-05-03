# AGENTS.md

This document defines how automated agents (and humans operating like them) should work in this repository.

Prism is a lightweight, high-performance reverse proxy with an frp-like tunnel mode.

> Implementation note: The current Prism implementation in this repo is **Rust**.

## Non‑negotiables

1. **Keep the project test-first**
   - Add/adjust tests for behavior changes.
   - Ensure `cargo test` passes before finishing.
   - If you touch the web UI, also ensure `pnpm test` (and ideally `pnpm check`) passes.

2. **Prefer minimal, reviewable diffs**
   - Make small, incremental changes.
   - Avoid unrelated refactors/renames.
   - Don't reformat unrelated code; only apply formatting that naturally results from touching code (`cargo fmt`, Oxfmt/Oxlint).

3. **Rust dependency hygiene**
   - Before adding a new Rust dependency (new crate in `Cargo.toml`), check whether `cargo upgrade` is available.
     - If it exists, run `cargo upgrade` to see whether a newer compatible version is available and prefer the newest reasonable versions.
     - If it does **not** exist (e.g., `cargo-edit` not installed), **do not install anything**; just skip this step and proceed.

## Repo map (Rust)

- Rust runtime/daemon: `src/prism/*` and `src/main.rs`
- Admin HTTP API: `src/prism/admin.rs`
- Frontend (React/TanStack): `src/routes/*`, `src/components/*`
- Config schema/examples: `prism.schema.json`, `prism.example.toml`, `prism.example.yaml`

If a change affects public behavior (config schema, admin endpoints, tunnel protocol), update the relevant docs, examples, and schema together.

## Quick verification checklist

- `cargo test`
- `cargo fmt` (when Rust code changes)
- `pnpm test` (when frontend changes)
- `pnpm check` (recommended when frontend changes)
