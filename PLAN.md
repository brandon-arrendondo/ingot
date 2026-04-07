# Ingot Roadmap

## Benchmarking: Cross-Platform ROM/RAM Comparison

**Goal:** Produce measured (not estimated) ROM and RAM numbers for ingot-generated
code versus competitive embedded KV storage schemes, across all supported targets.
These numbers will anchor the paper's comparison tables with real data and
demonstrate ingot's value proposition for multi-product IoT fleets.

### Motivation

A key differentiator for ingot is that a single TOML data model produces a
**common key encoding** that works identically across product platforms — from an
8051 with 32 KiB flash to an ESP32 with megabytes.  This gives cloud backends a
stable, compact identifier (32-bit encoded key) for any data point ("battery
status", "pump duty cycle") across an entire product suite, enabling arbitrary
cross-product queries without per-product dispatch logic.  The alternative — UUIDs
(128-bit) or string keys — is prohibitively expensive when storing hundreds of
data points (mostly booleans and 8-bit integers) on a 32 KiB or 64 KiB system.

The benchmarks should quantify this advantage concretely.

### Schemes to Benchmark

For each scheme, measure total ROM (code + const tables) and RAM (mutable data)
for the same data model (the `full.toml` 38-key model and a scaled 200-key model).

1. **Ingot (CHM perfect hash)** — baseline, generated C99
2. **Plain C struct** — hand-coded struct with direct member access (best-case
   ROM/RAM baseline, no key-based lookup, no type dispatch)
3. **Sorted array + binary search** — static sorted key-value array with
   `bsearch()`, O(log n) lookup
4. **Linear scan array** — unsorted key-value array, O(n) lookup (represents
   the simplest possible runtime KV store)
5. **Open-addressing hash table** — fixed-size hash table with linear probing,
   static allocation, ~75% load factor
6. **GNU gperf** — perfect hash from string keys (for string-key use cases)
7. **ESP-IDF NVS** — flash-resident, measured on ESP32 only (ROM = library code
   size; RAM = runtime overhead after init with 38 keys)
8. **Zephyr NVS** — flash-resident, measured on STM32 only

### Target Platforms

| Target | CPU | Flash | RAM | Toolchain |
|--------|-----|-------|-----|-----------|
| STM32F103 (Blue Pill) | Cortex-M3 @ 72 MHz | 64 KiB | 20 KiB | arm-none-eabi-gcc |
| STM32F407 (Discovery) | Cortex-M4F @ 168 MHz | 1 MiB | 192 KiB | arm-none-eabi-gcc |
| ESP32 (Xtensa) | LX6 @ 240 MHz | 4 MiB | 520 KiB | xtensa-esp32-elf-gcc |
| 8051 (STC8/AT89) | 8-bit @ 24 MHz | 32--64 KiB | 256 B--8 KiB | SDCC |
| Linux x86-64 | any | N/A | N/A | gcc -Os / -O2 |
| Linux ARM 32-bit (RPi) | Cortex-A53 (32-bit mode) | N/A | N/A | arm-linux-gnueabihf-gcc |

### Measurements

For each (scheme, target, model size) triple:

- **ROM**: `arm-none-eabi-size` .text + .rodata sections (bare-metal), or
  `size` on the .o files (Linux). Isolate the KV storage code from startup/libc.
- **RAM**: .data + .bss sections (bare-metal), or instrumented peak heap usage
  for dynamic schemes.
- **Lookup latency** (stretch goal): cycle-accurate measurement via DWT cycle
  counter (Cortex-M), `xthal_get_ccount()` (Xtensa), or `rdtsc` (x86).
  Measure worst-case single-key lookup across all keys.

### Methodology

- Write a minimal `main()` per scheme that initializes storage, performs one
  get and one set for every key, and returns. No printf, no UART, no OS.
- Compile with `-Os -ffunction-sections -fdata-sections -Wl,--gc-sections`
  to strip unused code.
- For ESP-IDF NVS: use `idf.py size-components` to isolate NVS library
  contribution.
- For 8051/SDCC: use SDCC's `--model-large` and measure code/xdata/idata from
  the .mem file.
- Report numbers in a table suitable for direct inclusion in the paper.

### Expected Outcome

Ingot should show:
- ROM within ~2x of plain C struct (the cost of hash tables + dispatch) but
  with key-based lookup that the struct doesn't provide
- ROM significantly smaller than ESP-IDF NVS and Zephyr NVS library code
- RAM comparable to plain C struct (just the value arrays)
- Constant lookup time vs O(n) for linear scan and O(log n) for binary search
- The 32-bit key encoding stores the same information as a 128-bit UUID in 4
  bytes — a 4x reduction that matters when transmitting hundreds of data points
  over constrained IoT links (BLE, LoRa, NB-IoT)

### Deliverables

- [ ] Benchmark harness: CMakeLists.txt or Makefile per target that builds all
      schemes from the same data model
- [ ] Size measurement script: parses `size` / `.map` / `.mem` output into CSV
- [ ] Results table: CSV + LaTeX table for the paper
- [ ] Paper update: replace estimated figures in Section 6 (Comparison) with
      measured data; add benchmark methodology section
- [ ] Lookup latency measurements (if DWT/cycle counter available)

### Key Encoding as Cross-Product Cloud Identifier

The benchmarking should also validate the cross-product identifier use case:

- Same `full.toml` model compiled for STM32, ESP32, 8051, and Linux produces
  **identical key values** (32-bit constants).  Verify byte-for-byte match of
  `key_definitions.h` across all targets.
- Compare wire cost: 38 keys x 4 bytes (ingot) = 152 bytes vs 38 x 16 bytes
  (UUID) = 608 bytes vs 38 x avg 15 bytes (string keys) = 570 bytes.
- For a 200-key model with mostly booleans and uint8s on a 32 KiB device,
  calculate what percentage of flash is consumed by each scheme's overhead
  alone (before application code).

---

## Future Paper Improvements

- [ ] Replace estimated cycle counts with measured DWT data
- [ ] Add production case study (Bissell deployment: before/after ROM, bug count)
- [ ] Evaluate CHD/BDZ space efficiency vs CHM at 200+ key scale
- [ ] Add string-to-key lookup table generation for text protocol integration
- [ ] Benchmark incremental regeneration time for large multi-namespace models
