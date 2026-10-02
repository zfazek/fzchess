#include "Table.h"

#include <cstring>
#include <cstdlib>

#include "Chess.h"
#include "Eval.h"
#include "Util.h"

Table::Table(Chess *ch) : chess(ch) {
    eval = std::make_unique<Eval>(chess);
}

// Resets the parameters and the table
void Table::reset_movelist() {
    // Startup position
    const int new_table[120] = {
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0x04, 0x02, 0x03, 0x05, 0x06, 0x03, 0x02, 0x04, 0xff, // white - first row
        0xff, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0xff,
        0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff,
        0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff,
        0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff,
        0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff,
        0xff, 0x81, 0x81, 0x81, 0x81, 0x81, 0x81, 0x81, 0x81, 0xff, // black - seventh row
        0xff, 0x84, 0x82, 0x83, 0x85, 0x86, 0x83, 0x82, 0x84, 0xff,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff};

    for (int n = 0; n < MAX_MOVES; n++) {
        chess->movelist[n].color = 0;
        chess->movelist[n].move_from = 0;
        chess->movelist[n].move_to = 0;
        chess->movelist[n].captured_figure = 0;
        chess->movelist[n].promotion = 0;

        // Both color, KQside castle possible
        // 1:WhiteShort, 2:WhiteLong, 4:BlackShort, 8:BlackLong
        chess->movelist[n].castle = 15;
        chess->movelist[n].not_pawn_move = 0;
        chess->movelist[n].en_passant = 0;
        chess->movelist[n].white_king_castled = 0;
        chess->movelist[n].black_king_castled = 0;
        chess->movelist[n].pos_white_king = 25;
        chess->movelist[n].pos_black_king = 95;
        chess->movelist[n].white_double_bishops =
            eval->double_bishops;
        chess->movelist[n].black_double_bishops =
            eval->double_bishops;
        chess->movelist[n].further = 0;
#ifdef POS_FIGURE
        for (int i = 0; i < 16; i++) {
            chess->movelist[n].pos_white_figure[i] =
                new_pos_white_figure[i];
            chess->movelist[n].pos_black_figure[i] =
                new_pos_black_figure[i];
        }
#endif
        for (int i = 0; i < 120; i++) {
            chess->tablelist[n][i] = new_table[i];
        }
    }
    chess->move_number = 0;
    // Compute the starting Zobrist key once; update_table() will maintain it incrementally
    const uint64_t start_key = chess->compute_zobrist_key(0);
    for (int n = 0; n < MAX_MOVES; n++) {
        chess->movelist[n].zobrist_key = start_key;
    }
}

void Table::update_table(const int move, const bool print, const bool fake) {

    // Copies the table for the next move and updates later according to the move
    int *pt1 = chess->tablelist[chess->move_number];
    struct position_t *pm1 = chess->movelist + chess->move_number;
    chess->move_number++;
    int *pt2 = chess->tablelist[chess->move_number];
    struct position_t *pm2 = chess->movelist + chess->move_number;
    memcpy(pt2, pt1, 120 * sizeof(int));
    const int x_from = (move & 0xe000) >> 13;
    const int y_from = (move & 0x1c00) >> 10;
    const int x_to = (move & 0x00e0) >> 5;
    const int y_to = (move & 0x001c) >> 2;
    const int figure_from = *(pt2 + 1 + x_from + (y_from + 2) * 10);
    const int figure_to = *(pt2 + 1 + x_to + (y_to + 2) * 10);

    // Copies the parameters according to the previous status and updates later
    pm2->pos_white_king = pm1->pos_white_king;
    pm2->pos_black_king = pm1->pos_black_king;
    pm2->white_king_castled = pm1->white_king_castled;
    pm2->black_king_castled = pm1->black_king_castled;
    pm2->en_passant = pm1->en_passant; // always copy; overwritten later in !fake path
    pm2->not_pawn_move = pm1->not_pawn_move; // always copy; overwritten later in !fake path

    const int square_from = 1 + x_from + (y_from + 2) * 10;
    const int square_to = 1 + x_to + (y_to + 2) * 10;
    pm2->color = chess->player_to_move;
    pm2->move_from = square_from;
    pm2->move_to = square_to;
    pm2->castle = pm1->castle;
    pm2->captured_figure = figure_to;

    if (!fake) {
        if (figure_to == WhiteBishop) {
            pm2->white_double_bishops = 0;
        } else if (figure_to == BlackBishop) {
            pm2->black_double_bishops = 0;
        } else {
            pm2->white_double_bishops = pm1->white_double_bishops;
            pm2->black_double_bishops = pm1->black_double_bishops;
        }

        // If captured is occured->not_pawn_move parameters is set to 1,
        // and further is set to 1
        if (figure_to != EMPTY) {
            pm2->not_pawn_move = 1;
            pm2->further = 1;
        } else if (pm1->further == 2) {
            pm2->further = 1;
        } else {
            pm2->further = 0;
        }
    }

    // Promotion
    char promotion = ' ';
    if ((move & 0x0303) > 0) {
        if ((move & 0x0200) == 0x0200) {
            promotion = 'q';
        } else if ((move & 0x0100) == 0x0100) {
            promotion = 'r';
        } else if ((move & 0x0002) == 0x0002) {
            promotion = 'b';
        } else if ((move & 0x0001) == 0x0001) {
            promotion = 'n';
        }
        for (int n = 0; n < 14; n++) {
            if (promotion == graphical_figure[n][1]) {
                pm2->promotion = (graphical_figure[n][0] & 127);
                *(pt2 + square_to) = (graphical_figure[n][0] & 127);
                break;
            }
        }
        if (chess->player_to_move == Chess::BLACK) {
            *(pt2 + square_to) += BlackColor;
        }
    }

    // Not promotion
    else {
        pm2->promotion = 0;
        *(pt2 + square_to) = figure_from;
    }

    // If not pawn move
    if (!(figure_from == WhitePawn || figure_from == BlackPawn)) {
        if (figure_from == WhiteKing) {
            pm2->pos_white_king = square_to;

            // White Castling is not possible any more
            pm2->castle = pm2->castle & 12;

            // Castling
            if ((square_from == 25 && square_to == 27) ||
                (square_from == 25 && square_to == 23)) {
                pm2->white_king_castled = eval->king_castled;
                if (square_to == 27) {
                    *(pt2 + 26) = WhiteRook;
                    *(pt2 + 28) = EMPTY;
                } else if (square_to == 23) {
                    *(pt2 + 24) = WhiteRook;
                    *(pt2 + 21) = EMPTY;
                }
            }
        } else if (figure_from == BlackKing) {
            pm2->pos_black_king = square_to;

            // Black Castling is not possible any more
            pm2->castle = pm2->castle & 3;
            if ((square_from == 95 && square_to == 97) ||
                (square_from == 95 && square_to == 93)) {
                pm2->black_king_castled = eval->king_castled;
                if (square_to == 97) {
                    *(pt2 + 96) = BlackRook;
                    *(pt2 + 98) = EMPTY;
                } else if (square_to == 93) {
                    *(pt2 + 94) = BlackRook;
                    *(pt2 + 91) = EMPTY;
                }
            }
        }

        if (!fake) {
            pm2->en_passant = 0;
            if (chess->move_number == 1) {
                pm2->not_pawn_move = 1;

                // not_pawn_move
            } else {
                pm2->not_pawn_move = pm1->not_pawn_move + 1;
            }
            if (figure_from == WhiteRook) {
                // White Long Castling is not possible any more
                if (square_from == 21) {
                    pm2->castle = pm2->castle & 13;

                    // White Short Castling is not possible any more
                } else if (square_from == 28) {
                    pm2->castle = pm2->castle & 14;
                }
            } else if (figure_from == BlackRook) {
                // Black Long Castling is not possible any more
                if (square_from == 91) {
                    pm2->castle = pm2->castle & 7;

                    // Black Short Castling is not possible any more
                } else if (square_from == 98) {
                    pm2->castle = pm2->castle & 11;
                }
            }
        }

    } else { // If pawn move

        if (!fake) {
#ifdef QUIESCENCE_SEARCH
            if (chess->player_to_move == Chess::WHITE && y_from > 3) {
                pm2->further = 2;
            }
            if (chess->player_to_move == Chess::BLACK && y_from < 4) {
                pm2->further = 2;
            }
#endif
            pm2->not_pawn_move = 0;
            if (square_to - square_from == 20) {
                pm2->en_passant = square_from + 10; // en passant possible
            }
            if (square_from - square_to == 20) {
                pm2->en_passant = square_to + 10; // en passant possible
            }
            if (abs(square_to - square_from) != 20) {
                pm2->en_passant = 0; // en passant not possible
            }
        }

        if (pm1->en_passant > 1) { // en passant possible
            if (figure_from == WhitePawn) {
                if (pm1->en_passant == square_from + 11 &&
                    square_to - square_from == 11) {
                    pm2->captured_figure = BlackPawn;
                    *(pt2 + square_to - 10) = EMPTY;
                } else if (pm1->en_passant == square_from + 9 &&
                           square_to - square_from == 9) {
                    pm2->captured_figure = BlackPawn;
                    *(pt2 + square_to - 10) = EMPTY;
                }
            } else if (figure_from == BlackPawn) {
                if (pm1->en_passant == square_from - 11 &&
                    square_to - square_from == -11) {
                    pm2->captured_figure = WhitePawn;
                    *(pt2 + square_to + 10) = EMPTY;
                } else if (pm1->en_passant == square_from - 9 &&
                           square_to - square_from == -9) {
                    pm2->captured_figure = WhitePawn;
                    *(pt2 + square_to + 10) = EMPTY;
                }
            }
        }
    }

    *(pt2 + square_from) = EMPTY;

#ifdef QUIESCENCE_SEARCH
    if (!fake) {
        if (is_attacked(chess->player_to_move == Chess::WHITE
                    ? pm2->pos_black_king
                    : pm2->pos_white_king,
                    -chess->player_to_move)) {
            pm2->further = 2;
        }
    }
#endif

    // --- Incremental Zobrist key update (only needed for real moves, not fake/sorting moves) ---
    if (!fake) {
        uint64_t key = pm1->zobrist_key;

        // Helper lambdas for readability
        auto xor_piece = [&](int sq, int piece) {
            const int ci = (piece & 128) >> 7;
            const int fig = piece & 127;
            key ^= chess->zobrist_piece[ci][fig][sq];
        };

        // 1. Flip side to move
        key ^= chess->zobrist_side_white;
        key ^= chess->zobrist_side_black;

        // 2. Castle rights changed
        key ^= chess->zobrist_castle[pm1->castle & 15];
        key ^= chess->zobrist_castle[pm2->castle & 15];

        // 3. En passant changed
        key ^= chess->zobrist_enpassant[pm1->en_passant];
        key ^= chess->zobrist_enpassant[pm2->en_passant];

        // 4. Moving piece leaves square_from
        xor_piece(square_from, figure_from);

        // 5. Captured piece (regular capture) leaves square_to
        if (figure_to != EMPTY) {
            xor_piece(square_to, figure_to);
        }

        // 6. Moving piece (or promoted piece) arrives at square_to
        //    pt2[square_to] already holds the final piece value
        xor_piece(square_to, *(pt2 + square_to));

        // 7. Castling: rook also moved
        if (figure_from == WhiteKing) {
            if (square_from == 25 && square_to == 27) {
                // Short castle: rook 28->26
                xor_piece(28, WhiteRook);
                xor_piece(26, WhiteRook);
            } else if (square_from == 25 && square_to == 23) {
                // Long castle: rook 21->24
                xor_piece(21, WhiteRook);
                xor_piece(24, WhiteRook);
            }
        } else if (figure_from == BlackKing) {
            if (square_from == 95 && square_to == 97) {
                // Short castle: rook 98->96
                xor_piece(98, BlackRook);
                xor_piece(96, BlackRook);
            } else if (square_from == 95 && square_to == 93) {
                // Long castle: rook 91->94
                xor_piece(91, BlackRook);
                xor_piece(94, BlackRook);
            }
        }

        // 8. En passant capture: the captured pawn is not at square_to
        if (pm2->captured_figure != EMPTY && figure_to == EMPTY) {
            // This was an en passant capture — the pawn was removed from a different square
            if (figure_from == WhitePawn) {
                xor_piece(square_to - 10, BlackPawn);
            } else if (figure_from == BlackPawn) {
                xor_piece(square_to + 10, WhitePawn);
            }
        }

        pm2->zobrist_key = key;
    }
    // --- End incremental Zobrist update ---

    if (print) {
        print_table();
    }
}

// Prints the table and the parameters to the console
void Table::print_table() {
    for (int i = 11; i >= 0; i--) {
        for (int j = 0; j < 10; j++) {
            const int field = chess->tablelist[chess->move_number][i * 10 + j];
            if (field == OFFBOARD) {
                continue;
            }
            printf("%3x", field);
        }
        printf("\n");
    }
    printf("Move number    : %d\n", chess->move_number);
    printf("Move color     : %d\n", chess->movelist[chess->move_number].color);
    printf("Move from      : %d\n",
           chess->movelist[chess->move_number].move_from);
    printf("Move to        : %d\n",
           chess->movelist[chess->move_number].move_to);
    printf("Captured       : %d\n",
           chess->movelist[chess->move_number].captured_figure);
    printf("Promotion      : %d\n",
           chess->movelist[chess->move_number].promotion);
    printf("Castle         : %d\n", chess->movelist[chess->move_number].castle);
    printf("Not Pawn Move  : %d\n",
           chess->movelist[chess->move_number].not_pawn_move);
    printf("En passant     : %d\n",
           chess->movelist[chess->move_number].en_passant);
    printf("White castled  : %d\n",
           chess->movelist[chess->move_number].white_king_castled);
    printf("Black castled  : %d\n",
           chess->movelist[chess->move_number].black_king_castled);
    printf("Pos White king : %d\n",
           chess->movelist[chess->move_number].pos_white_king);
    printf("Pos Black king : %d\n",
           chess->movelist[chess->move_number].pos_black_king);
    printf("White2bishops  : %d\n",
           chess->movelist[chess->move_number].white_double_bishops);
    printf("Black2bishops  : %d\n",
           chess->movelist[chess->move_number].black_double_bishops);
    printf("Further invest.: %d\n",
           chess->movelist[chess->move_number].further);
    printf("\n");
    Util::flush();
}

// FEN interpreter
void Table::setboard(const char *input) {
    size_t n = strlen("position fen ");
    int x = 1;
    int y = 9;
    char move_old[6] = "     ";
    chess->start_game();
    int move_number = 1;
    chess->move_number = 1;
    while (input[n] != ' ') {
        if (input[n] > '0' && input[n] < '9') {
            for (int m = 0; m < input[n] - '0'; m++) {
                chess->tablelist[move_number][y * 10 + x] = EMPTY;
                x++;
            }
            --x;
        }
        if (input[n] != '/') {
            for (int m = 0; m < 14; m++) {
                if (input[n] == graphical_figure[m][1]) {
                    chess->tablelist[move_number][y * 10 + x] =
                        graphical_figure[m][0];
                    break;
                }
            }
            x++;
        }
        if (input[n] == '/') {
            y--;
            x = 1;
        }
        n++;
    }
    n++;
    if (input[n] == 'w') {
        chess->player_to_move = Chess::WHITE;
        chess->FZChess = Chess::WHITE;
        chess->movelist[move_number].color = Chess::WHITE;
    } else {
        chess->player_to_move = Chess::BLACK;
        chess->FZChess = Chess::BLACK;
        chess->movelist[move_number].color = Chess::BLACK;
    }
    n++;
    n++;
    chess->movelist[move_number].castle = 0;
    while (input[n] != ' ') {
        if (input[n] == '-') {
            chess->movelist[move_number].castle = 0;
        }
        if (input[n] == 'K') {
            chess->movelist[move_number].castle =
                chess->movelist[move_number].castle | 1;
        }
        if (input[n] == 'Q') {
            chess->movelist[move_number].castle =
                chess->movelist[move_number].castle | 2;
        }
        if (input[n] == 'k') {
            chess->movelist[move_number].castle =
                chess->movelist[move_number].castle | 4;
        }
        if (input[n] == 'q') {
            chess->movelist[move_number].castle =
                chess->movelist[move_number].castle | 8;
        }
        n++;
    }
    chess->movelist[move_number - 1].castle =
        chess->movelist[move_number].castle;
    n++;
    if (input[n] == '-') {
        chess->movelist[move_number].en_passant = 0;
    }
    if (input[n] >= 'a' &&
        input[n] <= 'h') { // en passant possible, target square
        chess->movelist[move_number].en_passant =
            1 + input[n] - 'a' + (input[n + 1] - '1' + 2) * 10;
        n++;
    }
    n++;
    n++;
    chess->movelist[move_number].not_pawn_move = 0;
    int factor = 1;
    while (input[n] != ' ') {
        chess->movelist[move_number].not_pawn_move =
            chess->movelist[move_number].not_pawn_move * factor +
            (int)(input[n] - '0');
        factor = factor * 10;
        n++;
    }
    int white_bishop = 0;
    int black_bishop = 0;
    for (int i = 0; i < 120; ++i) {
        if ((chess->tablelist[move_number][i] & 127) == Bishop) {
            if (chess->tablelist[move_number][i] == WhiteBishop) {
                white_bishop++;
            }
            if (chess->tablelist[move_number][i] == BlackBishop) {
                black_bishop++;
            }
        }
        if ((chess->tablelist[move_number][i] & 127) == King) {
            if (chess->tablelist[move_number][i] == WhiteKing) {
                chess->movelist[move_number].pos_white_king = i;
            }
            if (chess->tablelist[move_number][i] == BlackKing) {
                chess->movelist[move_number].pos_black_king = i;
            }
        }
    }
    if (white_bishop < 2) {
        chess->movelist[move_number].white_double_bishops = 0;
    }
    if (black_bishop < 2) {
        chess->movelist[move_number].black_double_bishops = 0;
    }
    if (strstr(input, "moves")) {
        size_t m = strlen(input) - 1;
        for (size_t i = strstr(input, "moves") - input + 6; i < m; i++) {
            // printf("i: %d, input[i]: %c\n", i, input[i]);Util::flush();
            move_old[0] = input[i++];
            move_old[1] = input[i++];
            move_old[2] = input[i++];
            move_old[3] = input[i++];
            if (input[i] != ' ' && input[i] != '\n') {
                move_old[4] = input[i++];
            } else {
                move_old[4] = '\0';
            }
            // printf("move: %s\n", move);Util::flush();
            update_table(Util::str2move(move_old), false);
            chess->invert_player_to_move();
        }
    }
    // (Re)compute the Zobrist key for the final position after all FEN moves are applied.
    // update_table() maintains it incrementally from here on.
    chess->movelist[chess->move_number].zobrist_key =
        chess->compute_zobrist_key(chess->move_number);
    print_table();
}

// Returns if the figure of color is attacked or not
bool Table::is_attacked(const int field, const int color) {
    const int * const ptablelist = chess->tablelist[chess->move_number];
    int QueenColor, RookColor, KingColor, BishopColor, KnightColor;
    if (color == Chess::WHITE) {
        if (ptablelist[field + 9] == BlackPawn || ptablelist[field + 11] == BlackPawn) {
            return true;
        }
        QueenColor = BlackQueen;
        KingColor = BlackKing;
        RookColor = BlackRook;
        BishopColor = BlackBishop;
        KnightColor = BlackKnight;
    } else {
        if (ptablelist[field - 9] == WhitePawn || ptablelist[field - 11] == WhitePawn) {
            return true;
        }
        QueenColor = WhiteQueen;
        KingColor = WhiteKing;
        RookColor = WhiteRook;
        BishopColor = WhiteBishop;
        KnightColor = WhiteKnight;
    }
    for (int k = 0; k < 4; k++) {
        int kk = 1;
        while (ptablelist[field + kk * dir_rook[k]] == EMPTY) {
            ++kk;
        }
        const int coord = field + kk * dir_rook[k];
        if (ptablelist[coord] == RookColor) {
            return true;
        }
        if (ptablelist[coord] == QueenColor) {
            return true;
        }
        if (kk == 1 && ptablelist[coord] == KingColor) {
            return true;
        }
    }
    for (int k = 0; k < 4; k++) {
        int kk = 1;
        while (ptablelist[field + kk * dir_bishop[k]] == EMPTY) {
            ++kk;
        }
        const int coord = field + kk * dir_bishop[k];
        if (ptablelist[coord] == BishopColor) {
            return true;
        }
        if (ptablelist[coord] == QueenColor) {
            return true;
        }
        if (kk == 1 && ptablelist[coord] == KingColor) {
            return true;
        }
    }
    for (int k = 0; k < 8; k++) {
        if (ptablelist[field + dir_knight[k]] == KnightColor) {
            return true;
        }
    }
    return false;
}

// Checks not enough material
bool Table::is_not_enough_material() {
    int white_knight = 0;
    int white_bishop = 0;
    int black_knight = 0;
    int black_bishop = 0;
    for (int i = 20; i < 100; i++) {
        int c = chess->tablelist[chess->move_number][i];
        if (c == 0 || c == OFFBOARD) {
            continue;
        }
        if (c == WhitePawn) {
            return false;
        }
        if (c == BlackPawn) {
            return false;
        }
        if (c == WhiteQueen) {
            return false;
        }
        if (c == BlackQueen) {
            return false;
        }
        if (c == WhiteRook) {
            return false;
        }
        if (c == BlackRook) {
            return false;
        }
        if (c == WhiteKnight) {
            white_knight++;
        } else if (c == BlackKnight) {
            black_knight++;
        } else if (c == WhiteBishop) {
            white_bishop++;
        } else if (c == BlackBishop) {
            black_bishop++;
        }
    }
    if (white_bishop == 2) {
        return false;
    }
    if (black_bishop == 2) {
        return false;
    }
    if (white_knight + white_bishop > 1) {
        return false;
    }
    if (black_knight + black_bishop > 1) {
        return false;
    }
    // printf("not enough material\n");Util::flush();
    return true;
}

// Checks castlings and adds to legal moves if possible
void Table::castling() {
    const int *t = chess->tablelist[chess->move_number];
    if (chess->player_to_move == Chess::WHITE) {
        // Checks the conditions of castling
        // e1g1
        if (t[28] == WhiteRook &&
                t[25] == WhiteKing &&
                (chess->movelist[chess->move_number].castle & 1) == 1 &&
                t[26] == EMPTY &&
                t[27] == EMPTY &&
                !is_attacked(25, Chess::WHITE) &&
                !is_attacked(26, Chess::WHITE)) {
            chess->legal_pointer++;
            chess->legal_moves[chess->legal_pointer] = 0x80c0;
            is_really_legal();
        }
        // e1c1
        if (t[25] == WhiteKing &&
                t[21] == WhiteRook &&
                (chess->movelist[chess->move_number].castle & 2) == 2 &&
                t[24] == EMPTY &&
                t[23] == EMPTY &&
                t[22] == EMPTY &&
                !is_attacked(25, Chess::WHITE) &&
                !is_attacked(24, Chess::WHITE)) {
            chess->legal_pointer++;
            chess->legal_moves[chess->legal_pointer] = 0x8040;
            is_really_legal();
        }

        // if (chess->player_to_move == Chess::BLACK)
    } else {
        if ((chess->movelist[chess->move_number].castle & 4) == 4 &&
                t[96] == EMPTY &&
                t[97] == EMPTY &&
                t[98] == BlackRook &&
                t[95] == BlackKing &&
                !is_attacked(95, Chess::BLACK) &&
                !is_attacked(96, Chess::BLACK)) {
            chess->legal_pointer++;
            // e8g8
            chess->legal_moves[chess->legal_pointer] = 0x9cdc;
            is_really_legal();
        }
        // e8c8
        if (t[95] == BlackKing &&
                t[91] == BlackRook &&
                (chess->movelist[chess->move_number].castle & 8) == 8 &&
                t[94] == EMPTY &&
                t[93] == EMPTY &&
                t[92] == EMPTY &&
                !is_attacked(95, Chess::BLACK) &&
                !is_attacked(94, Chess::BLACK)) {
            chess->legal_pointer++;
            chess->legal_moves[chess->legal_pointer] = 0x9c5c;
            is_really_legal();
        }
    }
}

// Checks third occurance
bool Table::third_occurance() {
    int occurance = 0;
    if (chess->move_number < 6) {
        return false;
    }
    int i = chess->move_number - 2;
    while (i >= 0 && occurance < 2) {
        // printf("Third occurance: i: %d\n", i);Util::flush();
        bool equal = true;
        for (int j = 20; j < 100; j++) {
            if (chess->tablelist[chess->move_number][j] != chess->tablelist[i][j]) {
                equal = false;
                break;
            }
        }
        if (equal) {
            occurance++;
        }
        i -= 2;
    }
    if (occurance >= 2) {
        return true;
    } else {
        return false;
    }
}

// Searches and stores all the legal moves
void Table::list_legal_moves() {
    int move;
    // chess->legal_pointer = -1 means no legal moves
    chess->legal_pointer = -1;
    // Maps the table
    pt = chess->tablelist[chess->move_number];
    int *ptt = pt + 20; // skip table borders
    --ptt;
    for (int i = 2; i < 10; i++) {
        for (int j = 0; j < 10; j++) {
            ptt++;
            const int field = *ptt;
            if (field == OFFBOARD) {
                continue;
            }

            // figure without color
            const int figure = field & 127;
            const int field_color = field & 128;

            // Right color found
            if ((chess->player_to_move == Chess::WHITE && field_color == WhiteColor) ||
                (chess->player_to_move == Chess::BLACK && field_color == BlackColor)) {
                if (field == WhitePawn) {
                    // If upper field is empty
                    if (*(ptt + 10) == EMPTY) {
                        // If white pawn is in the 7th rank->promotion
                        if (i - 1 == 7) {
                            // Calculates Queen promotion
                            chess->legal_pointer++;
                            move = 0;
                            move |= (j - 1) * 0x2000;
                            move |= 6 * 0x0400;
                            move |= (j - 1) * 0x0020;
                            move |= 7 * 0x0004;
                            move |= 0x0200;
                            chess->legal_moves[chess->legal_pointer] = move;
                            is_really_legal();

#ifdef PERFT
                            // Calculates Rook promotion
                            chess->legal_pointer++;
                            move = 0;
                            move |= (j - 1) * 0x2000;
                            move |= 6 * 0x0400;
                            move |= (j - 1) * 0x0020;
                            move |= 7 * 0x0004;
                            move |= 0x0100;
                            chess->legal_moves[chess->legal_pointer] = move;
                            is_really_legal();

                            // Calculates Bishop promotion
                            chess->legal_pointer++;
                            move = 0;
                            move |= (j - 1) * 0x2000;
                            move |= 6 * 0x0400;
                            move |= (j - 1) * 0x0020;
                            move |= 7 * 0x0004;
                            move |= 0x0002;
                            chess->legal_moves[chess->legal_pointer] = move;
                            is_really_legal();

#endif
                            // Calculates Knight promotion
                            chess->legal_pointer++;
                            move = 0;
                            move |= (j - 1) * 0x2000;
                            move |= 6 * 0x0400;
                            move |= (j - 1) * 0x0020;
                            move |= 7 * 0x0004;
                            move |= 0x0001;
                            chess->legal_moves[chess->legal_pointer] = move;
                            is_really_legal();
                        } else {
                            // Normal pawn move
                            chess->legal_pointer++;
                            move = 0;
                            move |= (j - 1) * 0x2000;
                            move |= (i - 2) * 0x0400;
                            move |= (j - 1) * 0x0020;
                            move |= (i - 1) * 0x0004;
                            chess->legal_moves[chess->legal_pointer] = move;
                            is_really_legal();
                        }

                        // If white pawn is in the 2nd rank
                        if (i - 1 == 2) {
                            if (*(ptt + 20) == EMPTY) {
                                chess->legal_pointer++;
                                move = 0;
                                move |= (j - 1) * 0x2000;
                                move |= 0x0400;
                                move |= (j - 1) * 0x0020;
                                move |= 3 * 0x0004;
                                chess->legal_moves[chess->legal_pointer] = move;
                                is_really_legal();
                            }
                        }
                    }

                    // Pawn capture
                    if ((*(ptt + 9) & 128) == BlackColor && *(ptt + 9) != OFFBOARD) {
                        if (i - 1 == 7) {
                            // With Queen promotion
                            chess->legal_pointer++;
                            move = 0;
                            move |= (j - 1) * 0x2000;
                            move |= 6 * 0x0400;
                            move |= (j - 2) * 0x0020;
                            move |= 7 * 0x0004;
                            move |= 0x0200;
                            chess->legal_moves[chess->legal_pointer] = move;
                            is_really_legal();

#ifdef PERFT
                            // With Rook promotion
                            chess->legal_pointer++;
                            move = 0;
                            move |= (j - 1) * 0x2000;
                            move |= 6 * 0x0400;
                            move |= (j - 2) * 0x0020;
                            move |= 7 * 0x0004;
                            move |= 0x0100;
                            chess->legal_moves[chess->legal_pointer] = move;
                            is_really_legal();

                            // With Bishop promotion
                            chess->legal_pointer++;
                            move = 0;
                            move |= (j - 1) * 0x2000;
                            move |= 6 * 0x0400;
                            move |= (j - 2) * 0x0020;
                            move |= 7 * 0x0004;
                            move |= 0x0002;
                            chess->legal_moves[chess->legal_pointer] = move;
                            is_really_legal();

#endif
                            // With Knight promotion
                            chess->legal_pointer++;
                            move = 0;
                            move |= (j - 1) * 0x2000;
                            move |= 6 * 0x0400;
                            move |= (j - 2) * 0x0020;
                            move |= 7 * 0x0004;
                            move |= 0x0001;
                            chess->legal_moves[chess->legal_pointer] = move;
                            is_really_legal();
                        } else {
                            // Normal capture
                            chess->legal_pointer++;
                            move = 0;
                            move |= (j - 1) * 0x2000;
                            move |= (i - 2) * 0x0400;
                            move |= (j - 2) * 0x0020;
                            move |= (i - 1) * 0x0004;
                            chess->legal_moves[chess->legal_pointer] = move;
                            is_really_legal();
                        }
                    }

                    // Pawn capture of the other direction
                    if ((*(ptt + 11) & 128) == BlackColor && *(ptt + 11) != OFFBOARD) {
                        if (i - 1 == 7) {
                            // With Queen promotion
                            chess->legal_pointer++;
                            move = 0;
                            move |= (j - 1) * 0x2000;
                            move |= 6 * 0x0400;
                            move |= (j)*0x0020;
                            move |= 7 * 0x0004;
                            move |= 0x0200;
                            chess->legal_moves[chess->legal_pointer] = move;
                            is_really_legal();

#ifdef PERFT
                            // With Rook promotion
                            chess->legal_pointer++;
                            move = 0;
                            move |= (j - 1) * 0x2000;
                            move |= 6 * 0x0400;
                            move |= (j)*0x0020;
                            move |= 7 * 0x0004;
                            move |= 0x0100;
                            chess->legal_moves[chess->legal_pointer] = move;
                            is_really_legal();

                            // With Bishop promotion
                            chess->legal_pointer++;
                            move = 0;
                            move |= (j - 1) * 0x2000;
                            move |= 6 * 0x0400;
                            move |= (j)*0x0020;
                            move |= 7 * 0x0004;
                            move |= 0x0002;
                            chess->legal_moves[chess->legal_pointer] = move;
                            is_really_legal();

#endif
                            // With Knight promotion
                            chess->legal_pointer++;
                            move = 0;
                            move |= (j - 1) * 0x2000;
                            move |= 6 * 0x0400;
                            move |= (j)*0x0020;
                            move |= 7 * 0x0004;
                            move |= 0x0001;
                            chess->legal_moves[chess->legal_pointer] = move;
                            is_really_legal();
                        } else {
                            // Normal capture
                            chess->legal_pointer++;
                            move = 0;
                            move |= (j - 1) * 0x2000;
                            move |= (i - 2) * 0x0400;
                            move |= (j)*0x0020;
                            move |= (i - 1) * 0x0004;
                            chess->legal_moves[chess->legal_pointer] = move;
                            is_really_legal();
                        }
                    }

                    // If en passant is possible
                    const int en_pass = (chess->movelist + chess->move_number)->en_passant;

                    // If it is the right field
                    if (en_pass && en_pass == i * 10 + j + 9) {
                        chess->legal_pointer++;
                        move = 0;
                        move |= (j - 1) * 0x2000;
                        move |= 4 * 0x0400;
                        move |= (j - 2) * 0x0020;
                        move |= 5 * 0x0004;
                        chess->legal_moves[chess->legal_pointer] = move;
                        is_really_legal();
                    }

                    // If it is the right field
                    if (en_pass && en_pass == i * 10 + j + 11) {
                        chess->legal_pointer++;
                        move = 0;
                        move |= (j - 1) * 0x2000;
                        move |= 4 * 0x0400;
                        move |= (j)*0x0020;
                        move |= 5 * 0x0004;
                        chess->legal_moves[chess->legal_pointer] = move;
                        is_really_legal();
                    }
                }

                // The same for black pawn
                else if (field == BlackPawn) {
                    if (*(ptt - 10) == EMPTY) {
                        if (i - 1 == 2) {
                            // With Queen promotion
                            chess->legal_pointer++;
                            move = 0;
                            move |= (j - 1) * 0x2000;
                            move |= 1 * 0x0400;
                            move |= (j - 1) * 0x0020;
                            move |= (i - 3) * 0x0004;
                            move |= 0x0200;
                            chess->legal_moves[chess->legal_pointer] = move;
                            is_really_legal();

#ifdef PERFT
                            // With Rook promotion
                            chess->legal_pointer++;
                            move = 0;
                            move |= (j - 1) * 0x2000;
                            move |= 1 * 0x0400;
                            move |= (j - 1) * 0x0020;
                            move |= (i - 3) * 0x0004;
                            move |= 0x0100;
                            chess->legal_moves[chess->legal_pointer] = move;
                            is_really_legal();

                            // With Bishop promotion
                            chess->legal_pointer++;
                            move = 0;
                            move |= (j - 1) * 0x2000;
                            move |= 1 * 0x0400;
                            move |= (j - 1) * 0x0020;
                            move |= (i - 3) * 0x0004;
                            move |= 0x0002;
                            chess->legal_moves[chess->legal_pointer] = move;
                            is_really_legal();

#endif
                            // With Knight promotion
                            chess->legal_pointer++;
                            move = 0;
                            move |= (j - 1) * 0x2000;
                            move |= 1 * 0x0400;
                            move |= (j - 1) * 0x0020;
                            move |= (i - 3) * 0x0004;
                            move |= 0x00001;
                            chess->legal_moves[chess->legal_pointer] = move;
                            is_really_legal();
                        } else {
                            chess->legal_pointer++;
                            move = 0;
                            move |= (j - 1) * 0x2000;
                            move |= (i - 2) * 0x0400;
                            move |= (j - 1) * 0x0020;
                            move |= (i - 3) * 0x0004;
                            chess->legal_moves[chess->legal_pointer] = move;
                            is_really_legal();
                        }
                        if (i - 1 == 7) {
                            if (*(ptt - 20) == EMPTY) {
                                chess->legal_pointer++;
                                move = 0;
                                move |= (j - 1) * 0x2000;
                                move |= 6 * 0x0400;
                                move |= (j - 1) * 0x0020;
                                move |= 4 * 0x0004;
                                chess->legal_moves[chess->legal_pointer] = move;
                                is_really_legal();
                            }
                        }
                    }
                    if (*(ptt - 9) > 0 && *(ptt - 9) < BlackColor) {
                        if (i - 1 == 2) {
                            // With Queen promotion
                            chess->legal_pointer++;
                            move = 0;
                            move |= (j - 1) * 0x2000;
                            move |= (i - 2) * 0x0400;
                            move |= (j)*0x0020;
                            move |= (i - 3) * 0x0004;
                            move |= 0x0200;
                            chess->legal_moves[chess->legal_pointer] = move;
                            is_really_legal();

#ifdef PERFT
                            // With Rook promotion
                            chess->legal_pointer++;
                            move = 0;
                            move |= (j - 1) * 0x2000;
                            move |= (i - 2) * 0x0400;
                            move |= (j)*0x0020;
                            move |= (i - 3) * 0x0004;
                            move |= 0x0100;
                            chess->legal_moves[chess->legal_pointer] = move;
                            is_really_legal();

                            // With Bishop promotion
                            chess->legal_pointer++;
                            move = 0;
                            move |= (j - 1) * 0x2000;
                            move |= (i - 2) * 0x0400;
                            move |= (j)*0x0020;
                            move |= (i - 3) * 0x0004;
                            move |= 0x0002;
                            chess->legal_moves[chess->legal_pointer] = move;
                            is_really_legal();

#endif
                            // With Knight promotion
                            chess->legal_pointer++;
                            move = 0;
                            move |= (j - 1) * 0x2000;
                            move |= (i - 2) * 0x0400;
                            move |= (j)*0x0020;
                            move |= (i - 3) * 0x0004;
                            move |= 0x0001;
                            chess->legal_moves[chess->legal_pointer] = move;
                            is_really_legal();
                        } else {
                            chess->legal_pointer++;
                            move = 0;
                            move |= (j - 1) * 0x2000;
                            move |= (i - 2) * 0x0400;
                            move |= (j)*0x0020;
                            move |= (i - 3) * 0x0004;
                            chess->legal_moves[chess->legal_pointer] = move;
                            is_really_legal();
                        }
                    }
                    if (*(ptt - 11) > 0 && *(ptt - 11) < BlackColor) {
                        if (i - 1 == 2) {
                            // With Queen promotion
                            chess->legal_pointer++;
                            move = 0;
                            move |= (j - 1) * 0x2000;
                            move |= (i - 2) * 0x0400;
                            move |= (j - 2) * 0x0020;
                            move |= (i - 3) * 0x0004;
                            move |= 0x0200;
                            chess->legal_moves[chess->legal_pointer] = move;
                            is_really_legal();

#ifdef PERFT
                            // With Rook promotion
                            chess->legal_pointer++;
                            move = 0;
                            move |= (j - 1) * 0x2000;
                            move |= (i - 2) * 0x0400;
                            move |= (j - 2) * 0x0020;
                            move |= (i - 3) * 0x0004;
                            move |= 0x0100;
                            chess->legal_moves[chess->legal_pointer] = move;
                            is_really_legal();

                            // With Bishop promotion
                            chess->legal_pointer++;
                            move = 0;
                            move |= (j - 1) * 0x2000;
                            move |= (i - 2) * 0x0400;
                            move |= (j - 2) * 0x0020;
                            move |= (i - 3) * 0x0004;
                            move |= 0x0002;
                            chess->legal_moves[chess->legal_pointer] = move;
                            is_really_legal();

#endif
                            // With Knight promotion
                            chess->legal_pointer++;
                            move = 0;
                            move |= (j - 1) * 0x2000;
                            move |= (i - 2) * 0x0400;
                            move |= (j - 2) * 0x0020;
                            move |= (i - 3) * 0x0004;
                            move |= 0x0001;
                            chess->legal_moves[chess->legal_pointer] = move;
                            is_really_legal();
                        } else {
                            chess->legal_pointer++;
                            move = 0;
                            move |= (j - 1) * 0x2000;
                            move |= (i - 2) * 0x0400;
                            move |= (j - 2) * 0x0020;
                            move |= (i - 3) * 0x0004;
                            chess->legal_moves[chess->legal_pointer] = move;
                            is_really_legal();
                        }
                    }
                    const int en_pass = (chess->movelist + chess->move_number)->en_passant;
                    if (en_pass > 1) {
                        if (en_pass == i * 10 + j - 9) {
                            chess->legal_pointer++;
                            move = 0;
                            move |= (j - 1) * 0x2000;
                            move |= 3 * 0x0400;
                            move |= (j)*0x0020;
                            move |= 2 * 0x0004;
                            chess->legal_moves[chess->legal_pointer] = move;
                            is_really_legal();
                        }
                    }
                    if (en_pass > 1) {
                        if (en_pass == i * 10 + j - 11) {
                            chess->legal_pointer++;
                            move = 0;
                            move |= (j - 1) * 0x2000;
                            move |= 3 * 0x0400;
                            move |= (j - 2) * 0x0020;
                            move |= 2 * 0x0004;
                            chess->legal_moves[chess->legal_pointer] = move;
                            is_really_legal();
                        }
                    }
                } else if (figure == Knight) {
                    // kk : distance
                    // k : number of directions of possible knight moves
                    const int kk = 1;
                    for (int k = 0; k < 8; k++) {
                        if (*(ptt + dir_knight[k]) != OFFBOARD) {
                            append_legal_moves(dir_knight[k], i, j, kk);
                        }
                    }
                } else if (figure == King) {
                    // Appends castling moves if possible
                    castling();
                    const int kk = 1;
                    for (int k = 0; k < 8; k++) {
                        if (*(ptt + dir_king[k]) != OFFBOARD) {
                            append_legal_moves(dir_king[k], i, j, kk);
                        }
                    }
                } else if (figure == Queen) {
                    for (int k = 0; k < 8; k++) {
                        int kk = 1;
                        end_direction = false;

                        // Increases kk while queen can move in that direction
                        while (!end_direction && *(ptt + kk * (dir_king[k])) < OFFBOARD) {
                            append_legal_moves(dir_king[k], i, j, kk);
                            kk++;
                        }
                    }
                } else if (figure == Bishop) {
                    for (int k = 0; k < 4; k++) {
                        int kk = 1;
                        end_direction = false;
                        while (!end_direction && *(ptt + kk * (dir_bishop[k])) < OFFBOARD) {
                            append_legal_moves(dir_bishop[k], i, j, kk);
                            kk++;
                        }
                    }
                } else if (figure == Rook) {
                    for (int k = 0; k < 4; k++) {
                        int kk = 1;
                        end_direction = false;
                        while (!end_direction && *(ptt + kk * (dir_rook[k])) < OFFBOARD) {
                            append_legal_moves(dir_rook[k], i, j, kk);
                            kk++;
                        }
                    }
                }
            }
        }
    }
}

// Checks weather the move is legal. If the king is attacked then not legal.
// Decreases the legal pointer->does not store the move
void Table::is_really_legal() {
    update_table(chess->legal_moves[chess->legal_pointer], false, true);
    if (is_attacked(chess->player_to_move == Chess::WHITE
                ? chess->movelist[chess->move_number].pos_white_king
                : chess->movelist[chess->move_number].pos_black_king,
                chess->player_to_move)) {
        --chess->legal_pointer;
    }
    --chess->move_number;
}

// Co-function of append_legal_moves()
inline void Table::append_legal_moves_inner(const int dir_piece, const int i, const int j, const int kk) {
    chess->legal_pointer++;
    int move = 0;
    move |= (j - 1) * 0x2000;
    move |= (i - 2) * 0x0400;
    move |= (j - 1 + kk * conv[21 + dir_piece][0]) * 0x0020;
    move |= (i - 2 + kk * conv[21 + dir_piece][1]) * 0x0004;
    chess->legal_moves[chess->legal_pointer] = move;
    is_really_legal();
}

// Tries if a particular move is legal
void Table::append_legal_moves(const int dir_piece, const int i, const int j, const int kk) {
    // dir_piece : direction of the move
    // kk : distance
    const int tmp = i * 10 + j + kk * dir_piece;

    // No more moves in that direction if field is not empty
    if (pt[tmp] != EMPTY) {
        end_direction = true;
    }
    if (chess->player_to_move == Chess::WHITE) {
        // Tries move if field occupied by black or empty
        if (pt[tmp] > BlackColor || pt[tmp] == EMPTY) {
            append_legal_moves_inner(dir_piece, i, j, kk);
        }
    } else {
        // Tries move if field occupied by white or empty
        if (pt[tmp] < BlackColor) {
            append_legal_moves_inner(dir_piece, i, j, kk);
        }
    }
}
