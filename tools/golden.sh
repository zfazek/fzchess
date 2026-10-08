#!/usr/bin/env bash
#
# Golden-output regression net for the FZChess engine.
#
# Captures deterministic engine outputs that must stay identical across the
# C++ back-pointer refactor AND the subsequent Rust port:
#   1. perft node counts (pure move-generation check)
#   2. bestmove + final info line for fixed `go depth N` searches (search+eval)
#
# Usage:
#   tools/golden.sh capture [ENGINE]   # write baseline to tools/golden.txt
#   tools/golden.sh check   [ENGINE]   # diff current engine output vs baseline
#
# ENGINE defaults to build/src/fzchess. Exit code is non-zero on mismatch so it
# can gate a refactor step in CI or a pre-commit check.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENGINE="${2:-$ROOT/build/src/fzchess}"
GOLDEN="$ROOT/tools/golden.txt"

# Fixed-depth searches are deterministic (no wall-clock time limit), so the
# bestmove and final info line are reproducible. These FENs exercise castling,
# en passant, promotion races, and tactical mates.
FENS=(
  "position startpos"
  "position fen r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1"
  "position fen 8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1"
  "position fen 1k1r4/pp1b1R2/3q2pp/4p3/2B5/4Q3/PPP2B2/2K5 b - - 0 1"
  "position fen 3r1k2/4npp1/1ppr3p/p6P/P2PPPP1/1NR5/5K2/2R5 w - - 0 1"
)
DEPTH=4

# Progress logging goes to stderr so it never contaminates the golden output
# (generate's stdout is redirected to the baseline file / diff). Each message is
# prefixed with seconds elapsed since the script started.
START_TS=$SECONDS
log() {
  printf '[golden +%3ds] %s\n' "$((SECONDS - START_TS))" "$*" >&2
}

generate() {
  local engine="$1"
  log "perft suite starting (this is the slow part, ~a few seconds)..."
  echo "### PERFT ###"
  # The built-in perft suite (arg 2) prints "depth: N nodes: M ..." lines.
  # Strip the volatile time/knps columns so only the node counts are compared.
  local perft_out
  perft_out="$("$engine" 2 2>/dev/null || true)"
  grep -E '^depth: ' <<<"$perft_out" | sed -E 's/ time:.*$//'
  log "perft suite done."

  echo "### BESTMOVE ###"
  local total=${#FENS[@]}
  local idx=0
  for fen in "${FENS[@]}"; do
    idx=$((idx + 1))
    # Short label for the log: strip the "position " prefix, cap the length.
    local label="${fen#position }"
    log "bestmove search ${idx}/${total} (depth ${DEPTH}): ${label:0:48}"
    echo "--- $fen (depth $DEPTH) ---"
    # Drive one fixed-depth search through the UCI interface, then quit.
    # Capture only the bestmove line (deterministic for a fixed depth); drop the
    # per-iteration info/time lines which vary run to run.
    local out bm
    out="$(printf 'uci\n%s\ngo depth %d\nquit\n' "$fen" "$DEPTH" | "$engine" 2>/dev/null || true)"
    bm="$(grep -E '^bestmove' <<<"$out" | tail -1 || true)"
    echo "${bm:-bestmove <NONE>}"
  done
  log "all bestmove searches done."
}

case "${1:-}" in
  capture)
    log "capturing baseline using engine: $ENGINE"
    generate "$ENGINE" > "$GOLDEN"
    log "baseline written to $GOLDEN"
    echo "Baseline written to $GOLDEN"
    cat "$GOLDEN"
    ;;
  check)
    if [[ ! -f "$GOLDEN" ]]; then
      echo "No baseline at $GOLDEN. Run 'tools/golden.sh capture' first." >&2
      exit 2
    fi
    log "checking engine against baseline: $ENGINE"
    tmp="$(mktemp)"
    generate "$ENGINE" > "$tmp"
    log "comparing output to baseline..."
    if diff -u "$GOLDEN" "$tmp"; then
      echo "OK: output matches golden baseline."
      rm -f "$tmp"
    else
      echo "MISMATCH: engine output differs from golden baseline." >&2
      rm -f "$tmp"
      exit 1
    fi
    ;;
  *)
    echo "Usage: $0 {capture|check} [engine-path]" >&2
    exit 2
    ;;
esac
