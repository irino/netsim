# AGENTS.md

## Project purpose

Build a Rust-based multi-vendor network configuration translator and network emulation generator.

Read `docs/design-memo-v0.2.md` before implementing any feature. Treat it as the authoritative design specification.

## Architecture

- Parse vendor configurations into a vendor-independent Rust IR.
- Keep the IR independent of vendor parsers and output backends.
- Align IR naming and JSON structure with relevant IETF YANG models.
- Prefer `winnow` for line-oriented parsing.
- Use Junos `set` configuration as the initial Junos input format.
- Generate zebra-rs configuration and Containerlab topology in later phases.
- Prefer zebra-rs as the runtime engine, but verify its actual configuration schema and supported features before generating runtime configuration.

## Development rules

- Implement one small, reviewable task at a time.
- Add unit tests and golden tests for every parser or IR change.
- Run `cargo fmt --check` and `cargo test --workspace` when applicable.
- Never claim a test passed unless it was actually executed successfully.
- Do not silently discard unsupported configuration lines.
- Report unsupported lines with filename, line number, original content, and reason.
- Distinguish unsupported input syntax from features unsupported by the runtime engine.
- Do not invent zebra-rs YAML keys or assume unverified protocol support.
- Update documentation when the design or supported syntax changes.
- Do not introduce unrelated dependencies or features without explaining why.
