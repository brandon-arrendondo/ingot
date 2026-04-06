"""
Invoke tasks for ingot development.

Usage:
    invoke check    # Run pre-commit hooks on all files
    invoke build    # Build in release mode
    invoke test     # Run all tests
    invoke clean    # Remove build artifacts and generated output
    invoke generate # Generate C code from an example model

Install invoke: pip install invoke
"""

from invoke import task


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
    """Run all Rust unit tests and generated C integration tests."""
    import tempfile, os

    c.run("cargo test", pty=True)

    unity_dir = os.path.join(os.path.dirname(__file__), "deps", "unity", "src")
    if not os.path.isfile(os.path.join(unity_dir, "unity.c")):
        print("Unity submodule not initialized — skipping C tests")
        print("  Run: git submodule update --init deps/unity")
        return

    models = ["examples/battery.toml", "examples/minimal.toml"]
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

    models = ["examples/battery.toml", "examples/minimal.toml"]
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
