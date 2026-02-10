---
title: Routing parsers (WASM)
sidebar_position: 2
---

Prism extracts the routing hostname from the first bytes of each TCP connection using **routing parsers**.

A routing parser is a `.wat` file (WebAssembly text format). Prism loads these modules from a directory on disk and executes them to extract the hostname.

## Built-in parsers

Prism ships two built-in parsers:

- `minecraft_handshake`
- `tls_sni`

At startup, Prism **materializes** (writes) the built-in `.wat` modules into the routing parser directory *if they are missing*.

## Where parsers live

The routing parser directory is configured via:

- CLI flag: `--routing-parser-dir /path/to/parsers`
- Environment variable: `PRISM_ROUTING_PARSER_DIR=/path/to/parsers`
- Default: `<config_dir>/parsers` (Linux default: `/etc/prism/parsers`)

Each parser name maps to a file:

- `parsers = ["minecraft_handshake"]` → `<routing_parser_dir>/minecraft_handshake.wat`

## Relative paths

If `--routing-parser-dir` is relative (like `parsers`), Prism resolves it relative to the **config directory**.

## Security & format notes

- Prism intentionally **does not load raw `.wasm` binaries** for routing parsers.
- Routing parsers execute on untrusted client input (the first bytes of a TCP stream). Keep them small and defensive.
- If you add custom parsers, prefer reviewing them like you would any code that processes network bytes.

## Troubleshooting

### My parser isn’t being found

- Confirm the parser name in `routes[*].parsers` matches the filename **without** `.wat`.
- Confirm the file exists in the resolved routing parser directory.
- Check Prism logs at `logging.level = "debug"`.
