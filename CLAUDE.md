# Ingot — Claude Code Project Guide

@AGENTS.md

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
  codegen/             - C and Rust code generation
    target.rs          - C target platform configs (STM32, ESP, 8-bit, Linux)
    storage/           - C per-type storage generators (bool, int, string)
    rust_kvp/          - no_std Rust module generator
      storage.rs       - Rust static storage and encoded-key access
      enums.rs         - Rust enum code generation
  hash/                - Perfect hash implementation
    jenkins.rs         - Jenkins lookup3.c final() hash
    mod.rs             - CHM perfect hash generation

templates/             - Tera templates for generated Rust module and C output
examples/              - Example TOML data models
tests/                 - Integration tests
deps/unity/            - Unity C test framework (git submodule)
docs/                  - User guide and manpage
```

## Conventions

- All public functions need unit tests
- Use `thiserror` for error types
- Use `log` macros (not println) for diagnostic output
- Generated C code targets C99; the Rust target emits a no_std module
- `invoke check` runs formatting, Clippy with warnings denied, the coverage gate, and the knots complexity hook
- Commit messages follow conventional format
- CHANGELOG.md uses keepachangelog format
- The repository keeps no task tracker; branch, commit and open a PR as normal

## Key Architecture Decisions

- **Input format**: Kaitai-inspired TOML
- **Templating**: Tera (Jinja2-like, runtime templates)
- **Perfect hash**: Pure Rust implementation (CHD algorithm, 2-seed Jenkins)
- **Target abstraction**: TargetConfig struct with per-platform settings
- **No dynamic allocation in generated C**: all storage is static arrays
