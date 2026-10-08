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
        chess->movelist[n].figure_moved = 0;
        chess->movelist[n].ep_capture_sq = 0;
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
    }
    // Fill the single board once
    for (int i = 0; i < 120; i++) {
        chess->board[i] = new_table[i];
    }
    chess->move_number = 0;
    // Compute the starting Zobrist key once; update_table() will maintain it incrementally
    const uint64_t start_key = chess->compute_zobrist_key(0);
    // Compute the starting White-perspective material sum once
    int start_material = 0;
    for (int i = 20; i < 100; ++i) {
        start_material += material_delta(chess->board[i]);
    }
    for (int n = 0; n < MAX_MOVES; n++) {
        chess->movelist[n].zobrist_key = start_key;
        chess->movelist[n].material_wp = start_material;
    }
    chess->rebuild_pawn_list();
}

void Table::update_table(const int move, const bool print, const bool fake) {

    // Get previous position state
    struct position_t *pm1 = chess->movelist + chess->move_number;
    chess->move_number++;
    struct position_t *pm2 = chess->movelist + chess->move_number;
    int *pt2 = chess->board; // single board, mutated in place

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
    pm2->figure_moved = figure_from;
    pm2->ep_capture_sq = 0;

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

        // En passant capture: the pawn moves diagonally onto the en_passant target
        // square (which is empty), and an enemy pawn sits on the square it passed.
        // We verify the enemy pawn is actually present to reject stale/coincidental
        // en_passant values that merely match a normal capture's geometry.
        if (pm1->en_passant > 1 && figure_to == EMPTY && square_to == pm1->en_passant) {
            if (figure_from == WhitePawn &&
                (square_to - square_from == 9 || square_to - square_from == 11) &&
                *(pt2 + square_to - 10) == BlackPawn) {
                pm2->captured_figure = BlackPawn;
                pm2->ep_capture_sq = square_to - 10;
                *(pt2 + square_to - 10) = EMPTY;
            } else if (figure_from == BlackPawn &&
                       (square_from - square_to == 9 || square_from - square_to == 11) &&
                       *(pt2 + square_to + 10) == WhitePawn) {
                pm2->captured_figure = WhitePawn;
                pm2->ep_capture_sq = square_to + 10;
                *(pt2 + square_to + 10) = EMPTY;
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
        if (pm2->ep_capture_sq != 0) {
            // This was an en passant capture — the pawn was removed from a different square
            if (figure_from == WhitePawn) {
                xor_piece(pm2->ep_capture_sq, BlackPawn);
            } else if (figure_from == BlackPawn) {
                xor_piece(pm2->ep_capture_sq, WhitePawn);
            }
        }

        pm2->zobrist_key = key;
    }
    // --- End incremental Zobrist update ---

    // --- Incremental pawn-list update (both fake and real moves, so unmake stays balanced) ---
    {
        const bool from_is_pawn = (figure_from == WhitePawn || figure_from == BlackPawn);
        const int cap = pm2->captured_figure;
        const bool cap_is_pawn = (cap == WhitePawn || cap == BlackPawn);

        // 1. Remove a captured pawn first.
        if (cap_is_pawn) {
            if (pm2->ep_capture_sq != 0) {
                chess->pawn_remove(pm2->ep_capture_sq); // en passant: pawn beside square_to
            } else {
                chess->pawn_remove(square_to);          // normal capture on square_to
            }
        }
        // 2. The moving pawn.
        if (from_is_pawn) {
            if (pm2->promotion != 0) {
                chess->pawn_remove(square_from);        // promoted: no longer a pawn
            } else {
                chess->pawn_move(square_from, square_to);
            }
        }
    }
    // --- End pawn-list update ---

    // --- Incremental material (White-perspective) update ---
    // Start from parent, subtract any captured piece, apply promotion delta.
    if (!fake) {
        int mat = pm1->material_wp;

        // Captured piece removed from the board (regular or en passant).
        // pm2->captured_figure holds the captured piece code (0 if none).
        mat -= material_delta(pm2->captured_figure);

        // Promotion: the moving pawn became the promoted piece on square_to.
        // material gains (promoted_value - pawn_value) for the moving side.
        if (pm2->promotion != 0) {
            const int promoted_piece = *(pt2 + square_to); // final piece value at destination
            // figure_from is the pawn that moved (WhitePawn/BlackPawn)
            mat -= material_delta(figure_from);     // remove the pawn's value
            mat += material_delta(promoted_piece);  // add the promoted piece's value
        }

        pm2->material_wp = mat;
    }
    // --- End incremental material update ---

    if (print) {
        print_table();
    }
}

// Reverses the last move made by update_table(), restoring the board to its prior state.
// Decrements move_number as the last step.
void Table::unmake_table() {
    const struct position_t& pm = chess->movelist[chess->move_number];
    int *board = chess->board;
    const int sq_from = pm.move_from;
    const int sq_to   = pm.move_to;

    // Restore the moving piece (or pawn if promotion) to sq_from
    if (pm.promotion != 0) {
        // Before promotion it was a pawn; figure_moved stores the original pawn
        board[sq_from] = pm.figure_moved;
    } else {
        // The piece currently at sq_to is what moved
        board[sq_from] = board[sq_to];
    }

    // Restore sq_to: the captured piece (or EMPTY for quiet moves)
    // For en passant, the captured pawn was NOT at sq_to — ep_capture_sq tells us where
    if (pm.ep_capture_sq != 0) {
        // En passant: destination square is empty after unmake, captured pawn goes back
        board[sq_to] = EMPTY;
        board[pm.ep_capture_sq] = pm.captured_figure;
    } else {
        board[sq_to] = pm.captured_figure;
    }

    // Castling: put the rook back
    if ((board[sq_from] & 127) == King) {
        // White short castle: king e1->g1 (25->27), rook was moved h1->f1 (28->26)
        if (sq_from == 25 && sq_to == 27) {
            board[28] = WhiteRook;
            board[26] = EMPTY;
        }
        // White long castle: king e1->c1 (25->23), rook was moved a1->d1 (21->24)
        else if (sq_from == 25 && sq_to == 23) {
            board[21] = WhiteRook;
            board[24] = EMPTY;
        }
        // Black short castle: king e8->g8 (95->97), rook was moved h8->f8 (98->96)
        else if (sq_from == 95 && sq_to == 97) {
            board[98] = BlackRook;
            board[96] = EMPTY;
        }
        // Black long castle: king e8->c8 (95->93), rook was moved a8->d8 (91->94)
        else if (sq_from == 95 && sq_to == 93) {
            board[91] = BlackRook;
            board[94] = EMPTY;
        }
    }

    // --- Reverse the incremental pawn-list update (exact inverse of update_table,
    //     applied in reverse order: undo the moving pawn first, then re-add captured) ---
    {
        const int moved = pm.figure_moved;
        const bool from_is_pawn = (moved == WhitePawn || moved == BlackPawn);
        const int cap = pm.captured_figure;
        const bool cap_is_pawn = (cap == WhitePawn || cap == BlackPawn);

        // 1. Undo the moving pawn.
        if (from_is_pawn) {
            if (pm.promotion != 0) {
                chess->pawn_add(sq_from);               // pawn reappears at origin
            } else {
                chess->pawn_move(sq_to, sq_from);       // move back
            }
        }
        // 2. Re-add the captured pawn.
        if (cap_is_pawn) {
            if (pm.ep_capture_sq != 0) {
                chess->pawn_add(pm.ep_capture_sq);
            } else {
                chess->pawn_add(sq_to);
            }
        }
    }
    // --- End reverse pawn-list update ---

    --chess->move_number;
}

// Prints the table and the parameters to the console
void Table::print_table() {
    for (int i = 11; i >= 0; i--) {
        for (int j = 0; j < 10; j++) {
            const int field = chess->board[i * 10 + j];
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
    chess->move_number = 1;
    while (input[n] != ' ') {
        if (input[n] > '0' && input[n] < '9') {
            for (int m = 0; m < input[n] - '0'; m++) {
                chess->board[y * 10 + x] = EMPTY;
                x++;
            }
            --x;
        }
        if (input[n] != '/') {
            for (int m = 0; m < 14; m++) {
                if (input[n] == graphical_figure[m][1]) {
                    chess->board[y * 10 + x] =
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
        chess->movelist[chess->move_number].color = Chess::WHITE;
    } else {
        chess->player_to_move = Chess::BLACK;
        chess->FZChess = Chess::BLACK;
        chess->movelist[chess->move_number].color = Chess::BLACK;
    }
    n++;
    n++;
    chess->movelist[chess->move_number].castle = 0;
    while (input[n] != ' ' && input[n] != '\0' && input[n] != '\n') {
        if (input[n] == '-') {
            chess->movelist[chess->move_number].castle = 0;
        }
        if (input[n] == 'K') {
            chess->movelist[chess->move_number].castle =
                chess->movelist[chess->move_number].castle | 1;
        }
        if (input[n] == 'Q') {
            chess->movelist[chess->move_number].castle =
                chess->movelist[chess->move_number].castle | 2;
        }
        if (input[n] == 'k') {
            chess->movelist[chess->move_number].castle =
                chess->movelist[chess->move_number].castle | 4;
        }
        if (input[n] == 'q') {
            chess->movelist[chess->move_number].castle =
                chess->movelist[chess->move_number].castle | 8;
        }
        n++;
    }
    chess->movelist[chess->move_number - 1].castle =
        chess->movelist[chess->move_number].castle;
    n++;
    if (input[n] == '-') {
        chess->movelist[chess->move_number].en_passant = 0;
    }
    if (input[n] >= 'a' &&
        input[n] <= 'h') { // en passant possible, target square
        chess->movelist[chess->move_number].en_passant =
            1 + input[n] - 'a' + (input[n + 1] - '1' + 2) * 10;
        n++;
    }
    // Advance past the en passant field and the single separating space, but
    // stop at end-of-string. A FEN may legally omit the halfmove/fullmove
    // counters (e.g. "... w - -"); without this guard the loop below scanned
    // past the '\0' into arbitrary memory, leaving the engine unresponsive.
    if (input[n] != '\0' && input[n] != '\n') {
        n++; // past the en passant char
    }
    if (input[n] == ' ') {
        n++; // past the separating space
    }
    chess->movelist[chess->move_number].not_pawn_move = 0;
    int factor = 1;
    while (input[n] != ' ' && input[n] != '\0' && input[n] != '\n') {
        chess->movelist[chess->move_number].not_pawn_move =
            chess->movelist[chess->move_number].not_pawn_move * factor +
            (int)(input[n] - '0');
        factor = factor * 10;
        n++;
    }
    int white_bishop = 0;
    int black_bishop = 0;
    for (int i = 0; i < 120; ++i) {
        if ((chess->board[i] & 127) == Bishop) {
            if (chess->board[i] == WhiteBishop) {
                white_bishop++;
            }
            if (chess->board[i] == BlackBishop) {
                black_bishop++;
            }
        }
        if ((chess->board[i] & 127) == King) {
            if (chess->board[i] == WhiteKing) {
                chess->movelist[chess->move_number].pos_white_king = i;
            }
            if (chess->board[i] == BlackKing) {
                chess->movelist[chess->move_number].pos_black_king = i;
            }
        }
    }
    if (white_bishop < 2) {
        chess->movelist[chess->move_number].white_double_bishops = 0;
    }
    if (black_bishop < 2) {
        chess->movelist[chess->move_number].black_double_bishops = 0;
    }
    // Rebuild the pawn list for the parsed FEN board before applying any moves;
    // update_table keeps it in sync incrementally from here on.
    chess->rebuild_pawn_list();
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
    // (Re)compute the Zobrist key and material for the final position after all FEN
    // moves are applied. update_table() maintains both incrementally from here on.
    chess->movelist[chess->move_number].zobrist_key =
        chess->compute_zobrist_key(chess->move_number);
    {
        int mat = 0;
        for (int i = 20; i < 100; ++i) {
            mat += material_delta(chess->board[i]);
        }
        chess->movelist[chess->move_number].material_wp = mat;
    }
    print_table();
}

// Returns if the figure of color is attacked or not
bool Table::is_attacked(const int field, const int color) {
    const int * const b = chess->board;

    // Piece values for the attacking side (opposite of `color`)
    int Knight, Bishop, Rook, Queen, King;
    if (color == Chess::WHITE) {
        // White king is at `field` — check if attacked by Black
        Knight  = BlackKnight;
        Bishop  = BlackBishop;
        Rook    = BlackRook;
        Queen   = BlackQueen;
        King    = BlackKing;
        // Pawn attack squares (black pawn attacks from above: field+9, field+11)
        if (__builtin_expect(b[field + 9] == BlackPawn || b[field + 11] == BlackPawn, 0))
            return true;
    } else {
        // Black king is at `field` — check if attacked by White
        Knight  = WhiteKnight;
        Bishop  = WhiteBishop;
        Rook    = WhiteRook;
        Queen   = WhiteQueen;
        King    = WhiteKing;
        if (__builtin_expect(b[field - 9] == WhitePawn || b[field - 11] == WhitePawn, 0))
            return true;
    }

    // Rook/Queen rays (4 directions: up, down, left, right)
    {
        int sq;
        // direction +10
        sq = field + 10;
        while (b[sq] == EMPTY) sq += 10;
        if (__builtin_expect(b[sq] == Rook || b[sq] == Queen, 0)) return true;
        if (sq == field + 10 && b[sq] == King) return true;
        // direction -10
        sq = field - 10;
        while (b[sq] == EMPTY) sq -= 10;
        if (__builtin_expect(b[sq] == Rook || b[sq] == Queen, 0)) return true;
        if (sq == field - 10 && b[sq] == King) return true;
        // direction +1
        sq = field + 1;
        while (b[sq] == EMPTY) sq += 1;
        if (__builtin_expect(b[sq] == Rook || b[sq] == Queen, 0)) return true;
        if (sq == field + 1 && b[sq] == King) return true;
        // direction -1
        sq = field - 1;
        while (b[sq] == EMPTY) sq -= 1;
        if (__builtin_expect(b[sq] == Rook || b[sq] == Queen, 0)) return true;
        if (sq == field - 1 && b[sq] == King) return true;
    }

    // Bishop/Queen rays (4 diagonal directions)
    {
        int sq;
        // direction +11
        sq = field + 11;
        while (b[sq] == EMPTY) sq += 11;
        if (__builtin_expect(b[sq] == Bishop || b[sq] == Queen, 0)) return true;
        if (sq == field + 11 && b[sq] == King) return true;
        // direction -11
        sq = field - 11;
        while (b[sq] == EMPTY) sq -= 11;
        if (__builtin_expect(b[sq] == Bishop || b[sq] == Queen, 0)) return true;
        if (sq == field - 11 && b[sq] == King) return true;
        // direction +9
        sq = field + 9;
        while (b[sq] == EMPTY) sq += 9;
        if (__builtin_expect(b[sq] == Bishop || b[sq] == Queen, 0)) return true;
        if (sq == field + 9 && b[sq] == King) return true;
        // direction -9
        sq = field - 9;
        while (b[sq] == EMPTY) sq -= 9;
        if (__builtin_expect(b[sq] == Bishop || b[sq] == Queen, 0)) return true;
        if (sq == field - 9 && b[sq] == King) return true;
    }

    // Knight attacks (8 squares)
    if (__builtin_expect(
        b[field - 21] == Knight || b[field - 19] == Knight ||
        b[field - 12] == Knight || b[field -  8] == Knight ||
        b[field +  8] == Knight || b[field + 12] == Knight ||
        b[field + 19] == Knight || b[field + 21] == Knight, 0))
        return true;

    return false;
}

// Checks not enough material
bool Table::is_not_enough_material() {
    int white_knight = 0;
    int white_bishop = 0;
    int black_knight = 0;
    int black_bishop = 0;
    for (int i = 20; i < 100; i++) {
        int c = chess->board[i];
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
    const int *t = chess->board;
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
    const uint64_t current_key = chess->movelist[chess->move_number].zobrist_key;
    int i = chess->move_number - 2;
    while (i >= 0 && occurance < 2) {
        if (chess->movelist[i].zobrist_key == current_key) {
            occurance++;
        }
        i -= 2;
    }
    return (occurance >= 2);
}

// Searches and stores all the legal moves
void Table::list_legal_moves() {
    chess->legal_pointer = -1;
    const int * const b = chess->board;
    const int ptm = chess->player_to_move;
    const int my_color  = (ptm == Chess::WHITE) ? WhiteColor : BlackColor;
    const int opp_color = (ptm == Chess::WHITE) ? BlackColor : WhiteColor;
    const int en_pass   = chess->movelist[chess->move_number].en_passant;

    // Encode a move from two square indices + optional promotion bits
    // sq: board index (21-98), x = sq%10-1, y = sq/10-2
    auto encode = [](int sf, int st, int promo = 0) -> int {
        const int xf = sf % 10 - 1, yf = sf / 10 - 2;
        const int xt = st % 10 - 1, yt = st / 10 - 2;
        return (xf << 13) | (yf << 10) | (xt << 5) | (yt << 2) | promo;
    };

    // Try adding a move; drop it if it leaves own king in check
    auto try_move = [&](int sf, int st, int promo = 0) {
        chess->legal_pointer++;
        chess->legal_moves[chess->legal_pointer] = encode(sf, st, promo);
        is_really_legal();
    };

    // A square is a legal destination if it's empty or holds an opponent piece
    auto can_land = [&](int t) -> bool {
        if (t < 21 || t > 98) return false;
        const int v = b[t];
        return v == EMPTY || ((v & 128) == opp_color && v != OFFBOARD);
    };

    // Sliding ray: walk sq+=dir, add quiet moves, then try capture
    auto slide = [&](int sf, int dir) {
        int sq = sf + dir;
        while (b[sq] == EMPTY) { try_move(sf, sq); sq += dir; }
        if (b[sq] != OFFBOARD && (b[sq] & 128) == opp_color) try_move(sf, sq); // capture
    };

    for (int sq = 21; sq <= 98; ++sq) {
        const int field = b[sq];
        if (field == EMPTY || field == OFFBOARD) continue;
        if ((field & 128) != my_color) continue;
        const int figure = field & 127;

        if (figure == Pawn) {
            if (ptm == Chess::WHITE) {
                // Push
                if (b[sq + 10] == EMPTY) {
                    if (sq / 10 == 8) { // rank 7 -> promotion
                        try_move(sq, sq + 10, 0x0200); // queen
                        try_move(sq, sq + 10, 0x0100); // rook
                        try_move(sq, sq + 10, 0x0002); // bishop
                        try_move(sq, sq + 10, 0x0001); // knight
                    } else {
                        try_move(sq, sq + 10);
                        if (sq / 10 == 3 && b[sq + 20] == EMPTY) // double push from rank 2
                            try_move(sq, sq + 20);
                    }
                }
                // Captures
                for (int d : {9, 11}) {
                    const int cap = sq + d;
                    if ((b[cap] & 128) == BlackColor && b[cap] != OFFBOARD) {
                        if (sq / 10 == 8) {
                            try_move(sq, cap, 0x0200);
                            try_move(sq, cap, 0x0100);
                            try_move(sq, cap, 0x0002);
                            try_move(sq, cap, 0x0001);
                        } else {
                            try_move(sq, cap);
                        }
                    }
                    // En passant
                    if (en_pass && en_pass == sq + d)
                        try_move(sq, cap);
                }
            } else { // BLACK
                // Push
                if (b[sq - 10] == EMPTY) {
                    if (sq / 10 == 3) { // rank 2 -> promotion
                        try_move(sq, sq - 10, 0x0200);
                        try_move(sq, sq - 10, 0x0100);
                        try_move(sq, sq - 10, 0x0002);
                        try_move(sq, sq - 10, 0x0001);
                    } else {
                        try_move(sq, sq - 10);
                        if (sq / 10 == 8 && b[sq - 20] == EMPTY) // double push from rank 7
                            try_move(sq, sq - 20);
                    }
                }
                // Captures
                for (int d : {9, 11}) {
                    const int cap = sq - d;
                    if (b[cap] > 0 && b[cap] < BlackColor) { // white piece
                        if (sq / 10 == 3) {
                            try_move(sq, cap, 0x0200);
                            try_move(sq, cap, 0x0100);
                            try_move(sq, cap, 0x0002);
                            try_move(sq, cap, 0x0001);
                        } else {
                            try_move(sq, cap);
                        }
                    }
                    // En passant
                    if (en_pass > 1 && en_pass == sq - d)
                        try_move(sq, cap);
                }
            }
        } else if (figure == Knight) {
            for (int d : {-21, -19, -12, -8, 8, 12, 19, 21}) {
                if (can_land(sq + d)) try_move(sq, sq + d);
            }
        } else if (figure == King) {
            castling();
            for (int d : {-11, -10, -9, -1, 1, 9, 10, 11}) {
                if (can_land(sq + d)) try_move(sq, sq + d);
            }
        } else if (figure == Queen) {
            for (int d : {-11, -10, -9, -1, 1, 9, 10, 11}) slide(sq, d);
        } else if (figure == Rook) {
            for (int d : {-10, -1, 1, 10}) slide(sq, d);
        } else if (figure == Bishop) {
            for (int d : {-11, -9, 9, 11}) slide(sq, d);
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
    unmake_table();
}

