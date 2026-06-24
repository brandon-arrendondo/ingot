"""
Invoke tasks for ingot development.

Usage:
    invoke check          # Run pre-commit hooks on all files
    invoke build          # Build in release mode
    invoke test           # Run all tests
    invoke clean          # Remove build artifacts and generated output
    invoke generate       # Generate C code from an example model
    invoke bump-version   # Bump version across all files (reads Cargo.toml)

Install invoke: pip install invoke
"""

import datetime
import re
from pathlib import Path

from invoke import task

# A semver core like 1.0.1 (no pre-release / build metadata).
SEMVER = r"\d+\.\d+\.\d+"


def _read_cargo_version():
    cargo = Path("Cargo.toml").read_text()
    # Match the package version line (anchored at start of line) — not the
    # `version = "..."` fields inside dependency tables.
    match = re.search(r'^version = "([^"]+)"', cargo, re.MULTILINE)
    if not match:
        raise RuntimeError("Could not find version in Cargo.toml")
    return match.group(1)


# Files that embed THIS crate's own version, with the pattern that locates it
# and a replacement template ({new} = new version, {date} = today YYYY-MM-DD).
#
# Patterns match any semver (not just the current one) so a bump heals any
# existing drift instead of silently skipping an out-of-sync file.
#
# NOT touched (intentionally):
#   - .pre-commit-config.yaml `rev: v1.8.2`  -> that pins knots, not this crate
VERSION_FILES = [
    # (path, pattern, replacement-template)
    ("Cargo.toml", r'^(version = ")' + SEMVER + r'(")', r"\g<1>{new}\g<2>"),
    (
        "docs/ingot.1",
        r'("ingot )' + SEMVER + r'(")',
        r"\g<1>{new}\g<2>",
    ),
    # Refresh the man page date stamp on every bump.
    (
        "docs/ingot.1",
        r'(\.TH INGOT 1 ")\d{4}-\d{2}-\d{2}(")',
        r"\g<1>{date}\g<2>",
    ),
]


@task
def bump_version(c, new_version=None):
    """Bump this crate's version across every file that embeds it.

    Reads the current version from Cargo.toml. With no --new-version, prints
    the current version and the files that would change (dry run). Otherwise
    rewrites Cargo.toml, the man page version, and the man page date.

    Args:
        new_version: Target version string, e.g. 1.0.2 (no leading 'v').
    """
    current = _read_cargo_version()

    if not new_version:
        print(f"Current version (Cargo.toml): {current}")
        print("\nFiles that would be updated:")
        for path, *_ in VERSION_FILES:
            print(f"  {path}")
        print("\nRun: invoke bump-version --new-version X.Y.Z")
        return

    if not re.fullmatch(SEMVER, new_version):
        raise SystemExit(f"--new-version must look like X.Y.Z, got '{new_version}'")

    today = datetime.date.today().isoformat()
    changed = []

    for path, pattern, tmpl in VERSION_FILES:
        p = Path(path)
        if not p.exists():
            continue
        text = p.read_text()
        replacement = tmpl.format(new=new_version, date=today)
        updated = re.sub(pattern, replacement, text, flags=re.MULTILINE)
        if updated != text:
            p.write_text(updated)
            changed.append(path)

    if changed:
        print(f"Bumped -> {new_version} in:")
        for f in sorted(set(changed)):
            print(f"  {f}")
        print(
            "\nNext: review `git diff`, commit, then "
            f"`git tag v{new_version} && git push && git push origin v{new_version}`"
        )
    else:
        print("No version strings matched — nothing changed.")


@task
def check(c):
    """Run pre-commit hooks on all files."""
    c.run("pre-commit run --all-files", pty=True)


@task
def build(c, release=False):
    """Build the project.

    Args:
        release: Build in release mode with LTO (default: debug).
    """
    cmd = "cargo build"
    if release:
        cmd += " --release"
    c.run(cmd, pty=True)


@task
def test(c):
    """Run all Rust unit tests and generated C/C++ integration tests."""
    import tempfile, os

    c.run("cargo test", pty=True)

    here = os.path.dirname(__file__)

    # --- C integration tests (Unity) ---
    unity_dir = os.path.join(here, "deps", "unity", "src")
    if os.path.isfile(os.path.join(unity_dir, "unity.c")):
        models = ["examples/battery.toml", "examples/minimal.toml", "examples/full.toml"]
        modes = [
            ("events", []),
            ("no-events", ["--no-events"]),
        ]
        for model in models:
            for mode_name, extra_flags in modes:
                with tempfile.TemporaryDirectory(prefix="ingot_ctest_") as tmp:
                    label = f"{os.path.basename(model)}[{mode_name}]"
                    build_dir = os.path.join(tmp, "build")
                    flags = " ".join(extra_flags)
                    c.run(
                        f"cargo run -q -- --model {model} --output {tmp} {flags}",
                        pty=True,
                    )
                    c.run(
                        f"cmake -S {tmp} -B {build_dir} -DUNITY_DIR={unity_dir} > /dev/null 2>&1",
                    )
                    c.run(f"cmake --build {build_dir} > /dev/null 2>&1")
                    print(f"\n=== C tests: {label} ===")
                    c.run(f"{build_dir}/test_dm", pty=True)
    else:
        print("Unity submodule not initialized — skipping C tests")
        print("  Run: git submodule update --init deps/unity")

    # --- C++/tinyfsm event dispatch: compile + link + run the generated wrapper ---
    tinyfsm_dir = os.path.join(here, "deps", "tinyfsm", "include")
    if os.path.isfile(os.path.join(tinyfsm_dir, "tinyfsm.hpp")):
        fixture = os.path.join(here, "tests", "fixtures", "cxx")
        with tempfile.TemporaryDirectory(prefix="ingot_cxxtest_") as tmp:
            gen = os.path.join(tmp, "gen")
            build_dir = os.path.join(tmp, "build")
            c.run(
                f"cargo run -q -- --model examples/full.toml --output {gen} --emit-tinyfsm",
                pty=True,
            )
            c.run(
                f"cmake -S {fixture} -B {build_dir}"
                f" -DGEN_DIR={gen} -DTINYFSM_DIR={tinyfsm_dir} > /dev/null 2>&1",
            )
            c.run(f"cmake --build {build_dir} > /dev/null 2>&1")
            print("\n=== C++ tinyfsm dispatch test ===")
            c.run(f"{build_dir}/tinyfsm_dispatch && echo PASS", pty=True)
    else:
        print("tinyfsm submodule not initialized — skipping C++ event test")
        print("  Run: git submodule update --init deps/tinyfsm")


@task
def coverage(c, output="coverage"):
    """Generate coverage reports for both Rust and C code.

    Rust: cargo-llvm-cov → {output}/lcov.info
    C:    gcov + gcovr → {output}/<variant>.xml (Cobertura)

    Args:
        output: Directory for coverage reports (default: coverage).
    """
    import shutil, os

    os.makedirs(output, exist_ok=True)

    # --- Rust coverage ---
    print("=== Rust coverage ===")
    c.run("scripts/coverage-gate.sh 80", pty=True)
    lcov_dest = os.path.join(output, "lcov.info")
    if os.path.isfile("lcov.info"):
        shutil.copy("lcov.info", lcov_dest)
        print(f"  Rust lcov: {lcov_dest}")

    # --- C coverage ---
    print("\n=== C coverage ===")
    unity_dir = os.path.join(os.path.dirname(__file__), "deps", "unity", "src")
    if not os.path.isfile(os.path.join(unity_dir, "unity.c")):
        print("Unity submodule not initialized — skipping C coverage")
        print("  Run: git submodule update --init deps/unity")
        return

    models = ["examples/battery.toml", "examples/minimal.toml", "examples/full.toml"]
    modes = [
        ("events", []),
        ("no-events", ["--no-events"]),
    ]

    # Collect gcov data across all runs into a persistent directory
    gcov_dir = os.path.join(output, "gcov_build")
    os.makedirs(gcov_dir, exist_ok=True)
    build_dirs = []

    for model in models:
        for mode_name, extra_flags in modes:
            label = f"{os.path.splitext(os.path.basename(model))[0]}_{mode_name.replace('-', '_')}"
            gen_dir = os.path.join(gcov_dir, label)
            build_dir = os.path.join(gen_dir, "build")
            os.makedirs(gen_dir, exist_ok=True)
            flags = " ".join(extra_flags)
            c.run(f"cargo run -q -- --model {model} --output {gen_dir} {flags}")
            c.run(
                f"cmake -S {gen_dir} -B {build_dir}"
                f" -DUNITY_DIR={unity_dir} -DCOVERAGE=ON > /dev/null 2>&1",
            )
            c.run(f"cmake --build {build_dir} > /dev/null 2>&1")
            c.run(f"{build_dir}/test_dm", pty=True)
            build_dirs.append((label, build_dir))

    # Generate per-variant Cobertura XML reports
    print("\n=== Coverage reports ===")
    for label, build_dir in build_dirs:
        gen_dir = os.path.dirname(build_dir)
        xml_path = os.path.join(output, f"{label}.xml")
        c.run(
            f"gcovr --root {gen_dir} {build_dir}"
            f" --xml {xml_path}"
            f" --exclude '.*test_dm\\.c'",
        )
        print(f"  {xml_path}")

    # Print summary from the first variant (representative)
    first_label, first_build = build_dirs[0]
    first_gen = os.path.dirname(first_build)
    c.run(
        f"gcovr --root {first_gen} {first_build}"
        f" --exclude '.*test_dm\\.c'"
        f" --print-summary",
        pty=True,
    )


@task
def clean(c):
    """Remove build artifacts and generated output."""
    c.run("cargo clean", pty=True)
    c.run("rm -rf generated/ coverage/ lcov.info", pty=True)


@task
def generate(c, model="examples/battery.toml", output="generated", target="linux64"):
    """Generate C code from a data model.

    Args:
        model: Path to TOML data model file.
        output: Output directory for generated C code.
        target: Target platform (stm32, esp-xtensa, esp-riscv, mcu8bit, linux64).
    """
    c.run(f"cargo run -- --model {model} --output {output} --target {target} -v", pty=True)
