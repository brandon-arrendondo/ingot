# ingot

Embedded database C code generator with compile-time perfect hashing.

Ingot reads a TOML data model specification and generates optimized C99 code
for key-value storage on resource-constrained embedded systems. All data
structures are statically allocated. Key lookup is O(1) via minimal perfect
hashing -- no dynamic memory, no collisions, no linear search.

## Features

- **All common embedded types**: bool, uint8/int8, uint16/int16, uint32/int32, string
- **Boolean bitfield packing**: 32 booleans per `uint32_t` word
- **Type-separated storage**: right-sized arrays per integer width
- **Perfect hashing**: 2-seed CHM algorithm with Jenkins hash -- O(1) lookup,
  zero collisions, minimal table overhead (~2x key count)
- **Static allocation**: no `malloc`, no heap, no fragmentation
- **Inline accessors**: zero function-call overhead for simple get/set helpers
- **Thread safety**: per-key mutex support (pthread, FreeRTOS, or bare-metal)
- **Event callbacks**: optional value-change notifications
- **tinyfsm dispatch (opt-in)**: `--emit-tinyfsm` generates C++ `tinyfsm::Event`
  structs + a `key_id`→event dispatch wrapper for `event = true` keys
- **Persistence**: packed-struct binary save/load with magic number validation
- **Unity test generation**: auto-generated C tests for every key (defaults, roundtrip, read-only)

## Targets

| Target | Pointer | Mutex | Alignment |
|--------|---------|-------|-----------|
| STM32 (32-bit ARM) | 32-bit | bare-metal | 4-byte |
| ESP32 Xtensa | 32-bit | FreeRTOS | 4-byte |
| ESP32 RISC-V | 32-bit | FreeRTOS | 4-byte |
| 8-bit MCU | 16-bit | bare-metal | 1-byte |
| Linux (64-bit) | 64-bit | pthread | 8-byte |

## Building

```sh
cargo build --release
```

## Usage

```sh
ingot --model path/to/model.toml --output generated/ --target stm32
```

Options:

```
--model <path>     Path to TOML data model specification (required)
--output <dir>     Output directory for generated C code (default: generated/)
--target <target>  Target platform: stm32, esp-xtensa, esp-riscv, mcu8bit, linux64
--no-events        Disable event callback generation
--emit-tinyfsm     Also emit C++/tinyfsm event structs + dispatch-by-key wrapper
                   (opt-in, additive; independent of --no-events)
-v                 Verbose output (-vv for debug, -vvv for trace)
```

## Generated Files

For a model with all feature types enabled, ingot generates:

| File | Purpose |
|------|---------|
| `dm.h` / `dm.c` | Main API: type-dispatch get/set, init/teardown |
| `dm_key.h` | Key bitfield union, type enum, query macros |
| `key_definitions.h` | `#define` per key with encoded 32-bit value |
| `dm_namespace_definitions.h` | Namespace ID constant |
| `boolean_storage.h/.c` | Bitfield-packed boolean storage |
| `integer_storage.h/.c` | Per-type integer storage (one hash per type) |
| `string_storage.h/.c` | Read-only and read-write string storage |
| `persistence_storage.h/.c` | Binary save/load for persistent keys |
| `jenkins_hash.h/.c` | Jenkins lookup3 hash function |
| `dm_helpers.h/.c` | Named getter/setter convenience functions |
| `dm_full.yaml` | Resolved model manifest for downstream tools |
| `test_dm.c` | Unity test suite |
| `CMakeLists.txt` | Build config for tests |

With `--emit-tinyfsm`, ingot additionally emits (for `event = true` keys):

| File | Purpose |
|------|---------|
| `dm_key_events.hpp` | One empty `tinyfsm::Event` struct per event key |
| `dm_key_events_wrapper.hpp/.cpp` | `send_tinyfsm_event_by_key(key_id)` dispatch switch (calls a consumer-provided `send_tinyfsm_event` seam) |

## Data Model Format

Ingot uses a TOML-based schema inspired by [Kaitai Struct](https://kaitai.io/).

```toml
[meta]
id = "my_device"
version = "1.0.0"

[enums.mode]
[enums.mode.values]
off = 0
on = 1
turbo = 2

[[classes]]
id = "config"

    [[classes.keys]]
    id = "brightness"
    type = "uint8"
    default = 100
    persistent = true
    helpers = true

    [[classes.keys]]
    id = "device_name"
    type = "string"
    max_size = 32
    default = "my-device"
```

Key attributes: `type`, `default`, `defaults` (per-variant), `enum`, `max_size`,
`read_only`, `thread_safe`, `persistent`, `event`, `helpers`, `unit`, `doc`.

See `examples/` for complete models: `minimal.toml` (5 keys), `battery.toml`
(18 keys), and `full.toml` (38 keys exercising all features). Pre-generated
C output is in `examples/generated/`.

## Development

```sh
pip install invoke pre-commit
pre-commit install
git submodule update --init deps/unity deps/tinyfsm

invoke check      # Run pre-commit hooks (fmt, clippy, coverage gate)
invoke test       # Rust unit tests + C integration tests (3 models x 2 modes)
invoke coverage   # Rust lcov + C gcov/Cobertura coverage reports
invoke generate   # Generate C code from battery example
invoke clean      # Remove build artifacts
```

## License

MIT
