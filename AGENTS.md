# AGENTS.md

This document defines how automated agents (and humans operating like them) should work in this repository.

Prism is a lightweight, high-performance reverse proxy with an frp-like tunnel mode. The intended architecture and *implementation-level* guidance live in `DESIGN.md`.

> Implementation note: The current Prism implementation in this repo is **Rust**.

## Non‑negotiables

1. **Design consistency (required)**
   - For any requested change, **verify it matches `DESIGN.md`**.
   - If it does **not** match, **update `DESIGN.md` in the same change** (or immediately before) so design and implementation remain consistent.
   - Do not implement behavior that contradicts the design without also updating the design.

2. **Keep the project test-first**
   - Add/adjust tests for behavior changes.
   - Ensure `cargo test` passes before finishing.
   - If you touch the web UI, also ensure `pnpm test` (and ideally `pnpm check`) passes.

3. **Prefer minimal, reviewable diffs**
   - Make small, incremental changes.
   - Avoid unrelated refactors/renames.
   - Don’t reformat unrelated code; only apply formatting that naturally results from touching code (`cargo fmt`, Biome).

## Repo map (Rust)

- Rust runtime/daemon: `src/prism/*` and `src/main.rs`
- Admin HTTP API: `src/prism/admin.rs`
- Frontend (React/TanStack): `src/routes/*`, `src/components/*`
- Config schema/examples: `prism.schema.json`, `prism.example.toml`, `prism.example.yaml`

If a change affects public behavior (config schema, admin endpoints, tunnel protocol), update `DESIGN.md` and the examples/schema together.

## Quick verification checklist

- `cargo test`
- `cargo fmt` (when Rust code changes)
- `pnpm test` (when frontend changes)
- `pnpm check` (recommended when frontend changes)
