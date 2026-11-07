#!/usr/bin/env bash
#
# Particle Regression Gate
#
# Run this after ANY change to NFA construction, active-state tracking,
# UPA checking, or content-model validation.  It combines:
#
#   1. Internal unit suites  (compiler::nfa, compiler::upa, validation::content)
#   2. Green conformance groups that must stay at 100% (gated with --expect-pass)
#   3. Tracked conformance groups and focused regression cases (report-only)
#   4. Full particle catalog summary
#
# Usage:
#   ./scripts/particle-gate.sh                      # default: ../../xsdtests
#   ./scripts/particle-gate.sh /path/to/xsdtests    # custom test suite path
#
# Exit code is non-zero if any gated step fails.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
CRATE_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
TEST_SUITE="${1:-$CRATE_DIR/../../xsdtests}"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

gated_pass=0
gated_fail=0
info_count=0

step() {
    echo
    echo -e "${BOLD}=== $1 ===${NC}"
}

# Gated step: failure counts toward the gate result
run_gated() {
    local label="$1"
    shift
    step "$label"
    if "$@"; then
        echo -e "${GREEN}PASS${NC}: $label"
        gated_pass=$((gated_pass + 1))
    else
        echo -e "${RED}FAIL${NC}: $label"
        gated_fail=$((gated_fail + 1))
    fi
}

# Info step: report status but don't fail the gate
run_info() {
    local label="$1"
    shift
    step "$label"
    "$@" || true
    info_count=$((info_count + 1))
}

cd "$CRATE_DIR"

# ── 1. Internal unit suites (GATED) ─────────────────────────────────────

run_gated "NFA unit tests" \
    cargo test --lib compiler::nfa::tests:: --features xsd11

run_gated "UPA unit tests" \
    cargo test --lib compiler::upa::tests:: --features xsd11

run_gated "Content-model unit tests" \
    cargo test --lib validation::content::tests:: --features xsd11

run_gated "NFA compilation tests" \
    cargo test --lib compiler::compile::tests:: --features xsd11

# ── 2. Green conformance groups (GATED with --expect-pass) ───────────────
#
# These groups reached 100% during Phase 3/3b and must not regress.

if [ -d "$TEST_SUITE" ]; then
    for grp in particlesIj particlesIe particlesR particlesL particlesQ particlesJe; do
        run_gated "Conformance: $grp (100% required)" \
            cargo test --test conformance --features xsd11 -- \
                --test-suite "$TEST_SUITE" --version 1.0 --group "$grp" --expect-pass
    done

    # ── 3. Tracked groups & focused regression cases (INFO only) ─────────
    #
    # Runtime substitution / abstract heads (Step 4 targets):
    #   particlesDc002.v, particlesDc003.v, particlesDc007.v
    #
    # Runtime content-model regressions (Step 7 targets):
    #   particlesOb005.v, particlesOb032.v, particlesOb056.v

    run_info "Conformance: particlesHa (83/86 tracked)" \
        cargo test --test conformance --features xsd11 -- \
            --test-suite "$TEST_SUITE" --version 1.0 --group particlesHa

    run_info "Conformance: particlesOb (76/79 tracked)" \
        cargo test --test conformance --features xsd11 -- \
            --test-suite "$TEST_SUITE" --version 1.0 --group particlesOb

    run_info "Conformance: Dc regression slice" \
        cargo test --test conformance --features xsd11 -- \
            --test-suite "$TEST_SUITE" --version 1.0 \
            --name particlesDc002 --name particlesDc003 --name particlesDc007 \
            --verbose

    run_info "Conformance: Ob regression slice" \
        cargo test --test conformance --features xsd11 -- \
            --test-suite "$TEST_SUITE" --version 1.0 \
            --name particlesOb005 --name particlesOb032 --name particlesOb056 \
            --verbose

    # Step 9: Z033 counted UPA (cap-and-check)
    run_info "Step 9: Z033 counted UPA" \
        cargo test --test conformance --features xsd11 -- \
            --test-suite "$TEST_SUITE" --version 1.0 \
            --name particlesZ033_c --name particlesZ033_e \
            --name particlesZ033_f --name particlesZ033_g \
            --verbose

    # ── 4. XSD 1.1 particle catalog (GATED at 100%) ──────────────────────

    run_gated "Conformance: XSD 1.1 particles (100% required)" \
        cargo test --test conformance --features xsd11 -- \
            --test-suite "$TEST_SUITE" --version 1.1 --group particles --expect-pass

    # ── 5. Full XSD 1.0 particle catalog (INFO) ──────────────────────────

    run_info "Conformance: full particle catalog (1406 pass / 3 fail / 2 skip baseline)" \
        cargo test --test conformance --features xsd11 -- \
            --test-suite "$TEST_SUITE" --version 1.0 --group particles
else
    echo -e "${YELLOW}WARNING${NC}: Test suite not found at $TEST_SUITE"
    echo "  Skipping conformance regression tests."
    echo "  Pass the path as first argument: $0 /path/to/xsdtests"
fi

# ── Summary ──────────────────────────────────────────────────────────────

echo
echo -e "${BOLD}=== Particle Regression Gate Summary ===${NC}"
echo -e "  Gated steps passed: ${GREEN}${gated_pass}${NC}"
if [ "$gated_fail" -gt 0 ]; then
    echo -e "  Gated steps failed: ${RED}${gated_fail}${NC}"
fi
echo -e "  Info steps reported: ${CYAN}${info_count}${NC}"

if [ "$gated_fail" -gt 0 ]; then
    echo
    echo -e "${RED}GATE FAILED${NC} — investigate before pushing NFA/particle changes."
    exit 1
else
    echo
    echo -e "${GREEN}GATE PASSED${NC}"
    exit 0
fi
