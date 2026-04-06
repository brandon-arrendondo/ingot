#!/usr/bin/env bash
# Coverage gate script — runs Rust tests with coverage, produces lcov.info, enforces threshold.
# Used by pre-commit hook and CI pipeline.
#
# Usage: scripts/coverage-gate.sh [THRESHOLD]
#   THRESHOLD: minimum line coverage percentage (default: 80)
#
# Output: lcov.info in project root (publishable as CI artifact)
# Exit code: 0 if coverage >= threshold, 1 otherwise
#
# Prerequisites: cargo install cargo-llvm-cov

set -euo pipefail

THRESHOLD="${1:-80}"
# Exclude: main.rs (CLI entry point), codegen/mod.rs (template orchestration + file I/O,
# tested via C integration tests rather than Rust unit tests)
EXCLUDE_REGEX="(main\.rs|codegen/mod\.rs)"
LCOV_OUT="lcov.info"

echo "Running Rust tests with coverage (threshold: ${THRESHOLD}%)..."
echo "Excluding: ${EXCLUDE_REGEX}"
echo ""

cargo llvm-cov --lcov --output-path "${LCOV_OUT}" --ignore-filename-regex "${EXCLUDE_REGEX}" 2>&1
test_exit=$?

if [ $test_exit -ne 0 ]; then
    echo ""
    echo "FAILED: tests did not pass (exit code ${test_exit})"
    exit $test_exit
fi

echo ""
echo "Tests passed. Checking coverage threshold..."
echo ""

cargo llvm-cov report --summary-only --ignore-filename-regex "${EXCLUDE_REGEX}" 2>&1 | tail -2

COVERAGE=$(cargo llvm-cov report --summary-only --ignore-filename-regex "${EXCLUDE_REGEX}" 2>&1 \
    | grep '^TOTAL' \
    | awk '{for(i=1;i<=NF;i++){if($i ~ /^[0-9]+\.[0-9]+%$/){print $i+0; exit}}}')

if [ -z "${COVERAGE}" ]; then
    echo "WARNING: could not parse coverage percentage"
    exit 1
fi

echo ""
echo "Line coverage: ${COVERAGE}% (threshold: ${THRESHOLD}%)"

PASS=$(awk "BEGIN {print (${COVERAGE} >= ${THRESHOLD}) ? 1 : 0}")

if [ "${PASS}" -eq 1 ]; then
    echo "PASSED: coverage meets threshold"
    echo ""
    echo "Output: ${LCOV_OUT}"
    exit 0
else
    echo "FAILED: coverage ${COVERAGE}% is below ${THRESHOLD}% threshold"
    echo ""
    echo "Largest uncovered files:"
    cargo llvm-cov report --summary-only --ignore-filename-regex "${EXCLUDE_REGEX}" 2>&1 \
        | grep '\.rs' \
        | awk '{
            fname=$1; uncov=0; pct=0;
            for(i=2;i<=NF;i++){
                if($i ~ /^[0-9]+\.[0-9]+%$/){pct=$i+0; uncov=$(i-1)+0; break}
            }
            if(uncov>0 && pct<70) printf "%6d uncovered (%5.1f%%) %s\n", uncov, pct, fname
        }' | sort -rn | head -10
    exit 1
fi
