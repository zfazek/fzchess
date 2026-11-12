#!/usr/bin/env python3
"""Estimate fzchess strength by playing Stockfish with UCI_Elo adapted after each game.

Win  -> Stockfish Elo goes up, loss -> goes down, draw -> unchanged.
The step starts large and shrinks on every direction change (down to --min-step).

Requires: pip install chess==1.11.2   (e.g. in a venv)

Examples:
    # phase 1: adaptive, find the approximate level
    python tools/elo_match.py --games 40 --start-elo 1800

    # phase 2: fixed Stockfish Elo, report fzchess Elo with 95% confidence interval
    python tools/elo_match.py --games 300 --fixed-elo 1700
"""
import argparse
import datetime
import math
import random
import statistics
import sys

import chess
import chess.engine
import chess.pgn

GREEN, RED = "\033[32m", "\033[31m"
SF_MIN, SF_MAX = 1320, 3190  # UCI_Elo range of Stockfish 17.1


def parse_args():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--engine", default="rust/target/release/fzchess", help="path to fzchess binary")
    p.add_argument("--stockfish", default="/usr/games/stockfish")
    p.add_argument("--games", type=int, default=20, help="N games")
    p.add_argument("--movetime", type=int, default=1000, help="ms per move for both engines")
    p.add_argument("--start-elo", type=int, default=1800)
    p.add_argument("--fixed-elo", type=int, default=None,
                   help="phase 2: keep Stockfish at this Elo for all games and report Elo with 95%% CI")
    p.add_argument("--step", type=int, default=200, help="initial Elo step")
    p.add_argument("--min-step", type=int, default=25)
    p.add_argument("--random-plies", type=int, default=4,
                   help="random opening plies; each opening is played with both colors")
    p.add_argument("--max-plies", type=int, default=400, help="draw adjudication")
    p.add_argument("--use-book", action="store_true", help="let fzchess use its opening book (OwnBook)")
    p.add_argument("--seed", type=int, default=None)
    p.add_argument("--log", default=None, help="log file (default elo_match_<timestamp>.log)")
    p.add_argument("--pgn", default=None, help="PGN file (default elo_match_<timestamp>.pgn)")
    return p.parse_args()


def elo_from_score(p):
    p = min(max(p, 1e-6), 1 - 1e-6)
    return -400 * math.log10(1 / p - 1)


def elo_estimate(w, d, l, opp_elo):
    """Return (elo, lo, hi) with a normal-approximation 95% CI on the score."""
    n = w + d + l
    if n == 0:
        return None
    mu = (w + 0.5 * d) / n
    var = (w * (1 - mu) ** 2 + d * (0.5 - mu) ** 2 + l * (0 - mu) ** 2) / n
    se = math.sqrt(var / n)
    return (opp_elo + elo_from_score(mu),
            opp_elo + elo_from_score(mu - 1.96 * se),
            opp_elo + elo_from_score(mu + 1.96 * se))


def clamp(elo):
    return max(SF_MIN, min(SF_MAX, elo))


def random_opening(rng, plies):
    while True:
        b = chess.Board()
        for _ in range(plies):
            if b.is_game_over():
                break
            b.push(rng.choice(list(b.legal_moves)))
        if not b.is_game_over():
            return [m for m in b.move_stack]


def play_game(fz, sf, opening, fz_white, movetime, max_plies):
    """Returns (result_str, fz_score, termination, board, moves_san)."""
    board = chess.Board()
    for m in opening:
        board.push(m)
    limit = chess.engine.Limit(time=movetime / 1000)
    fz_color = chess.WHITE if fz_white else chess.BLACK
    while True:
        if board.is_checkmate():
            winner_fz = board.turn != fz_color
            return ("1-0" if board.turn == chess.BLACK else "0-1"), (1.0 if winner_fz else 0.0), "checkmate", board
        draw_reason = None
        if board.is_stalemate():
            draw_reason = "stalemate"
        elif board.is_insufficient_material():
            draw_reason = "insufficient material"
        elif board.is_fivefold_repetition() or board.can_claim_threefold_repetition():
            draw_reason = "repetition"
        elif board.is_seventyfive_moves() or board.can_claim_fifty_moves():
            draw_reason = "50-move rule"
        if draw_reason:
            return "1/2-1/2", 0.5, f"draw ({draw_reason})", board
        if len(board.move_stack) >= max_plies:
            return "1/2-1/2", 0.5, "draw (max plies)", board
        eng = fz if board.turn == fz_color else sf
        try:
            res = eng.play(board, limit)
            move = res.move
            if move is None or move not in board.legal_moves:
                raise chess.engine.EngineError(f"illegal/no move {move}")
        except (chess.engine.EngineError, chess.engine.EngineTerminatedError, ValueError) as e:
            loser_is_fz = eng is fz
            white_wins = (board.turn == chess.BLACK)
            return ("1-0" if white_wins else "0-1"), (0.0 if loser_is_fz else 1.0), f"engine failure: {e}", board
        board.push(move)


def main():
    a = parse_args()
    ts = datetime.datetime.now().strftime("%Y%m%d_%H%M%S")
    log_path = a.log or f"elo_match_{ts}.log"
    pgn_path = a.pgn or f"elo_match_{ts}.pgn"
    rng = random.Random(a.seed)

    log_f = open(log_path, "w", buffering=1)

    use_color = sys.stdout.isatty()

    def log(msg, color=None):
        # colors only on the terminal; the log file stays plain text
        if color and use_color:
            print(f"{color}{msg}\033[0m")
        else:
            print(msg)
        log_f.write(msg + "\n")

    fz = chess.engine.SimpleEngine.popen_uci(a.engine)
    sf = chess.engine.SimpleEngine.popen_uci(a.stockfish)
    try:
        if "OwnBook" in fz.options:
            fz.configure({"OwnBook": a.use_book})
        sf.configure({"UCI_LimitStrength": True})

        fixed = a.fixed_elo is not None
        elo = clamp(a.fixed_elo if fixed else a.start_elo)
        step = a.step
        last_dir = 0
        opening = None
        history = []  # (sf_elo, score)
        w = d = l = 0
        log(f"# fzchess vs Stockfish (UCI_Elo), movetime={a.movetime}ms, games={a.games}, "
            f"{'fixed' if fixed else 'start'}_elo={elo}, book={'on' if a.use_book else 'off'}")
        log(f"{'game':>4} {'fz_color':<8} {'sf_elo':>6} {'result':<7} {'fz_score':>8} {'total':>9} {'plies':>5}  termination")

        with open(pgn_path, "w") as pgn_f:
            for g in range(1, a.games + 1):
                fz_white = (g % 2 == 1)
                if g % 2 == 1:  # new opening for each pair of games
                    opening = random_opening(rng, a.random_plies)
                sf.configure({"UCI_Elo": elo})
                result, score, term, board = play_game(fz, sf, opening, fz_white, a.movetime, a.max_plies)

                game = chess.pgn.Game.from_board(board)
                game.headers.update({
                    "Event": "fzchess Elo estimate", "Round": str(g),
                    "White": "fzchess" if fz_white else f"Stockfish UCI_Elo={elo}",
                    "Black": f"Stockfish UCI_Elo={elo}" if fz_white else "fzchess",
                    "Result": result, "Termination": term, "TimeControl": f"{a.movetime / 1000}/move",
                })
                print(game, file=pgn_f, end="\n\n")
                pgn_f.flush()

                history.append((elo, score))
                w += score == 1.0
                d += score == 0.5
                l += score == 0.0
                total = f"{w + 0.5 * d:g}/{g}"

                log(f"{g:>4} {'white' if fz_white else 'black':<8} {elo:>6} {result:<7} "
                    f"{score:>8} {total:>9} {len(board.move_stack):>5}  {term}",
                    GREEN if score == 1.0 else RED if score == 0.0 else None)

                if fixed and (g % 10 == 0):
                    e, lo, hi = elo_estimate(w, d, l, elo)
                    log(f"     running: +{w:.0f} ={d:.0f} -{l:.0f}  Elo {e:.0f} [{lo:.0f}, {hi:.0f}]")

                direction = 1 if score == 1.0 else -1 if score == 0.0 else 0
                if direction and not fixed:
                    if last_dir and direction != last_dir:
                        step = max(a.min_step, step // 2)
                    last_dir = direction
                    elo = clamp(elo + direction * step)

        # Estimate: average opponent Elo over the second half (converged part),
        # plus the classic performance rating over all games.
        half = history[len(history) // 2:] or history
        est_tail = statistics.mean(e for e, _ in half)
        avg_opp = statistics.mean(e for e, _ in history)
        pct = sum(s for _, s in history) / len(history)
        perf = None
        if 0 < pct < 1:
            perf = avg_opp + 400 * math.log10(pct / (1 - pct))
        log("")
        log(f"# Result: +{w} ={d} -{l}  score {pct * 100:.1f}%")
        log(f"# Estimated fzchess Elo (avg SF Elo, 2nd half of games): {est_tail:.0f}")
        if perf is not None:
            log(f"# Performance rating (all games): {perf:.0f}")
        if fixed:
            e, lo, hi = elo_estimate(w, d, l, elo)
            log(f"# Elo vs fixed SF {elo}: {e:.0f}  (95% CI {lo:.0f} .. {hi:.0f})")
        else:
            log(f"# Next Stockfish Elo would be {elo}")
        log(f"# NOTE: SF UCI_Elo is calibrated for its own TC/hardware (CCRL-ish, 120s+1s); treat as relative.")
        log(f"# Files: {log_path}, {pgn_path}")
    finally:
        for e in (fz, sf):
            try:
                e.quit()
            except Exception:
                pass
        log_f.close()


if __name__ == "__main__":
    sys.exit(main())
