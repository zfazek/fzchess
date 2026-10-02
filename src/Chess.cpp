#include "Chess.h"

#include <algorithm>
#include <cstdio>
#include <cstring>
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
    table->eval->hash->reset_counters();
    table->eval->hash->clear();
    start_time = Util::get_ms();
    stop_time = start_time + max_time;
    depth = 1;
    stop_search = false;
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
            for (int i = 0; i < curr_seldepth - 1; i++) {
                --move_number;
            }
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
    table->eval->hash->printStatistics(nodes);
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
            u = table->eval->evaluation(legal_pointer, dpt);
            --move_number;
        } else { // Not last ply
            if (table->third_occurance() ||
                table->is_not_enough_material() ||
                movelist[move_number].not_pawn_move >= 100) {
                u = table->eval->DRAW;
                --move_number;
            } else {
                u = table->eval->evaluation_only_end_game(dpt);
                if (u == 32767) { // not end
                    invert_player_to_move();
                    u = -alfabeta(dpt + 1, -beta, -alfa);
                    last_ply = false;
                    invert_player_to_move();
                }
                --move_number;
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
                table->eval->hash->printStatistics(nodes);
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
        --move_number;
    }
    return nodes;
}

void Chess::sort_legal_moves(const int nbr_legal, const int dpt) {
    for (int i = 0; i < nbr_legal; i++) {
        const int move = legal_moves[i];
        table->update_table(move, false);
        const int u = table->eval->evaluation_material(dpt);
        sorted_legal_moves[i] = {move, u};
        --move_number;
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

// Returns the FEN string for the position stored in tablelist[movenumber] / movelist[movenumber].
// The side to move is inferred from movelist[movenumber].color (the side that just moved),
// so the side to move in the FEN is the opposite. At move_number==0 (start), color==0,
// so we use player_to_move instead for safety.
std::string Chess::get_fen(const int movenumber) const {
    // Piece characters indexed by piece value (0x00..0x06 white, 0x81..0x86 black)
    // WhitePawn=1,Knight=2,Bishop=3,Rook=4,Queen=5,King=6
    // BlackPawn=0x81, etc.
    static const char white_piece_char[7] = {'.', 'P', 'N', 'B', 'R', 'Q', 'K'};
    static const char black_piece_char[7] = {'.', 'p', 'n', 'b', 'r', 'q', 'k'};

    const int *board = tablelist[movenumber];
    const struct position_t &pos = movelist[movenumber];

    std::string fen;

    // 1. Piece placement: rank 8 (y=9) down to rank 1 (y=2), files a-h (x=1..8)
    for (int rank = 9; rank >= 2; --rank) {
        int empty_count = 0;
        for (int file = 1; file <= 8; ++file) {
            const int sq = rank * 10 + file;
            const int piece = board[sq];
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
