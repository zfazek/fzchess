#pragma once

#include <cstdint>
#include <memory>
#include <string>

#include "Table.h"
#include "Uci.h"

#define QUIESCENCE_SEARCH
#define ALFABETA
#define PERFT

#define MAX_MOVES 1000
#define MAX_LEGAL_MOVES 128

// Parameters of the given position
struct position_t {
    int color;
    int move_from;
    int move_to;
    int captured_figure;
    int promotion;
    int castle;
    int not_pawn_move;
    int en_passant; // eq. e3
    int white_king_castled;
    int black_king_castled;
    int pos_white_king;
    int pos_black_king;
    int white_double_bishops;
    int black_double_bishops;
    int further;

#ifdef POS_FIGURE
    int pos_white_figure[16];
    int pos_black_figure[16];
#endif
};

struct move_t {
    int move;
    int value;

    bool operator<(const struct move_t& o) const {
        return value > o.value;
    }
};

class Chess {
  public:
    static constexpr int WHITE = 1;
    static constexpr int BLACK = -1;

    Chess();

    void start_game();
    void make_move();
    void calculate_evarray();
    void calculate_evarray_new();
    void invert_player_to_move();
    void processCommands(const char *input) const;
    uint64_t perft(const int dpt);
    std::string get_fen(const int movenumber) const;

public:
    std::unique_ptr<Table> table;
    std::unique_ptr<Uci> uci;

    // Array of moves. Each time the whole table is stored
    int tablelist[MAX_MOVES][120];

    // Array of legal moves
    int legal_moves[MAX_LEGAL_MOVES];
    struct move_t sorted_legal_moves[MAX_LEGAL_MOVES];

    int FZChess; // 1:white, -1:black
    int move_number;
    int player_to_move; // 1:white, -1:black

    uint64_t nodes;
    int max_time = 0;
    int movetime;
    int best_move;
    int depth, seldepth, curr_depth, curr_seldepth, gui_depth;
    int default_seldepth;
    bool break_if_mate_found;
    int sm;
    int legal_pointer;
    int nof_legal_root_moves;
    int mate_score;

    // Stores the parameters of the given position
    struct position_t movelist[MAX_MOVES];
    struct move_t root_moves[MAX_LEGAL_MOVES];

    bool stop_received = false;
    bool sort_alfarray = true;

#ifdef SORT_ALFARRAY

    // Array of sorted legal moves
    int eva_alfabeta_temp[MAX_LEGAL_MOVES];
#endif

  private:
    int alfabeta(int dpt, int alfa, int beta);
    void checkup();
    void sort_legal_moves(const int nbr_legal, const int dpt);

    // Array of best line
    int curr_line[MAX_LEGAL_MOVES];

    // Array of best move of iterative deepening
    int best_iterative[MAX_LEGAL_MOVES];
    bool last_ply;
    uint64_t start_time, stop_time;
    int stop_search;
};
