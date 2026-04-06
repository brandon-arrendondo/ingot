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
    """Run all Rust unit tests."""
    c.run("cargo test", pty=True)


@task
def clean(c):
    """Remove build artifacts and generated output."""
    c.run("cargo clean", pty=True)
    c.run("rm -rf generated/", pty=True)


@task
def generate(c, model="examples/battery.toml", output="generated", target="linux64"):
    """Generate C code from a data model.

    Args:
        model: Path to TOML data model file.
        output: Output directory for generated C code.
        target: Target platform (stm32, esp-xtensa, esp-riscv, mcu8bit, linux64).
    """
    c.run(f"cargo run -- --model {model} --output {output} --target {target} -v", pty=True)
