#pragma once

#include <memory>

#include "TranspositionTable.h"

class Chess;

class Eval {
  public:
    Eval(Chess *chess);

    int evaluation(const int e_legal_pointer, const int dpt);
    int evaluation_material(const int dpt);
    int evaluation_only_end_game(const int dpt);
    int sum_material(const int color);

    const int DRAW = 0;
    const int LOST = -22000;
    const int WON = 22000;

    const int end_game_threshold = 1200;
    const int king_castled = 40;
    const int cant_castle = -40;
    const int double_bishops = 50;
    const int double_pawn = -30;
    const int friendly_pawn = 20;
    const int pawn_advantage = 10;

    uint64_t tt_nodes = 0; // Number of TT cache hits
    std::unique_ptr<TranspositionTable> tt;

  private:
    Chess *chess;

    int random_window;

    // Values for evaluation
    // empty, pawn, knight, bishop, rook, queen, king
    const int figure_value[7] = {0, 100, 330, 330, 500, 900, 0};

    int evaluation_king(const int idx, const int field);
    int evaluation_pawn(const int idx, const int field, const int sm);
};
