# Ingot User Guide

Ingot is a C code generator for embedded key-value databases. It reads a TOML
data model specification and produces optimized C99 code with compile-time
perfect hashing for O(1) key lookup. All storage is statically allocated --
no heap, no dynamic memory, no fragmentation.

## Installation

### From source

```sh
cargo install --path .
```

### From crates.io (when published)

```sh
cargo install ingot
```

### Prerequisites

- Rust toolchain (1.70+)
- For running generated tests: CMake 3.10+, a C99 compiler, and the Unity
  test framework (included as a git submodule in `deps/unity`)

## Quick Start

1. Write a data model in TOML:

```toml
[meta]
id = "my_device"
version = "1.0.0"

[[classes]]
id = "config"

    [[classes.keys]]
    id = "brightness"
    type = "uint8"
    default = 100
```

2. Generate C code:

```sh
ingot --model my_device.toml --output generated/
```

3. Include the generated files in your build system and use the API:

```c
#include "dm.h"
#include "key_definitions.h"

DataModel_Initialize(NULL);

dm_val_t val;
val.u8val = 75;
DataModel_SetIntegralTypeByKey(DM_KEY_MY_DEVICE_CONFIG_BRIGHTNESS, val);

dm_val_t out = DataModel_GetIntegralTypeByKey(DM_KEY_MY_DEVICE_CONFIG_BRIGHTNESS);
// out.u8val == 75
```

## CLI Reference

```
ingot --model <PATH> [--output <DIR>] [--target <TARGET>] [--no-events] [-v]
```

| Option | Default | Description |
|--------|---------|-------------|
| `--model <PATH>` | (required) | Path to TOML data model file |
| `--output <DIR>` | `generated/` | Output directory for generated C code |
| `--target <TARGET>` | `linux64` | Target platform (see below) |
| `--no-events` | off | Disable event callback generation |
| `-v` / `-vv` / `-vvv` | warn | Verbosity: info / debug / trace |

### Targets

| Value | Platform | Mutex | Notes |
|-------|----------|-------|-------|
| `stm32` | 32-bit ARM STM32 | None (bare-metal) | 4-byte alignment |
| `esp-xtensa` | ESP32 Xtensa | FreeRTOS semaphore | `xSemaphoreTake`/`Give` |
| `esp-riscv` | ESP32 RISC-V | FreeRTOS semaphore | `xSemaphoreTake`/`Give` |
| `mcu8bit` | 8-bit MCU | None (bare-metal) | 1-byte alignment, 16-bit pointers |
| `linux64` | 64-bit Linux | pthread mutex | `PTHREAD_MUTEX_INITIALIZER` |

Thread-safe keys are protected by the target's mutex mechanism. Keys without
`thread_safe = true` bypass locking entirely.

## Schema Reference

A data model file has three top-level sections: `[meta]`, `[enums]`, and
`[[classes]]`.

### Meta

```toml
[meta]
id = "my_device"        # Namespace identifier (required)
version = "1.0.0"       # Semantic version (required)
doc = "Description"     # Optional documentation
```

The `id` is used to prefix all generated `#define` names:
`DM_KEY_{ID}_{CLASS}_{KEY}`.

### Enums

Enums define named integer-to-label mappings reusable across keys.

```toml
[enums.mode]
doc = "Operating mode"

[enums.mode.values]
off = 0
idle = 1
active = 2

# Optional: per-variant overrides
[enums.mode.variants.compact]
off = 0
active = 1
```

Reference an enum on a key with `enum = "mode"`. The enum values appear as
comments in the generated `key_definitions.h`.

### Classes

Classes group related keys within a namespace. A namespace can have up to 31
classes, each with up to 1023 keys.

```toml
[[classes]]
id = "status"
doc = "Runtime status"

    [[classes.keys]]
    id = "temperature"
    type = "uint16"
    # ... attributes
```

### Key Attributes

| Attribute | Type | Default | Description |
|-----------|------|---------|-------------|
| `id` | string | (required) | Key identifier |
| `type` | string | (required) | Data type (see below) |
| `default` | value | type zero | Default value |
| `defaults` | table | `{}` | Per-variant default overrides |
| `enum` | string | none | Reference to an `[enums]` entry |
| `max_size` | integer | none | Max bytes (required for string/binary) |
| `read_only` | bool | `false` | Prevent modification at runtime |
| `thread_safe` | bool | `false` | Protect with target mutex |
| `persistent` | bool | `false` | Include in binary save/load |
| `event` | bool | `false` | Fire callback on value change |
| `helpers` | bool | `false` | Generate named getter/setter functions |
| `unit` | string | none | Physical unit (documentary) |
| `doc` | string | none | Documentation string |

### Data Types

| Type | C Type | Size | Notes |
|------|--------|------|-------|
| `bool` | `bool` | 1 bit | Packed into uint32_t bitfields |
| `uint8` | `uint8_t` | 1 byte | |
| `int8` | `int8_t` | 1 byte | |
| `uint16` | `uint16_t` | 2 bytes | |
| `int16` | `int16_t` | 2 bytes | |
| `uint32` | `uint32_t` | 4 bytes | |
| `int32` | `int32_t` | 4 bytes | |
| `string` | `char[]` | max_size | Requires `max_size` attribute |
| `binary` | `uint8_t[]` | max_size | Requires `max_size` (schema only, codegen pending) |

### Per-Variant Defaults

Keys can have different default values for different product variants:

```toml
[[classes.keys]]
id = "threshold"
type = "uint8"
default = 10              # Base default

[classes.keys.defaults]
compact = 5               # Override for "compact" variant
pro = 15                  # Override for "pro" variant
```

The base `default` applies when no variant-specific override exists.

### Constraints

- `string` and `binary` types must specify `max_size`
- `read_only` keys cannot be `persistent` (validated at parse time)
- `enum` references must point to a defined `[enums]` entry
- Class IDs must be unique within a namespace
- Key IDs must be unique within a class

## Generated API

### Initialization

```c
#include "dm.h"

// With event callbacks:
void my_callback(uint32_t key) { /* handle change */ }
DataModel_Initialize(my_callback);

// Without events (--no-events):
DataModel_Initialize();

// Cleanup:
DataModel_TearDown();
```

### Reading and Writing Values

All integral types (bool, integers) use a type-dispatch API:

```c
#include "dm.h"
#include "key_definitions.h"

// Set a value
dm_val_t val = { 0 };
val.u16val = 42;
DM_RETURN_CODE rc = DataModel_SetIntegralTypeByKey(DM_KEY_..., val);

// Get a value
dm_val_t out = DataModel_GetIntegralTypeByKey(DM_KEY_...);
uint16_t temp = out.u16val;
```

Typed convenience setters are also generated:

```c
DataModel_SetBooleanByKey(DM_KEY_..., true);
DataModel_SetUInt16ByKey(DM_KEY_..., 42);
DataModel_SetInt32ByKey(DM_KEY_..., -7);
```

### String API

```c
// Get
const char *str = NULL;
DM_RETURN_CODE rc = DataModel_GetStringByKey(DM_KEY_..., &str);

// Set (read-write strings only)
rc = DataModel_SetStringByKey(DM_KEY_..., "new value");
```

String sets are bounds-checked against `max_size`. Strings exceeding the limit
are rejected.

### Helper Functions

Keys with `helpers = true` get named getter/setter functions:

```c
#include "dm_helpers.h"

// Integral types: static inline in the header
uint16_t temp = DataModel_Get_MY_DEVICE_STATUS_TEMPERATURE();
DataModel_Set_MY_DEVICE_STATUS_TEMPERATURE(2950);

// String types: declarations in header, implementations in dm_helpers.c
const char *name = DataModel_Get_MY_DEVICE_CONFIG_DEVICE_NAME();
DataModel_Set_MY_DEVICE_CONFIG_DEVICE_NAME("new-name");
```

### Return Codes

```c
typedef enum {
    DM_RETURN_CODE_SUCCESS              =  1,
    DM_RETURN_CODE_NOT_INITIALIZED      = -1,
    DM_RETURN_CODE_KEY_TYPE_INCORRECT   = -2,
    DM_RETURN_CODE_SET_ON_READONLY_KEY  = -3,
    DM_RETURN_CODE_STORAGE_LOOKUP_FAILURE = -4,
    DM_RETURN_CODE_SET_VALUE_UNCHANGED  = -5,
    DM_RETURN_CODE_NO_KEYS_OF_TYPE      = -6,
    DM_RETURN_CODE_OUTPUT_VARIABLE_NULL = -7,
    // When persistence is enabled:
    DM_RETURN_CODE_PERSISTENCE_FILE_NOT_FOUND  = -8,
    DM_RETURN_CODE_PERSISTENCE_READ_FAILURE    = -9,
    DM_RETURN_CODE_PERSISTENCE_WRITE_FAILURE   = -10,
    DM_RETURN_CODE_PERSISTENCE_MAGIC_MISMATCH  = -11,
} DM_RETURN_CODE;
```

### Key Query Macros

The generated `dm_key.h` provides macros to inspect key properties at runtime:

```c
#include "dm_key.h"

DM_IS_KEY_READONLY(key)       // Is this key read-only?
DM_IS_KEY_THREADSAFE(key)     // Does this key require mutex protection?
DM_IS_KEY_DATA_TYPE(key, t)   // Does this key have data type t?
DM_IS_KEY_IN_NAMESPACE(key, n)// Is this key in namespace n?
DM_KEY_GET_TYPE(key)          // Extract the 4-bit type code
```

### Persistence

Keys marked `persistent = true` can be saved to and loaded from a binary file:

```c
#include "persistence_storage.h"

// Save all persistent key values to a file
DM_RETURN_CODE rc = DataModel_SavePersistentKeys("/data/dm.bin");

// Load and restore persistent key values from a file
rc = DataModel_LoadPersistentKeys("/data/dm.bin");

// Pass NULL to use the default path ("/sdcard/dm.bin")
rc = DataModel_LoadPersistentKeys(NULL);

// Check if a specific key is persistent
bool is_persistent = PersistenceStorage_IsKeyPersistent(DM_KEY_...);
```

The binary format uses a packed struct with a magic number
(`sizeof(struct) XOR num_keys`) to detect layout changes. If the magic number
does not match on load, `DM_RETURN_CODE_PERSISTENCE_MAGIC_MISMATCH` is
returned and the live storage is not modified.

## Build System Integration

### CMake

The generated `CMakeLists.txt` is for the Unity test suite. For your
application, add the generated `.c` files to your build:

```cmake
add_library(data_model STATIC
    generated/jenkins_hash.c
    generated/boolean_storage.c
    generated/integer_storage.c
    generated/string_storage.c
    generated/persistence_storage.c  # if persistent keys exist
    generated/dm.c
    generated/dm_helpers.c           # if string helpers exist
)
target_include_directories(data_model PUBLIC generated/)
```

### Make

```makefile
DM_SRCS = $(wildcard generated/*.c)
DM_OBJS = $(DM_SRCS:.c=.o)

CFLAGS += -Igenerated/ -std=c99

libdm.a: $(DM_OBJS)
	$(AR) rcs $@ $^
```

### ESP-IDF

Create a component directory and symlink or copy the generated files:

```
components/data_model/
    CMakeLists.txt
    include/          # symlink to generated headers
    src/              # symlink to generated .c files
```

## Testing Generated Code

Ingot generates a Unity test suite (`test_dm.c`) that validates:

- **Default values**: every key returns its declared default after init
- **Set/get roundtrip**: set a test value, read it back, verify equality
- **Read-only rejection**: SetIntegralTypeByKey returns error for read-only keys
- **Unchanged detection**: setting the same value returns SET_VALUE_UNCHANGED
- **String roundtrip**: set and get for read-write string keys
- **Persistence roundtrip**: save, reset, load, verify (when persistent keys exist)

To run the generated tests:

```sh
# Initialize Unity submodule (first time)
git submodule update --init deps/unity

# Build and run
cmake -S generated/ -B build/ -DUNITY_DIR=deps/unity/src
cmake --build build/
./build/test_dm
```

Or use the invoke task which runs all models in both events/no-events modes:

```sh
invoke test
```

## Examples

Three example models are included in `examples/`:

| Model | Keys | Classes | Enums | Features |
|-------|------|---------|-------|----------|
| `minimal.toml` | 5 | 2 | 1 | Bool, integers, string, persistence, helpers |
| `battery.toml` | 18 | 2 | 2 | Per-variant defaults, enums with variants, read-only strings, events |
| `full.toml` | 38 | 4 | 3 | All types, all flags, comprehensive coverage |

Pre-generated C output for each model is in `examples/generated/`.

## 32-Bit Key Encoding

Each key is encoded as a 32-bit unsigned integer with the following bit layout:

```
| Namespace | Class   | ID      | Type    | Thread  | Derived | Read    |
| (10 bits) | (5 bits)| (10 bit)| (4 bit) | Safe(1) | (1 bit) | Only(1) |
| 31-22     | 21-17   | 16-7    | 6-3     | 2       | 1       | 0       |
```

This encoding allows up to 1024 namespaces, 32 classes per namespace, and
1024 keys per type per class. The type field supports 16 data types. Key
properties (read-only, thread-safe) are encoded directly in the key value
for O(1) property checks.

## Perfect Hash Algorithm

Ingot uses the CHM (Czech-Havas-Majewski) algorithm with a 2-seed Jenkins
lookup3 `final()` hash function. For each group of keys (by type), it:

1. Picks two random 32-bit seeds
2. Builds an undirected graph where edges map to keys
3. Verifies the graph is acyclic (retries with new seeds if not)
4. Assigns G-table values so `(G[h1(key)] + G[h2(key)]) % n` gives a unique
   index for each key

The G-table is typically ~2x the number of keys. Values are stored in the
smallest signed integer type that fits (int8_t, int16_t, or int32_t).
