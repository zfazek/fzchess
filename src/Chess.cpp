#include "Chess.h"

#include <algorithm>
#include <cstdio>
#include <cstring>
#include <ctime>
#include <iostream>
#include <string>

#include "Eval.h"
#include "Util.h"

using std::string;

struct best_lines {
    int length;
    int value;
    int moves[1000];
} best_line[99];

Chess::Chess() {
    uci = std::make_unique<Uci>(this);
    table = std::make_unique<Table>(this);
    init_zobrist();
}

void Chess::init_zobrist() {
    srand(static_cast<unsigned int>(time(nullptr)));
    // WHITE: i=0, BLACK: i=1; piece types 1..6
    for (int i = 0; i < 2; ++i) {
        for (int j = 1; j < 7; ++j) {
            for (int k = 0; k < 120; ++k) {
                zobrist_piece[i][j][k] = zobrist_rand();
            }
        }
    }
    zobrist_side_white = zobrist_rand();
    zobrist_side_black = zobrist_rand();
    for (int i = 0; i < 120; ++i) {
        zobrist_enpassant[i] = zobrist_rand();
    }
    for (int i = 0; i < 16; ++i) {
        zobrist_castle[i] = zobrist_rand();
    }
}

uint64_t Chess::zobrist_rand() const {
    uint64_t r = 0;
    for (int i = 0; i < 4; ++i) {
        r ^= static_cast<uint64_t>(rand()) << (i * 15);
    }
    return r;
}

uint64_t Chess::zobrist_key() const {
    return compute_zobrist_key(move_number);
}

uint64_t Chess::compute_zobrist_key(int mn) const {
    uint64_t key = 0;
    // Side to move: infer from movelist[mn].color (the side that just moved)
    // At mn==0 use player_to_move directly; otherwise the side to move is opposite of who just moved
    int side;
    if (mn == 0) {
        side = player_to_move;
    } else {
        side = -movelist[mn].color;
    }
    key ^= (side == WHITE) ? zobrist_side_white : zobrist_side_black;

    const int *brd = board;
    for (int k = 20; k < 100; ++k) {
        const int field = brd[k];
        if (field > EMPTY && field < OFFBOARD) {
            const int figure = field & 127;
            const int color_idx = (field & 128) >> 7;
            key ^= zobrist_piece[color_idx][figure][k];
        }
    }
    key ^= zobrist_enpassant[movelist[mn].en_passant];
    key ^= zobrist_castle[movelist[mn].castle & 15];
    return key;
}

// Rebuilds the pawn square list from the current board. Called after any full
// board setup (reset_movelist, setboard); update_table/unmake_table keep it in
// sync incrementally afterwards.
void Chess::rebuild_pawn_list() {
    n_pawns = 0;
    for (int i = 0; i < 120; ++i) {
        pawn_at[i] = -1;
    }
    for (int sq = 20; sq < 100; ++sq) {
        const int f = board[sq];
        if ((f & 127) == 1 && f != 0xff) { // pawn (white 0x01 or black 0x81)
            pawn_add(sq);
        }
    }
}

void Chess::start_game() { // new
    table->reset_movelist();
    player_to_move = WHITE;
    default_seldepth = 0;
    break_if_mate_found = true;
    // table->print_table();
}

// Inverts who the next player is
void Chess::invert_player_to_move() {
    player_to_move = -player_to_move;
}

void Chess::make_move() {
    const int MAX = 22767;
    uint64_t time_elapsed;
    uint64_t time_current_depth_start, time_current_depth_stop, time_remaining;
    nodes = 0;
    table->eval->tt_nodes = 0;
    ++search_age; // invalidates TT entries from the previous search
    for (int p = 0; p < MAX_PLY; ++p) {
        killer_moves[p][0] = 0;
        killer_moves[p][1] = 0;
    }
    start_time = Util::get_ms();
    stop_time = start_time + max_time;
    depth = 1;
    stop_search = false;
    const int root_move_number = move_number; // restore point after a search interruption
    const int root_player_to_move = player_to_move;

    // Ensure best_move is always a legal move for the CURRENT position before the
    // search starts. Otherwise, if the time limit expires before depth 1 assigns
    // best_move, make_move would play a stale move from a previous search, corrupt
    // the board, and crash on the next search.
    table->list_legal_moves();
    if (legal_pointer >= 0) {
        best_move = legal_moves[0];
    }
    const string fen = get_fen(move_number).c_str();
    printf("FEN: %s\n", get_fen(move_number).c_str());
    Util::flush();
    for (;;) {
        // Calculate summa of material for end game threshold
        // sm = table->eval->sum_material(player_to_move);
        //        if (depth > 4) {
        //            seldepth = depth + 4;
        //        } else {
        seldepth = depth + default_seldepth;
        //        printf("%d %d\n", depth, seldepth);
        Util::flush();

        // Calculates the time of the move
        // Searches the best move with negamax algorithm with alfa-beta cut off
        time_current_depth_start = Util::get_ms();
        printf("info depth %d\n", depth);
        Util::flush();
        try {
            alfabeta(1, -MAX, MAX);
        } catch (...) {

        }
        if (stop_search) {
            // The time-limit exception unwinds the recursion without undoing the
            // moves made along the current line. With a single shared board, we must
            // restore it to the root position by unmaking every pending move.
            while (move_number > root_move_number) {
                table->unmake_table();
            }
            // Restore side to move to the root side (invert_player_to_move pairs in
            // the recursion are not balanced after an exception).
            player_to_move = root_player_to_move;
        }
        time_current_depth_stop = Util::get_ms();
        time_remaining = stop_time - time_current_depth_stop;

        // If there is no time for another depth search
        if (movetime == 0 && max_time != 0) {
            if ((time_current_depth_stop - time_current_depth_start) * 5 > time_remaining) {
                stop_search = true;
            }
        }
        if (depth > 30) {
            stop_search = true;
        }
        time_elapsed = Util::get_ms() - start_time;
        printf("info depth %d seldepth %d time %lu nodes %lu nps %lu\n",
               depth,
               seldepth,
               time_elapsed,
               nodes,
               (time_elapsed == 0) ? 0 : (uint64_t)((1000.0 * nodes / time_elapsed)));
        Util::flush();
        // Human-readable time-to-depth: time to complete this iteration and the
        // cumulative time to reach this depth. This reflects search efficiency far
        // better than raw NPS (good move ordering lowers nodes, which lowers NPS but
        // shortens time-to-depth).
        {
            const uint64_t this_depth_ms = time_current_depth_stop - time_current_depth_start;
            printf("time-to-depth: depth %d | this iter %lu ms | cumulative %lu ms\n",
                   depth, this_depth_ms, time_elapsed);
            Util::flush();
        }
        // calculate_evarray_new();
        // for (int i = 0; i < nof_legal_root_moves; i++) {
        //     printf("(%s:%d) ", Util::move2str(root_moves[i].move),
        //            root_moves[i].value);
        //     if (i % 8 == 7) {
        //         puts("");
        //     }
        // }
        // puts("");
        // Prints statistics
        // printf("alfabeta: %d\n", a);Util::flush();
        // printf("best %s\n", best_move);Util::flush();
        best_iterative[depth] = best_move;
        if (depth == 1 && nof_legal_root_moves == 1) {
            break;
        }
        if (stop_search) {
            break;
        }
        if (mate_score > 20000 && break_if_mate_found) {
            break;
        }
        if (depth == gui_depth) {
            break;
        }
        // if (legal_pointer == 0) break; //there is only one legal move
        ++depth;
        // if (depth == 1) depth = 5;
    }
    printf("\nbestmove %s\n", Util::move2str(best_move));
    Util::flush();
    if (nodes > 0) {
        std::cout << "TT hits " << table->eval->tt_nodes
                  << ", TT/nodes: " << (table->eval->tt_nodes * 100 / nodes) << "%" << std::endl;
    }
    // Update the table without printing it
    table->update_table(best_move, false);
    invert_player_to_move();
}

int Chess::alfabeta(int dpt, int alfa, int beta) {
    int u;
    int uu; // Evaluation if checkmate is found
    int alfarray[MAX_LEGAL_MOVES];
    int value = -22767;

    table->list_legal_moves();
    if (legal_pointer == -1) {
        if (!table->is_attacked(player_to_move == WHITE
                                    ? (movelist + move_number)->pos_white_king
                                    : (movelist + move_number)->pos_black_king,
                                player_to_move)) {
            // printf("DRAW: ");Util::flush();
            return table->eval->DRAW;
        } else {
            // printf("LOST: ");Util::flush();
            return table->eval->LOST;
        }
    }
    const int nbr_legal = legal_pointer + 1;

    // Sorts legal moves
    if (dpt == 1) {
        nof_legal_root_moves = legal_pointer + 1;
        calculate_evarray();
        for (int i = 0; i < nof_legal_root_moves; ++i) {
            alfarray[i] = root_moves[i].move;
        }
    } else {
        if (sort_alfarray) {
            sort_legal_moves(nbr_legal, dpt);
        }
        for (int i = 0; i < nbr_legal; ++i) {
            alfarray[i] = legal_moves[i];
        }
        // Killer-move ordering: pull the two killer moves for this ply toward the
        // front (just after any already-prioritised moves) so they are tried early.
        if (dpt < MAX_PLY) {
            int front = 0;
            for (int kslot = 0; kslot < 2; ++kslot) {
                const int km = killer_moves[dpt][kslot];
                if (km == 0) continue;
                for (int i = front; i < nbr_legal; ++i) {
                    if (alfarray[i] == km) {
                        if (i != front) {
                            const int tmp = alfarray[front];
                            alfarray[front] = alfarray[i];
                            alfarray[i] = tmp;
                        }
                        ++front;
                        break;
                    }
                }
            }
        }
    }

    // main loop, checking all the legal moves
    for (int i = 0; i < nbr_legal; ++i) {
        ++nodes;
        if ((nodes & 1023) == 0) {
            checkup();
        }
        curr_depth = std::max(dpt, depth);
        curr_seldepth = seldepth;
        if (dpt == 1) {
            printf("info currmove %s currmovenumber %d\n", Util::move2str(alfarray[i]), i + 1);
            Util::flush();
        }
        table->update_table(alfarray[i], false);
        curr_line[dpt] = alfarray[i];

        // If last ply->evaluating
        if ((dpt >= depth && movelist[move_number].further == 0) || dpt >= seldepth) {
            last_ply = true;
            u = table->eval->evaluation(*this, legal_pointer, dpt);
            table->unmake_table();
        } else { // Not last ply
            if (table->third_occurance() ||
                table->is_not_enough_material() ||
                movelist[move_number].not_pawn_move >= 100) {
                u = table->eval->DRAW;
                table->unmake_table();
            } else {
                u = table->eval->evaluation_only_end_game(*this, dpt);
                if (u == 32767) { // not end
                    invert_player_to_move();
                    u = -alfabeta(dpt + 1, -beta, -alfa);
                    last_ply = false;
                    invert_player_to_move();
                }
                table->unmake_table();
            }
        }
        if (dpt == 1) {
            root_moves[i].move = alfarray[i];
            root_moves[i].value = u;
        }

        // Better move is found
        if (u > value) {
            // printf("dpt: %d, %d > %d\n", dpt, u, value);Util::flush();
            value = u;
            for (int b = 1; b <= curr_seldepth; b++) {
                best_line[dpt].moves[b] = curr_line[b];
            }
            best_line[dpt].value = u;
            if (last_ply) {
                best_line[dpt].length = dpt;
            }
            if (!last_ply) {
                best_line[dpt].length = best_line[dpt + 1].length;
                best_line[dpt].value = best_line[dpt + 1].value;
                for (int b = 1; b <= best_line[dpt + 1].length; b++) {
                    best_line[dpt].moves[b] = best_line[dpt + 1].moves[b];
                }
                // printf("best_line[%d].length: %d\n", dpt, best_line[dpt].length);Util::flush();
            }
            if (dpt == 1) {
                best_move = alfarray[i];
                // (TT statistics printed at end of make_move)
                uint64_t time_elapsed = Util::get_ms() - start_time;
                // If checkmate is found
                if (abs(u) > 20000) {
                    if (u > 0) {
                        uu = (22001 - u) / 2;
                    } else {
                        uu = -(u + 22001) / 2;
                    }
                    if (uu == 0 && u > 0) {
                        uu = 1;
                    }
                    if (uu == 0 && u < 0) {
                        uu = -1;
                    }
                    printf("info multipv 1 depth %d seldepth %d time %lu score mate %d nodes %lu pv ",
                           curr_depth, curr_seldepth, time_elapsed, uu, nodes);
                    for (int b = 1; b <= best_line[dpt].length; ++b) {
                        printf("%s ", Util::move2str(best_line[dpt].moves[b]));
                    }
                    printf("\n");
                    Util::flush();
                    mate_score = abs(u);
                } else {
                    printf("info multipv 1 depth %d seldepth %d time %lu score cp %d nodes %lu pv ",
                           curr_depth, curr_seldepth, time_elapsed, u, nodes);
                    for (int b = 1; b <= best_line[dpt].length; ++b) {
                        printf("%s ", Util::move2str(best_line[dpt].moves[b]));
                    }
                    printf("\n");
                    Util::flush();
                    mate_score = 0;
                }
            }
        }
#ifdef ALFABETA

        // Alfa Beta cut-off
        if (value >= beta) {
            // Record a killer move: a quiet (non-capturing) move that caused a cutoff.
            // The move was already unmade, so the board holds the pre-move position;
            // a capture has an enemy piece on the destination square.
            const int km = alfarray[i];
            const int x_to = (km & 0x00e0) >> 5;
            const int y_to = (km & 0x001c) >> 2;
            const int sq_to = 1 + x_to + (y_to + 2) * 10;
            const bool is_capture = (board[sq_to] != 0 /*EMPTY*/ && board[sq_to] != 0xff /*OFFBOARD*/);
            if (!is_capture && dpt < MAX_PLY && killer_moves[dpt][0] != km) {
                killer_moves[dpt][1] = killer_moves[dpt][0];
                killer_moves[dpt][0] = km;
            }
            return value;
        }
        if (value > alfa) {
            alfa = value;
        }
#endif
    } // for
    return value;
}

uint64_t Chess::perft(const int dpt) {
    int alfarray[MAX_LEGAL_MOVES];
    uint64_t nodes = 0;

    if (dpt == 0) {
        return 1;
    }

    table->list_legal_moves();
    const int nbr_legal = legal_pointer + 1;
    for (int i = 0; i < nbr_legal; ++i) {
        alfarray[i] = legal_moves[i];
    }
    for (int i = 0; i < nbr_legal; ++i) {
        if ((nodes & 1023) == 0) {
            checkup();
        }
        table->update_table(alfarray[i], false);
        invert_player_to_move();
        nodes += perft(dpt - 1);
        invert_player_to_move();
        table->unmake_table();
    }
    return nodes;
}

void Chess::sort_legal_moves(const int nbr_legal, const int dpt) {
    (void)dpt; // no longer needed: MVV-LVA is a static ordering (no search/eval)
    // MVV-LVA (Most Valuable Victim - Least Valuable Attacker) ordering.
    // Captures are scored by the captured piece's value (prefer taking big pieces)
    // minus a fraction of the capturing piece's value (prefer using small pieces),
    // and ranked ahead of all quiet moves. Promotions add the promoted piece value.
    // This is computed statically from the board — no make/unmake, no evaluation.
    for (int i = 0; i < nbr_legal; i++) {
        const int move = legal_moves[i];
        const int x_from = (move & 0xe000) >> 13;
        const int y_from = (move & 0x1c00) >> 10;
        const int x_to   = (move & 0x00e0) >> 5;
        const int y_to   = (move & 0x001c) >> 2;
        const int sq_from = 1 + x_from + (y_from + 2) * 10;
        const int sq_to   = 1 + x_to + (y_to + 2) * 10;

        const int victim   = board[sq_to] & 127;    // 0 if empty (quiet move)
        const int attacker = board[sq_from] & 127;

        int score = 0;
        if (victim != 0) {
            // Capture: big victim, small attacker ranks highest. +100000 keeps all
            // captures ahead of all quiet moves.
            score = 100000 + Table::piece_value[victim] * 16 - Table::piece_value[attacker];
        }
        // Promotion bonus (move encodes promotion in bits 0x0303).
        if ((move & 0x0303) != 0) {
            int promo_val = Table::piece_value[5]; // default queen
            if ((move & 0x0100) == 0x0100) promo_val = Table::piece_value[4]; // rook
            else if ((move & 0x0002) == 0x0002) promo_val = Table::piece_value[3]; // bishop
            else if ((move & 0x0001) == 0x0001) promo_val = Table::piece_value[2]; // knight
            score += 100000 + promo_val;
        }
        sorted_legal_moves[i] = {move, score};
    }
    std::sort(sorted_legal_moves, sorted_legal_moves + nbr_legal);
    for (int i = 0; i < nbr_legal; i++) {
        legal_moves[i] = sorted_legal_moves[i].move;
    }
}

// Sorts legal moves
// Bubble sort
void Chess::calculate_evarray() {
    if (depth == 1) {
        for (int i = 0; i < nof_legal_root_moves; i++) {
            root_moves[i].move = legal_moves[i];
            root_moves[i].value = 0;
        }
    } else {
        for (int i = 0; i < nof_legal_root_moves; i++) {
            for (int j = i + 1; j < nof_legal_root_moves; j++) {
                if (root_moves[j].value > root_moves[i].value) {
                    const int tempmove = root_moves[j].move;
                    root_moves[j].move = root_moves[i].move;
                    root_moves[i].move = tempmove;
                    const int v = root_moves[j].value;
                    root_moves[j].value = root_moves[i].value;
                    root_moves[i].value = v;
                }
            }
        }
    }
}

void Chess::calculate_evarray_new() {
    for (int i = 0; i < nof_legal_root_moves; i++) {
        for (int j = i + 1; j < nof_legal_root_moves; j++) {
            if (root_moves[j].value > root_moves[i].value) {
                const int tempmove = root_moves[j].move;
                root_moves[j].move = root_moves[i].move;
                root_moves[i].move = tempmove;
                const int v = root_moves[j].value;
                root_moves[j].value = root_moves[i].value;
                root_moves[i].value = v;
            }
        }
    }
}

/*
// Sorts legal moves
// Insertion sort
void Chess::calculate_evarray_new() {
    for (int i = 1; i < nof_legal_root_moves; i++) {
        for (int j = i;
                j > 0 && root_moves[j].value > root_moves[j-1].value;
                j--) {
            const int tempmove = root_moves[j].move;
            root_moves[j].move = root_moves[i].move;
            root_moves[i].move = tempmove;
            const int v = root_moves[j].value;
            root_moves[j].value = root_moves[i].value;
            root_moves[i].value = v;
        }
    }
}
*/

std::string Chess::get_fen(const int movenumber) const {
    // Piece characters indexed by piece value (0x00..0x06 white, 0x81..0x86 black)
    // WhitePawn=1,Knight=2,Bishop=3,Rook=4,Queen=5,King=6
    // BlackPawn=0x81, etc.
    static const char white_piece_char[7] = {'.', 'P', 'N', 'B', 'R', 'Q', 'K'};
    static const char black_piece_char[7] = {'.', 'p', 'n', 'b', 'r', 'q', 'k'};

    // Only current position is available (no historical board copies)
    const int *brd = board;
    const struct position_t &pos = movelist[movenumber];

    std::string fen;

    // 1. Piece placement: rank 8 (y=9) down to rank 1 (y=2), files a-h (x=1..8)
    for (int rank = 9; rank >= 2; --rank) {
        int empty_count = 0;
        for (int file = 1; file <= 8; ++file) {
            const int sq = rank * 10 + file;
            const int piece = brd[sq];
            if (piece == EMPTY) {
                ++empty_count;
            } else {
                if (empty_count > 0) {
                    fen += ('0' + empty_count);
                    empty_count = 0;
                }
                const int piece_type = piece & 0x7f; // strip color bit
                const bool is_black = (piece & 0x80) != 0;
                if (piece_type >= 1 && piece_type <= 6) {
                    fen += is_black ? black_piece_char[piece_type]
                                   : white_piece_char[piece_type];
                }
            }
        }
        if (empty_count > 0) {
            fen += ('0' + empty_count);
        }
        if (rank > 2) {
            fen += '/';
        }
    }

    // 2. Active color
    // pos.color is the side that just moved to reach this position.
    // If movenumber == 0, it's the starting position; use player_to_move.
    bool white_to_move;
    if (movenumber == 0) {
        white_to_move = (player_to_move == WHITE);
    } else {
        // The side that just moved is pos.color; the side to move next is opposite
        white_to_move = (pos.color == BLACK);
    }
    fen += white_to_move ? " w " : " b ";

    // 3. Castling availability
    // castle bits: 1=WhiteShort(K), 2=WhiteLong(Q), 4=BlackShort(k), 8=BlackLong(q)
    std::string castling;
    if (pos.castle & 1) castling += 'K';
    if (pos.castle & 2) castling += 'Q';
    if (pos.castle & 4) castling += 'k';
    if (pos.castle & 8) castling += 'q';
    fen += castling.empty() ? "-" : castling;

    // 4. En passant target square
    // en_passant is stored as board index (rank*10 + file), 0 means none
    fen += ' ';
    if (pos.en_passant > 0) {
        const int ep_file = pos.en_passant % 10; // 1..8
        const int ep_rank = pos.en_passant / 10; // 2..9
        fen += static_cast<char>('a' + ep_file - 1);
        fen += static_cast<char>('1' + ep_rank - 2);
    } else {
        fen += '-';
    }

    // 5. Halfmove clock (50-move rule counter)
    fen += ' ';
    fen += std::to_string(pos.not_pawn_move);

    // 6. Fullmove number (1-based; increments after Black's move)
    // movenumber 0 and 1 are both move 1; after that: (movenumber / 2) + 1
    const int fullmove = (movenumber <= 1) ? 1 : (movenumber / 2) + 1;
    fen += ' ';
    fen += std::to_string(fullmove);

    return fen;
}

void Chess::checkup() {
    if ((max_time != 0 && Util::get_ms() >= stop_time) || stop_received) {
        stop_search = true;
        throw 1;
    }
}

void Chess::processCommands(const char *input) const {
    if (strstr(input, "uci") || strstr(input, "isready")) {
        uci->processCommands(input);
    }
}
