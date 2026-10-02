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
    uint64_t zobrist_key;   // Incrementally maintained Zobrist hash
    int material_wp;        // Incremental material from White's perspective (white - black)
    int color;
    int move_from;
    int move_to;
    int captured_figure;
    int figure_moved;       // The piece that moved (needed for unmake)
    int ep_capture_sq;      // Square where captured pawn was removed (en passant), 0 if not ep
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
    uint64_t zobrist_key() const;          // full recompute (used for init/debug only)
    uint64_t compute_zobrist_key(int mn) const; // full recompute for a given move_number slot

public:
    std::unique_ptr<Table> table;
    std::unique_ptr<Uci> uci;

    // Single board (replaces tablelist[MAX_MOVES][120])
    int board[120];

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
    uint8_t search_age = 0; // Incremented each make_move() to invalidate TT entries from prior searches

    // Zobrist hashing tables — also used by Table::update_table() for incremental updates
    uint64_t zobrist_side_white;
    uint64_t zobrist_side_black;
    uint64_t zobrist_piece[2][7][120]; // [color 0=white/1=black][piece 1-6][square]
    uint64_t zobrist_enpassant[120];
    uint64_t zobrist_castle[16];

    // Incrementally-maintained list of pawn squares (both colors), updated in
    // Table::update_table / unmake_table. Lets evaluation iterate ~16 pawns
    // instead of scanning all 80 board squares. pawn_at[sq] gives the index of
    // square sq within pawn_sq[] (or -1), for O(1) removal.
    int pawn_sq[16];
    int pawn_at[120];
    int n_pawns;

    inline void pawn_add(int sq) {
        pawn_at[sq] = n_pawns;
        pawn_sq[n_pawns++] = sq;
    }
    inline void pawn_remove(int sq) {
        const int idx = pawn_at[sq];
        const int last = --n_pawns;
        const int moved = pawn_sq[last];
        pawn_sq[idx] = moved;
        pawn_at[moved] = idx;
        pawn_at[sq] = -1;
    }
    inline void pawn_move(int from, int to) {
        const int idx = pawn_at[from];
        pawn_sq[idx] = to;
        pawn_at[to] = idx;
        pawn_at[from] = -1;
    }
    void rebuild_pawn_list();

#ifdef SORT_ALFARRAY

    // Array of sorted legal moves
    int eva_alfabeta_temp[MAX_LEGAL_MOVES];
#endif

  private:
    int alfabeta(int dpt, int alfa, int beta);
    void checkup();
    void sort_legal_moves(const int nbr_legal, const int dpt);

    // Killer moves: two quiet moves per ply that caused a beta cutoff in a sibling.
    // Trying them early yields more cutoffs. Indexed by search ply (dpt).
    static constexpr int MAX_PLY = 128;
    int killer_moves[MAX_PLY][2];

    // Array of best line
    int curr_line[MAX_LEGAL_MOVES];

    // Array of best move of iterative deepening
    int best_iterative[MAX_LEGAL_MOVES];
    bool last_ply;
    uint64_t start_time, stop_time;
    int stop_search;

    uint64_t zobrist_rand() const;
    void init_zobrist();
};
