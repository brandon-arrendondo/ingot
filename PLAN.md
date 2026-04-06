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
# Status: done
# Dependencies: 4
# Priority: P2
# Description: Generate C code for boolean storage using bitfield packing.
# Details:
DONE. Implemented in src/codegen/storage/boolean.rs:
  - Pack booleans into uint32_t words (32 per word), ceil(n/32) words
  - SetBit/ClearBit/TestBit macros for bit manipulation
  - Perfect hash maps encoded key → bit index
  - Default values packed into initial uint32_t storage words
  - Tera templates: boolean_storage.{h,c}
  - BooleanStorage_SetKey / BooleanStorage_GetKey API
  - 5 tests (battery, minimal, no-bools, defaults, ceiling division)
  - Generated C compiles clean: gcc -Wall -Wextra -Wpedantic -std=c99

---

# Task ID: 8
# Title: String storage codegen (RO + RW)
# Status: done
# Dependencies: 4
# Priority: P2
# Description: Generate C code for string storage with separate RO/RW paths.
# Details:
DONE. Implemented in src/codegen/storage/string.rs:
  - RO: const char * const array of string literals, GetReadOnlyKey()
  - RW: individual static char arrays with max_size, pointer array for
    hash-based access, max_size array for bounds checking
  - SetKey() rejects strings >= max_size, NULL clears storage
  - GetKey() returns pointer to storage, GetMaxSize() returns limit
  - Separate perfect hashes for RO and RW groups
  - Tera templates: string_storage.{h,c} with conditional RO/RW sections
  - 5 tests (battery RO, minimal RW, no-strings, mixed, escaping)
  - Generated C compiles clean for both RO-only and RW-only models

---

# Phase 4: API Layer + Helpers

# Task ID: 9
# Title: Main data model API codegen
# Status: done
# Dependencies: 6, 7, 8
# Priority: P2
# Description: Generate dm.h/dm.c with type-dispatch get/set functions.
# Details:
DONE. Three new templates: dm_key.h, dm.h, dm.c.
  - dm_key.h: DataModel_Key union (bitfield + uint32_t), DM_KEY_TYPE enum,
    query macros (IS_KEY_READONLY, IS_KEY_DERIVED, IS_KEY_THREADSAFE,
    IS_KEY_DATA_TYPE, IS_KEY_IN_NAMESPACE, KEY_GET_TYPE)
  - dm.h: DM_RETURN_CODE enum, dm_val_t union, DataModel_Initialize/TearDown,
    SetIntegralTypeByKey/GetIntegralTypeByKey, typed convenience setters
    (SetBooleanByKey, SetUInt8ByKey, etc.), Get/SetStringByKey
  - dm.c: Type-dispatch switch via DM_KEY_GET_TYPE(key), routes to per-type
    storage modules. Set checks: initialized, read-only, value-unchanged.
    Event callback fired on value change. Conditional compilation based on
    which storage types exist (bool, each int type, RO/RW strings).
  - Typed convenience setters wrap SetIntegralTypeByKey via dm_val_t union
  - String API: conditional RO/RW dispatch via IS_KEY_READONLY
  - Generated C compiles clean for both battery and minimal models
  - 13 output files total per model

---

# Task ID: 10
# Title: Key definitions + namespace definitions codegen
# Status: done
# Dependencies: 2
# Priority: P2
# Description: Generate dm_key_definitions.h and dm_namespace_definitions.h.
# Details:
DONE. key_definitions.h was done in Phase 2.
  - dm_namespace_definitions.h: single #define DM_NAMESPACE_{NAME} {id}
  - Tera template: dm_namespace_definitions.h

---

# Task ID: 11
# Title: Auto-generated helper getter/setter codegen
# Status: done
# Dependencies: 9
# Priority: P2
# Description: Generate inline helpers for keys with generate_helpers=true.
# Details:
DONE. Implemented in codegen::collect_helpers() + templates.
  - Integral types (bool/int): static inline get/set in dm_helpers.h
    Get returns the C type directly, Set wraps dm_val_t dispatch
  - String types: declarations in dm_helpers.h, implementations in dm_helpers.c
    Get returns const char*, Set returns DM_RETURN_CODE
  - Read-only keys only get a getter (no setter generated)
  - dm_helpers.c only emitted when string helpers exist
  - Naming: DataModel_Get_{NS}_{CLASS}_{KEY}() / DataModel_Set_{NS}_{CLASS}_{KEY}()
  - Generated C compiles clean for both battery (integral-only) and minimal (mixed)

---

# Phase 5: Events, Derived Keys, Persistence

# Task ID: 12
# Title: Event system codegen
# Status: done
# Dependencies: 9
# Priority: P3
# Description: Generate key change event callbacks and dispatcher.
# Details:
DONE. Event callback is built into dm.c/dm.h (implemented as part of Task 9).
  - DataModel_Event_Callback typedef + DataModel_Initialize(callback)
  - Callback fires after successful set (integral and string)
  - --no-events CLI flag disables all event code: Initialize takes void,
    no callback variable, no dispatch after set
  - FSM event enums dropped (not used in practice)
  - Unity tests pass in both events and no-events modes

---

# Task ID: 13
# Title: Derived key codegen
# Status: dropped
# Dependencies: 9
# Priority: P3
# Description: Generate computed read-only values from operation expressions.
# Details:
DROPPED. Not used in practice. InstanceDef struct and empty-expression
validation removed from codebase. The derived bit (bit 1) remains in the
32-bit key encoding for forward compatibility.

---

# Task ID: 14
# Title: Persistence storage codegen
# Status: done
# Dependencies: 9
# Priority: P3
# Description: Generate serialization/deserialization for persistent keys.
# Details:
DONE. Implemented in src/codegen/storage/persistence.rs:
  - Packed C struct (PersistenceStorage_T) with #pragma pack(push, 1)
  - Magic number: sizeof(struct) XOR num_keys (catches layout changes)
  - Filesystem load/save via fopen/fread/fwrite (DEFAULT_DM_FILENAME "/sdcard/dm.bin")
  - SyncToStorage: after load, pushes values into live hash-indexed storage
    via DataModel_Set*ByKey APIs
  - SyncFromStorage: before save, reads live storage via DataModel_Get*ByKey
  - PersistenceStorage_IsKeyPersistent() switch-based key query
  - Supported types: bool, uint8/int8/uint16/int16/uint32/int32, string (char[max_size])
  - Validation: read-only keys cannot be persistent (ReadOnlyPersistent error)
  - 4 new persistence return codes in DM_RETURN_CODE enum
  - Conditional generation: only when persistent keys exist in model
  - Unity tests: file-not-found + save/load roundtrip per model
  - Tera templates: persistence_storage.{h,c}
  - 6 Rust unit tests in persistence.rs
  - Battery model: 54 tests (2 persistent uint8 keys), Minimal: 16 tests (1 persistent uint32)

---

# Phase 6: Multi-Target, Packaging, Documentation

# Task ID: 15
# Title: Target abstraction implementation
# Status: done
# Dependencies: 6
# Priority: P2
# Description: Implement target-specific code generation variations.
# Details:
DONE. TargetConfig now Serialize with per-target mutex fields.
  - STM32: bare-metal, no mutex, 4-byte align
  - ESP32 (Xtensa/RISC-V): FreeRTOS SemaphoreHandle_t, xSemaphoreTake/Give
  - 8-bit MCU: bare-metal, no mutex, 1-byte align, 16-bit pointers
  - Linux64: pthread_mutex_t with PTHREAD_MUTEX_INITIALIZER
  - dm.c template: conditional mutex include, decl, init/destroy, lock/unlock
  - Thread-safe key access: DM_IS_KEY_THREADSAFE(key) gates lock/unlock
  - Mutex wraps Set/GetIntegralTypeByKey and Get/SetStringByKey
  - Event callback fired after mutex unlock (outside critical section)
  - --target CLI flag wired through to codegen::generate()
  - 4 new unit tests for target configs
  - Unity tests pass on Linux64 with mutex enabled (52 tests, 0 failures)

---

# Task ID: 16
# Title: Unity integration test framework
# Status: done
# Dependencies: 6, 7, 8, 9
# Priority: P2
# Description: Generate Unity test files for generated data models.
# Details:
DONE. Generates test_dm.c + CMakeLists.txt per model.
  - Default value tests: verify every key returns its declared default
  - Set/get roundtrip tests: set a test value, get it back, verify equality
  - Read-only rejection tests: verify SetIntegralTypeByKey returns error
  - Unchanged value tests: verify setting same value returns SET_VALUE_UNCHANGED
  - String roundtrip tests: set string, get string, compare
  - CMakeLists.txt: builds against Unity (UNITY_DIR), links all storage + dm
  - Battery model: 52 tests, 0 failures
  - Minimal model: 14 tests, 0 failures (includes string roundtrip)
  - Templates: test_dm.c, CMakeLists.txt

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
