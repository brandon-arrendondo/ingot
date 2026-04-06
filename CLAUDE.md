# Ingot — Claude Code Project Guide

## Build & Test Commands

```sh
invoke check             # Run pre-commit hooks on all files
invoke build             # Debug build
invoke build --release   # Release build (LTO + strip)
invoke test              # Run Rust unit tests + C integration tests (both modes)
invoke coverage          # Rust lcov + C Cobertura XML coverage reports
invoke clean             # Remove build artifacts and generated output
invoke generate          # Generate C code from battery example
invoke generate --model examples/minimal.toml  # Generate from specific model
```

Alternatively, use cargo directly:

```sh
cargo build              # Debug build
cargo build --release    # Release build (LTO + strip)
cargo test               # Run all unit tests
cargo fmt --all --check  # Check formatting
cargo clippy --all-targets -- -D warnings  # Lint
```

## Project Structure

```
src/
  main.rs              - CLI entry point (clap)
  model/               - Data model parsing and internal representation
    schema.rs          - serde types for TOML input format
    key.rs             - 32-bit key encoding/decoding
    validation.rs      - Model validation passes
  codegen/             - C code generation
    target.rs          - Target platform configs (STM32, ESP, 8-bit, Linux)
    storage/           - Per-type storage generators (bool, int, string, binary)
  hash/                - Perfect hash implementation
    jenkins.rs         - Jenkins lookup3.c final() hash
    mod.rs             - CHM perfect hash generation

templates/             - Tera templates for C output
examples/              - Example TOML data models
tests/                 - Integration tests
deps/unity/            - Unity C test framework (git submodule)
docs/                  - User guide and manpage
```

## Conventions

- All public functions need unit tests
- Use `thiserror` for error types
- Use `log` macros (not println) for diagnostic output
- Generated C code targets C99
- Pre-commit hooks enforce: cargo fmt, cargo clippy (deny warnings), cargo test
- Commit messages follow conventional format
- CHANGELOG.txt uses keepachangelog format
- PLAN.md tracks roadmap tasks

## Key Architecture Decisions

- **Input format**: Kaitai-inspired TOML
- **Templating**: Tera (Jinja2-like, runtime templates)
- **Perfect hash**: Pure Rust implementation (CHD algorithm, 2-seed Jenkins)
- **Target abstraction**: TargetConfig struct with per-platform settings
- **No dynamic allocation in generated C**: all storage is static arrays
