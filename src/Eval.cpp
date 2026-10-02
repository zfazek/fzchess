#include "Eval.h"

#include <cstdlib>
#include <iostream>

#include "Chess.h"

Eval::Eval(Chess *ch) : chess(ch) {
    tt = std::make_unique<TranspositionTable>();
    tt->resize(1 << 20); // 2^20 = ~1M entries, allocated once
}

// Evaluates material and king position, pawn structure, etc
int Eval::evaluation_material(const int dpt) {
    int c;
    int evaking;
    int e = 0;
    const int *pt = chess->board;
    const struct position_t *pm = chess->movelist + chess->move_number;

    // Hoist loop invariants out of the per-square scan
    const int ptm = chess->player_to_move;
    const int sm = chess->sm;
    const int move_number = chess->move_number;
    // Color bit of the side to move: WHITE pieces have bit 0 clear (0), BLACK set (128)
    const int my_color_bit = (ptm == chess->WHITE) ? 0 : 128;

    random_window = 10;

    // Material from the side-to-move's perspective, maintained incrementally.
    // material_wp is White's perspective; flip sign if Black is to move.
    e += (ptm == chess->WHITE) ? pm->material_wp : -pm->material_wp;

    // Pawn structure/advance terms: iterate only the pawns (incremental list),
    // not all 80 board squares.
    for (int k = 0; k < chess->n_pawns; ++k) {
        const int i = chess->pawn_sq[k];
        const int field = pt[i];
        const int c = ((field & 128) == my_color_bit) ? 1 : -1;
        e += c * evaluation_pawn(i, field, sm);
    }

    // King pawn-shield term: use the tracked king squares directly.
    if (move_number > 6) {
        const int wk = pm->pos_white_king;
        const int bk = pm->pos_black_king;
        const int wk_c = (my_color_bit == 0) ? 1 : -1;  // white king is "ours" iff we are white
        e += wk_c * evaluation_king(wk, pt[wk]);
        e += -wk_c * evaluation_king(bk, pt[bk]);
    }

    if (ptm == chess->WHITE) {
        c = 1;
    } else {
        c = -1;
    }

    // Bonus for castling
    e += c * (pm->white_king_castled - pm->black_king_castled);

    // Bonus for double bishops
    e += c * (pm->white_double_bishops - pm->black_double_bishops);

    // In the end game bonus if the enemy king is close
    if (sm < end_game_threshold) {
        random_window = 2;
        evaking = 3 * abs((pm->pos_white_king / 10) - (pm->pos_black_king / 10));
        evaking += 3 * abs((pm->pos_white_king % 10) - (pm->pos_black_king % 10));
        if ((dpt % 2) == 0) {
            e += evaking;
        } else {
            e -= evaking;
        }

        // force to the corner
        if ((ptm == chess->WHITE) && ((dpt % 2) == 1)) {
            evaking = 5 * (abs(5 - (pm->pos_black_king / 10)) +
                           abs(5 - (pm->pos_black_king % 10)));
        } else if ((ptm == chess->WHITE) && ((dpt % 2) == 0)) {
            evaking = -5 * (abs(5 - (pm->pos_white_king / 10)) +
                            abs(5 - (pm->pos_white_king % 10)));
        } else if ((ptm == chess->BLACK) && ((dpt % 2) == 0)) {
            evaking = -5 * (abs(5 - (pm->pos_black_king / 10)) +
                            abs(5 - (pm->pos_black_king % 10)));
        } else if ((ptm == chess->BLACK) && ((dpt % 2) == 1)) {
            evaking = 5 * (abs(5 - (pm->pos_white_king / 10)) +
                           abs(5 - (pm->pos_white_king % 10)));
        }
        e += evaking;
    } else {
#ifdef CASTLING_PUNISHMENT

        // Punishment if can not castle
        if (pm->white_king_castled == 0 && (pm->castle & 3) == 0) {
            e += c * cant_castle;
        }
        if (pm->black_king_castled == 0 && (pm->castle & 12) == 0) {
            e += c * cant_castle;
        }
#endif
    }

    return e;
}

// Evaluates material plus number of legal moves of both sides plus a random number
int Eval::evaluation(const int e_legal_pointer, const int dpt) {
    // Use the incrementally maintained Zobrist key — no board scan needed
    const uint64_t key = chess->movelist[chess->move_number].zobrist_key;

    const int cached = tt->probe(key);
    if (cached != TT_MISS) {
        ++tt_nodes;
        return cached;
    }

    // Not in the table — normal evaluation
    if (chess->table->third_occurance() ||
        chess->table->is_not_enough_material() ||
        chess->movelist[chess->move_number].not_pawn_move >= 100) {
        tt->store(key, static_cast<int16_t>(dpt), static_cast<int16_t>(DRAW), TT_EXACT,
                  chess->search_age);
        return chess->table->eval->DRAW;
    }
    chess->table->list_legal_moves();

    int lp = e_legal_pointer;
    chess->invert_player_to_move();
    chess->table->list_legal_moves();
    lp -= chess->legal_pointer;
    chess->invert_player_to_move();
    if (chess->legal_pointer == -1) { // No legal move
        if (!chess->table->is_attacked(
                chess->player_to_move == chess->WHITE
                    ? (chess->movelist + chess->move_number)->pos_black_king
                    : (chess->movelist + chess->move_number)->pos_white_king,
                -chess->player_to_move)) {
            return DRAW;
        } else {
            return WON - dpt;
        }
    }

    // Adds random to evaluation
    const int random_number = 0; // rand() % (random_window + 1);
    const int u = evaluation_material(dpt) + 2 * lp + random_number;

    tt->store(key, static_cast<int16_t>(dpt), static_cast<int16_t>(u), TT_EXACT,
              chess->search_age);
    return u;
}

int Eval::evaluation_only_end_game(const int dpt) {
    chess->invert_player_to_move();
    chess->table->list_legal_moves();
    chess->invert_player_to_move();
    if (chess->legal_pointer == -1) { // No legal move
        if (!chess->table->is_attacked(
                chess->player_to_move == chess->WHITE
                    ? (chess->movelist + chess->move_number)->pos_black_king
                    : (chess->movelist + chess->move_number)->pos_white_king,
                -chess->player_to_move)) {
            // Stalemate
            return DRAW;
        } else {
            // Mate
            return WON - dpt;
        }
    }
    return 32767; // not end
}

// Bonus for king's adjacent own pawns
int Eval::evaluation_king(const int idx, const int field) {
    int e = 0;
    const int *pt = chess->board;
    const int target = (field & 128) + chess->table->Pawn; // friendly pawn value
    static const int kdir[8] = {-11, -10, -9, -1, 1, 9, 10, 11};
    for (int k = 0; k < 8; ++k) {
        if (pt[idx + kdir[k]] == target) {
            e += friendly_pawn;
        }
    }
    return e;
}

int Eval::evaluation_pawn(const int idx, const int field, const int sm) {
    int e = 0;

    // punishing double pawns
    int dir = 0;
    int *pt = chess->board + idx;
    do {
        dir += 10;
        if (*(pt + dir) == field) {
            e += double_pawn;
        }
    } while (dir <= 20 && *(pt + dir) != OFFBOARD);
    dir = 0;
    do {
        dir -= 10;
        if (*(pt + dir) == field) {
            e += double_pawn;
        }
    } while (dir >= -20 && *(pt + dir) != OFFBOARD);

    // Bonus for pawn advantage
    if (sm < 2000) {
        if ((field & 128) == 0) {
            e += (idx / 10) * pawn_advantage;
        } else {
            e += (11 - (idx / 10)) * pawn_advantage;
        }
    }
    return e;
}

// Calculates material for evaluating end game threshold
int Eval::sum_material(const int color) {
    int e = 0;
    // int* pt = tablelist + move_number;
    for (int i = 20; i < 100; ++i) {
        const int field = chess->board[i];
        if (field > 0 && field < OFFBOARD) {
            const int figure_color = field & 128;
            if ((color == chess->WHITE && figure_color == 0) ||
                (color == chess->BLACK && figure_color == 128)) {
                e += figure_value[(field & 127)];
            }
        }
    }
    return e;
}
