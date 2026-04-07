#!/usr/bin/env python3
"""Estimate ROM usage of generated ingot C code.

Compiles each generated .c file to an object file with gcc, then uses
`size -A` to measure .text (code), .rodata (constants/tables), and .data
(initialized globals) — all of which land in ROM on a typical embedded
target.

Requirements: gcc, size (binutils)

Usage:
    python3 scripts/estimate_rom.py <generated-dir> [--gcc PATH] [-O LEVEL]
"""

import argparse
import os
import re
import subprocess
import sys
import tempfile

# Files to skip (test harness, not part of the runtime)
SKIP_FILES = {"test_dm.c", "CMakeLists.txt"}

# Sections that contribute to ROM
ROM_SECTIONS = {".text", ".rodata", ".data"}


def find_c_files(directory):
    files = []
    for name in sorted(os.listdir(directory)):
        if name.endswith(".c") and name not in SKIP_FILES:
            files.append(os.path.join(directory, name))
    return files


def compile_to_object(c_file, include_dir, gcc, opt_level, tmpdir):
    base = os.path.splitext(os.path.basename(c_file))[0]
    obj = os.path.join(tmpdir, base + ".o")
    cmd = [gcc, "-c", "-std=c99", f"-O{opt_level}", f"-I{include_dir}", c_file, "-o", obj]
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        print(f"  error compiling {os.path.basename(c_file)}:", file=sys.stderr)
        print(f"    {result.stderr.strip()}", file=sys.stderr)
        return None
    return obj


def parse_size_output(obj_file, size_cmd="size"):
    result = subprocess.run([size_cmd, "-A", obj_file], capture_output=True, text=True)
    if result.returncode != 0:
        return {}
    sections = {}
    for line in result.stdout.splitlines():
        m = re.match(r"^(\.\S+)\s+(\d+)", line)
        if m:
            name, sz = m.group(1), int(m.group(2))
            sections[name] = sections.get(name, 0) + sz
    return sections


def format_bytes(n):
    if n >= 1024:
        return f"{n:>8,}  ({n / 1024:.1f} KB)"
    return f"{n:>8,}"


def main():
    parser = argparse.ArgumentParser(description="Estimate ROM usage of generated ingot code")
    parser.add_argument("directory", help="Path to generated C output directory")
    parser.add_argument("--gcc", default="gcc", help="gcc binary (default: gcc)")
    parser.add_argument("-O", dest="opt", default="s", help="Optimization level (default: s)")
    args = parser.parse_args()

    gen_dir = os.path.abspath(args.directory)
    if not os.path.isdir(gen_dir):
        print(f"Error: {gen_dir} is not a directory", file=sys.stderr)
        sys.exit(1)

    c_files = find_c_files(gen_dir)
    if not c_files:
        print(f"Error: no .c files found in {gen_dir}", file=sys.stderr)
        sys.exit(1)

    # Print compiler info
    ver = subprocess.run([args.gcc, "--version"], capture_output=True, text=True)
    compiler_line = ver.stdout.splitlines()[0] if ver.returncode == 0 else args.gcc
    print(f"Compiler: {compiler_line}")
    print(f"Optimization: -O{args.opt}")
    print()

    total_text = 0
    total_rodata = 0
    total_data = 0

    file_results = []

    with tempfile.TemporaryDirectory() as tmpdir:
        for c_file in c_files:
            obj = compile_to_object(c_file, gen_dir, args.gcc, args.opt, tmpdir)
            if obj is None:
                continue
            sections = parse_size_output(obj)
            text = sections.get(".text", 0)
            rodata = sections.get(".rodata", 0)
            data = sections.get(".data", 0)
            rom = text + rodata + data
            total_text += text
            total_rodata += rodata
            total_data += data
            file_results.append((os.path.basename(c_file), text, rodata, data, rom))

    # Print per-file breakdown
    name_width = max(len(r[0]) for r in file_results) if file_results else 20
    print(f"{'File':<{name_width}}  {'Code':>8}  {'Tables':>8}  {'Data':>8}  {'ROM':>8}")
    print(f"{'-' * name_width}  {'-' * 8}  {'-' * 8}  {'-' * 8}  {'-' * 8}")

    for name, text, rodata, data, rom in file_results:
        print(f"{name:<{name_width}}  {text:>8}  {rodata:>8}  {data:>8}  {rom:>8}")

    total_rom = total_text + total_rodata + total_data
    print(f"{'-' * name_width}  {'-' * 8}  {'-' * 8}  {'-' * 8}  {'-' * 8}")
    print(f"{'TOTAL':<{name_width}}  {total_text:>8}  {total_rodata:>8}  {total_data:>8}  {total_rom:>8}")
    print()
    print(f"Estimated ROM: {format_bytes(total_rom)}")
    print(f"  .text (code):     {format_bytes(total_text)}")
    print(f"  .rodata (tables): {format_bytes(total_rodata)}")
    print(f"  .data (init):     {format_bytes(total_data)}")
    print()
    print("Note: actual usage varies with target architecture, compiler, and")
    print("optimization settings. This estimate uses host gcc as a baseline.")


if __name__ == "__main__":
    main()
