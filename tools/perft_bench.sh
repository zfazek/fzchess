#!/usr/bin/env bash
#
# Perft speed benchmark: Rust (release) vs C++ (-O2), on identical positions.
#
# Perft is a pure, deterministic move-generation workload with no eval/search,
# so it isolates move-gen throughput. Both engines produce identical node counts
# (verified by tools/golden.sh), so this measures speed on the same work.
#
# IMPORTANT CAVEAT — the two engines do slightly different per-node work:
# both skip eval/repetition/material/Zobrist/pawn-list bookkeeping on the perft
# path (the Rust port gates all of it behind its `fake` flag; perft never reads
# it). The remaining difference is genuine move-generation + make/unmake speed,
# so this is a reasonably fair movegen comparison. Node counts are identical
# (verified by tools/golden.sh), so both engines do the same search work.
#
# Usage: tools/perft_bench.sh [runs]
#   runs: how many times to repeat each case (default 3; reports the best/min
#         time, which is the least noisy estimate of peak throughput).

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUST_BIN="$ROOT/rust/target/release/fzchess"
RUNS="${1:-3}"

# Benchmark cases: "label|depth|fen". Chosen to be large enough (millions of
# nodes) for stable timing. The C++ engine's own perft suite prints per-depth
# timing, which we parse for its side.
CASES=(
  "Kiwipete|4|r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1"
  "Promotion|5|r2q1rk1/pP1p2pp/Q4n2/bbp1p3/Np6/1B3NBn/pPPP1PPP/R3K2R b KQ - 0 1"
)

ensure_builds() {
  echo ">> building C++ (-O2) and Rust (release)..." >&2
  cmake --build "$ROOT/build" >/dev/null 2>&1
  (cd "$ROOT/rust" && cargo build --release >/dev/null 2>&1)
}

# Best (minimum) milliseconds over $RUNS runs of the Rust bench for one case.
rust_best_ms() {
  local depth="$1" fen="$2" best="" ms
  for _ in $(seq "$RUNS"); do
    ms="$("$RUST_BIN" perft-bench "$depth" "$fen" \
          | sed -nE 's/.*time_ms=([0-9.]+).*/\1/p')"
    if [[ -z "$best" ]] || awk "BEGIN{exit !($ms < $best)}"; then best="$ms"; fi
  done
  echo "$best"
}

# The C++ engine runs its whole fixed perft suite (arg 2) and prints per-depth
# time. We run it $RUNS times and take, for the matching (nodes) line, the best
# time. Matching by node count is robust to suite ordering.
cpp_best_ms_for_nodes() {
  local want_nodes="$1" best="" ms
  for _ in $(seq "$RUNS"); do
    ms="$("$ROOT/build/src/fzchess" 2 2>/dev/null \
          | grep -E "nodes: ${want_nodes} " \
          | sed -nE 's/.* time: ([0-9]+) ms.*/\1/p' | head -1)"
    if [[ -n "$ms" ]] && { [[ -z "$best" ]] || (( ms < best )); }; then best="$ms"; fi
  done
  echo "$best"
}

nps() { # nodes, ms -> integer nodes/sec
  awk "BEGIN{ if ($2>0) printf \"%d\", $1/($2/1000.0); else print 0 }"
}

main() {
  ensure_builds
  printf '%-11s %6s %12s %9s %9s %11s %11s %8s\n' \
    CASE DEPTH NODES CPP_MS RUST_MS CPP_NPS RUST_NPS SPEEDUP
  for entry in "${CASES[@]}"; do
    IFS='|' read -r label depth fen <<<"$entry"
    local line nodes
    line="$("$RUST_BIN" perft-bench "$depth" "$fen")"
    nodes="$(sed -nE 's/.*nodes=([0-9]+).*/\1/p' <<<"$line")"

    local rms cms cnps rnps speedup
    rms="$(rust_best_ms "$depth" "$fen")"
    cms="$(cpp_best_ms_for_nodes "$nodes")"
    cnps="$(nps "$nodes" "$cms")"
    rnps="$(nps "$nodes" "$rms")"
    speedup="$(awk "BEGIN{ if ($rms>0) printf \"%.2fx\", $cms/$rms; else print \"n/a\" }")"
    printf '%-11s %6s %12s %9s %9s %11s %11s %8s\n' \
      "$label" "$depth" "$nodes" "$cms" "$rms" "$cnps" "$rnps" "$speedup"
  done
  echo
  echo "NOTE: both engines skip eval/bookkeeping on the perft path, so this is a"
  echo "      reasonably fair move-generation + make/unmake comparison. See header."
}

main
