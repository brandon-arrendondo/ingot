# Ingot — Plans & Roadmap

Last Updated: 2026-04-06 (v0.1.0)

Ingot is a Rust-based C code generator for embedded key-value databases.
It produces optimized, statically-allocated C code with compile-time perfect
hashing for O(1) key lookup.  Targets: STM32, ESP32, 8-bit MCUs, 64-bit Linux.

Predecessor: d_data_model_generator (Python+Jinja, ~3000-line gen_udm.py).
Data model reference: d_bissell_unified_data_model (27 namespaces, ~1500 keys).

For completed work, see CHANGELOG.txt.

Default test strategy for all tasks: pre-commit hooks (cargo test + cargo fmt +
cargo clippy), then Unity-based integration tests on generated C output.

---

# Phase 1: Foundation — Schema, Parser, Key Encoding

# Task ID: 1
# Title: Design TOML data model schema (Kaitai-inspired)
# Status: done
# Dependencies: none
# Priority: P1
# Description: Define the TOML input format for data model specifications.
# Details:
DONE. Kaitai-inspired TOML schema designed and implemented:
  - [meta] section: id, version, doc
  - [enums.<name>] with values + per-variant overrides
  - [[classes]] with [[classes.keys]] for namespace/class/key hierarchy
  - [[instances]] for derived/computed values with expression strings
  - Key attributes: type, default, defaults (per-variant), enum, max_size,
    read_only, thread_safe, persistent, event, helpers, unit, doc
  - 9 data types: bool, uint8, int8, uint16, int16, uint32, int32, string, binary

Files: examples/minimal.toml, examples/battery.toml (port of battery_001.yaml)
Schema types: src/model/schema.rs (serde Deserialize)
Validation: src/model/validation.rs (6 checks, 5 error-path tests)
Tests: 19 total passing (parse + validation + type codes)

---

# Task ID: 2
# Title: Implement TOML parser with serde
# Status: done
# Dependencies: 1
# Priority: P1
# Description: Parse and validate TOML data model files into internal IR.
# Details:
DONE. Implemented as part of task 1:
  - serde Deserialize types in src/model/schema.rs
  - Validation in src/model/validation.rs:
    * Class count overflow (max 31)
    * Key count overflow (max 1023)
    * Duplicate class IDs
    * Duplicate key IDs within class
    * String/binary keys require max_size
    * Enum references must exist in [enums]
    * Instance expressions must be non-empty
  - CLI wired to parse + validate + report summary

---

# Task ID: 3
# Title: Implement 32-bit key encoding
# Status: done
# Dependencies: none
# Priority: P1
# Description: Encode/decode 32-bit keys from namespace/class/id/type/flags.
# Details:
DONE. Implemented in src/model/key.rs with encode/decode roundtrip tests.
Bit layout: ns(10) | class(5) | id(10) | type(4) | thread_safe(1) | derived(1) | read_only(1).

---

# Phase 2: Perfect Hash + Integer Storage Codegen

# Task ID: 4
# Title: Port perfect hash (CHM) algorithm to Rust
# Status: done
# Dependencies: none
# Priority: P1
# Description: Implement the CHM perfect hash generation algorithm in pure Rust.
# Details:
DONE. Ported from util_perfect_hash_integers Python reference.
Algorithm: CHM (Czech-Havas-Majewski) with 2-seed Jenkins lookup3.c final() hash.
  - generate(keys, max_iters) -> Option<PerfectHash { seed1, seed2, g_table }>
  - generate_with_seeds() for deterministic testing
  - G table size: ceil(2.09 * num_keys), signed i32 values
  - Verified: 500-key sets, deduplication, single-key edge case
  - 8 tests in src/hash/mod.rs

---

# Task ID: 5
# Title: Jenkins hash — cross-language verification
# Status: done
# Dependencies: 4
# Priority: P1
# Description: Verify Rust jenkins_hash matches C implementation exactly.
# Details:
DONE. 10 test vectors generated from Python reference, all match exactly.
Jenkins lookup3.c final() mixing function (NOT one-at-a-time).
Magic constant: 0x9e3779b9. Rotations: 14,11,25,16,4,14,24.
4 tests in src/hash/jenkins.rs.

---

# Task ID: 6
# Title: Integer storage codegen (uint8, uint16, uint32, int8, int16, int32)
# Status: done
# Dependencies: 4
# Priority: P1
# Description: Generate C code for integer storage with perfect hash lookup.
# Details:
DONE. Full pipeline: TOML → parse → validate → perfect hash → Tera → C code.
  - Tera templates: integer_storage.{h,c}, jenkins_hash.{h,c}, key_definitions.h
  - Per-type storage: separate hash seeds, G table, storage array, get/set functions
  - G table type auto-selected: int8_t/int16_t/int32_t based on value range
  - Default values placed in hash order in init arrays
  - Generated C compiles clean: gcc -Wall -Wextra -Wpedantic -std=c99
  - Tested with both examples (battery: 3 type groups, minimal: 3 type groups)

---

# Phase 3: Boolean + String Storage

# Task ID: 7
# Title: Boolean bitfield storage codegen
# Status: pending
# Dependencies: 4
# Priority: P2
# Description: Generate C code for boolean storage using bitfield packing.
# Details:
Pack booleans into uint32_t arrays (32 per word).
Generate SetBit/ClearBit/TestBit macros.
Perfect hash maps key to (word_index, bit_index).
Thread-safe variant uses mutex around word read-modify-write.

---

# Task ID: 8
# Title: String storage codegen (RO + RW)
# Status: pending
# Dependencies: 4
# Priority: P2
# Description: Generate C code for string storage with separate RO/RW paths.
# Details:
Read-only: static array of const char* pointers.
Read-write: struct with embedded fixed-size char arrays (max_size per key).
Perfect hash lookup for both.  Thread-safe RW uses mutex.

---

# Phase 4: API Layer + Helpers

# Task ID: 9
# Title: Main data model API codegen
# Status: pending
# Dependencies: 6, 7, 8
# Priority: P2
# Description: Generate dm.h/dm.c with type-dispatch get/set functions.
# Details:
Generate the top-level API:
  - DataModel_SetIntegralTypeByKey / DataModel_GetIntegralTypeByKey
  - DataModel_SetStringByKey / DataModel_GetStringByKey
  - Key query macros (IS_KEY_IN_NAMESPACE, IS_KEY_DATA_TYPE, etc.)
  - DM_RETURN_CODE enum
Type dispatch via key bits → route to per-type storage module.

---

# Task ID: 10
# Title: Key definitions + namespace definitions codegen
# Status: pending
# Dependencies: 2
# Priority: P2
# Description: Generate dm_key_definitions.h and dm_namespace_definitions.h.
# Details:
One #define per key with 32-bit encoded value and descriptive comment.
Namespace enum/defines mapping name to ID.

---

# Task ID: 11
# Title: Auto-generated helper getter/setter codegen
# Status: pending
# Dependencies: 9
# Priority: P2
# Description: Generate inline helpers for keys with generate_helpers=true.
# Details:
For each key with generate_helpers:
  DataModel_Get{Namespace}_{Class}_{Element}()
  DataModel_Set{Namespace}_{Class}_{Element}(value)
Inline functions in dm_helpers.h, implementations in dm_helpers.c.

---

# Phase 5: Events, Derived Keys, Persistence

# Task ID: 12
# Title: Event system codegen
# Status: pending
# Dependencies: 9
# Priority: P3
# Description: Generate key change event callbacks and dispatcher.
# Details:
For keys with event=true, generate registration and dispatch infrastructure.
Key-to-event mapping table.  FSM event enum generation from enum_sets.

---

# Task ID: 13
# Title: Derived key codegen
# Status: pending
# Dependencies: 9
# Priority: P3
# Description: Generate computed read-only values from operation expressions.
# Details:
Parse operation expressions (+, -, *, /, comparisons, ternary).
Generate static evaluation functions.
Derived event mapping: when operand changes, trigger derived key events.

---

# Task ID: 14
# Title: Persistence storage codegen
# Status: pending
# Dependencies: 9
# Priority: P3
# Description: Generate serialization/deserialization for persistent keys.
# Details:
Contiguous memory block layout.  Load/save to binary with magic number.
Magic number derived from model content hash.

---

# Phase 6: Multi-Target, Packaging, Documentation

# Task ID: 15
# Title: Target abstraction implementation
# Status: pending
# Dependencies: 6
# Priority: P2
# Description: Implement target-specific code generation variations.
# Details:
Target configs: STM32 (bare-metal, no mutex, 4-byte align),
ESP32-Xtensa/RISC-V (FreeRTOS mutex, 4-byte align),
8-bit MCU (no mutex, 1-byte align, 16-bit pointers),
Linux64 (pthread mutex, 8-byte align).
Affects: mutex includes/calls, alignment pragmas, inline size limits.

---

# Task ID: 16
# Title: Unity integration test framework
# Status: pending
# Dependencies: 6, 7, 8, 9
# Priority: P2
# Description: Generate Unity test files for generated data models.
# Details:
Submodule ThrowTheSwitch/Unity into deps/unity.
Generate test_dm_*.c files that verify:
  - Set/get roundtrip for every data type
  - Perfect hash collision-free for all keys
  - Default value initialization
  - Thread-safety (where applicable)
  - Persistence load/save roundtrip
CMakeLists.txt for building and running Unity tests.

---

# Task ID: 17
# Title: Example data models
# Status: pending
# Dependencies: 1, 9
# Priority: P3
# Description: Create example TOML models with generated output.
# Details:
  - minimal.toml: 3 keys, 2 types — quickstart example
  - battery.toml: port of battery namespace from Bissell UDM
  - full.toml: comprehensive example exercising all features
Include pre-generated C output for each in examples/generated/.

---

# Task ID: 18
# Title: User guide and manpage
# Status: pending
# Dependencies: 17
# Priority: P3
# Description: Write docs/user-guide.md and docs/ingot.1 manpage.
# Details:
User guide: installation, quickstart, schema reference, target config,
generated API reference, integration guide for build systems.
Manpage: standard man format covering CLI options and exit codes.

---

# Task ID: 19
# Title: Binary distribution packages
# Status: pending
# Dependencies: none
# Priority: P4
# Description: GitHub Actions for AppImage, deb, rpm, Windows exe, crate publish.
# Details:
Packaging targets:
  - crates.io: cargo publish workflow on tag
  - AppImage: cargo-appimage or linuxdeploy
  - .deb: cargo-deb (binary in /usr/bin/, manpage in /usr/share/man/man1/)
  - .rpm: cargo-generate-rpm
  - Windows .exe: cross-compile x86_64-pc-windows-gnu
CI: GitHub Actions matrix build, release artifacts on tag push.
