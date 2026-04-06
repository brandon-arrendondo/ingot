# ingot

Embedded database C code generator with compile-time perfect hashing.

Ingot reads a TOML data model specification and generates optimized C code
for key-value storage on resource-constrained embedded systems.  All data
structures are statically allocated.  Key lookup is O(1) via minimal perfect
hashing — no dynamic memory, no collisions, no linear search.

## Targets

| Target | Pointer | Mutex | Alignment |
|--------|---------|-------|-----------|
| STM32 (32-bit ARM) | 32-bit | bare-metal | 4-byte |
| ESP32 Xtensa | 32-bit | FreeRTOS | 4-byte |
| ESP32 RISC-V | 32-bit | FreeRTOS | 4-byte |
| 8-bit MCU | 16-bit | bare-metal | 1-byte |
| Linux (64-bit) | 64-bit | pthread | 8-byte |

## Optimizations

- **Boolean bitfield packing**: 32 booleans per `uint32_t` word
- **Type-separated storage**: right-sized arrays per integer width
- **Perfect hashing**: 2-seed CHD algorithm with Jenkins hash — O(1) lookup,
  zero collisions, minimal table overhead (~1.2x key count)
- **Static allocation**: no `malloc`, no heap, no fragmentation
- **Inline accessors**: zero function-call overhead for simple get/set

## Building

```sh
cargo build --release
```

## Usage

```sh
ingot --model path/to/model.toml --output generated/ --target stm32
```

Run `ingot --help` for full CLI documentation.

## Testing

```sh
# Rust unit tests
cargo test

# Generated C code tests (requires Unity framework in deps/unity)
# See tests/ directory after code generation
```

## Data Model Format

Ingot uses a TOML-based data model specification inspired by
[Kaitai Struct](https://kaitai.io/).  See `examples/` for sample models.

## License

MIT
