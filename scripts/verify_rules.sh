#!/usr/bin/env bash
# Read-only verification gate for AGENTS.md & docs/DESIGN_RULES.md compliance.
# Safe to run at any time: performs grep/text scans only (no git, cargo, npm, or runtime calls).

set -uo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

FAILURES=0

check_zero_matches() {
    local label="$1"
    local pattern="$2"
    shift 2
    local matches
    matches="$(grep -rnE "$pattern" "$@" 2>/dev/null || true)"
    if [[ -n "$matches" ]]; then
        echo "[FAIL] $label"
        echo "$matches" | head -n 20
        FAILURES=$((FAILURES + 1))
    else
        echo "[PASS] $label"
    fi
}

echo "=== ONE-SHIP Read-Only Rule Verification ==="

# 1. Zero TypeScript `any` outside generated bindings
TS_ANY="$(grep -rnE --include='*.ts' --include='*.vue' ': any\b|as any\b|<any>' src | grep -v 'src/types/generated/' || true)"
if [[ -n "$TS_ANY" ]]; then
    echo "[FAIL] TypeScript 'any' found in src/"
    echo "$TS_ANY" | head -n 20
    FAILURES=$((FAILURES + 1))
else
    echo "[PASS] Zero TypeScript 'any' in src/"
fi

# 2. Zero native title= attributes on interactive HTML elements (AppEmptyState prop excluded)
NATIVE_TITLE="$(grep -rn --include='*.vue' -E '\stitle="' src | grep -v 'AppTooltip' | grep -v -E '^\s*title="' || true)"
if [[ -n "$NATIVE_TITLE" ]]; then
    echo "[WARN] Native title= attributes found (verify these are component props, not HTML attributes):"
    echo "$NATIVE_TITLE" | head -n 15
else
    echo "[PASS] Zero inline native title= attributes in src/"
fi

# 3. Zero bare lock().unwrap() / read().unwrap() / write().unwrap() outside #[cfg(test)]
LOCK_UNWRAP="$(grep -rnE '\.(read|write|lock)\(\)\.(unwrap|expect)\(' src-tauri/src || true)"
if [[ -n "$LOCK_UNWRAP" ]]; then
    echo "[WARN] Lock .unwrap()/.expect() found (must only exist inside #[cfg(test)]):"
    echo "$LOCK_UNWRAP" | head -n 15
else
    echo "[PASS] Zero lock .unwrap()/.expect() in src-tauri/src"
fi

# 4. Zero hardcoded Tailwind palettes in UI (Spec 13 §0 / §11.2)
check_zero_matches "Zero Tailwind palette classes (text-/bg-/border-{color}-*)" \
    '(text|bg|border)-(amber|emerald|rose|orange|fuchsia|sky|red|green|blue|yellow|zinc|slate|gray|neutral|purple|indigo|teal|cyan|pink)-[0-9]+' \
    src/components src/views

# 5. Zero hardcoded bg-black/* or bg-[#...] in UI
check_zero_matches "Zero raw bg-black/* or bg-[#...] outside ui.css" \
    'bg-black/|bg-\[#|text-\[7px\]' \
    src/components src/views

# 6. Zero Result<_, String> in non-test Rust modules (allow nested generics like Result<HashMap<String, String>, DomainError>)
RES_STRING="$(grep -rnE 'Result<([^<>]|<([^<>]|<[^<>]*>)*>)*,\s*String>' src-tauri/src | grep -v 'tests\.rs' || true)"
if [[ -n "$RES_STRING" ]]; then
    echo "[FAIL] Unstructured Result<_, String> found outside tests:"
    echo "$RES_STRING" | head -n 15
    FAILURES=$((FAILURES + 1))
else
    echo "[PASS] Zero Result<_, String> in production Rust modules"
fi

# 7. Zero backdrop-blur-* outside src/styles/ui.css
check_zero_matches "Zero backdrop-blur-* in components/views" \
    'backdrop-blur' \
    src/components src/views

# 8. Zero hardcoded /home/ paths
check_zero_matches "Zero hardcoded /home/butcherwutcher paths" \
    '/home/butcherwutcher' \
    src src-tauri/src

echo "=== Summary: $FAILURES hard failure(s) ==="
exit "$FAILURES"
