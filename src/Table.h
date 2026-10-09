#pragma once

#include <memory>

#define EMPTY 0x00
#define OFFBOARD 0xff

class Chess;
class Eval;

class Table {
  public:
    std::unique_ptr<Eval> eval;

    static constexpr int Pawn = 1;
    static constexpr int Knight = 2;
    static constexpr int Bishop = 3;
    static constexpr int Rook = 4;
    static constexpr int Queen = 5;
    static constexpr int King = 6;

    static constexpr int WhiteColor = 0;
    static constexpr int BlackColor = 128;

    // Material value by figure index (empty, pawn, knight, bishop, rook, queen, king)
    // Shared with Eval; used for incremental material tracking in update_table.
    static constexpr int piece_value[7] = {0, 100, 330, 330, 500, 900, 0};

    // White-perspective material delta for a piece code (field = figure | colorbit).
    // Returns +value for a white piece, -value for a black piece, 0 for empty/offboard.
    static inline int material_delta(int field) {
        if (field == 0x00 || field == 0xff) return 0;
        const int v = piece_value[field & 127];
        return (field & 128) ? -v : v;
    }

    // Possible direction of figure's move
    const int dir_rook[4] = {-10, -1, 1, 10};
    const int dir_knight[8] = {-21, -19, -12, -8, 8, 12, 19, 21};
    const int dir_bishop[4] = {-11, -9, 9, 11};
    const int dir_king[8] = {-11, -10, -9, -1, 1, 9, 10, 11};

    Table();

    void list_legal_moves(Chess &chess);
    void print_table(const Chess &chess);
    void reset_movelist(Chess &chess);
    void setboard(Chess &chess, const char *fen_position);
    bool is_attacked(const Chess &chess, const int field, const int color);
    bool is_not_enough_material(const Chess &chess);
    void update_table(Chess &chess, const int move, const bool print,
                      const bool fake = false);
    void unmake_table(Chess &chess);
    bool third_occurance(const Chess &chess);

  private:
    // Values representing the figures in the table
    static constexpr int WhitePawn = 1;
    static constexpr int WhiteKnight = 2;
    static constexpr int WhiteBishop = 3;
    static constexpr int WhiteRook = 4;
    static constexpr int WhiteQueen = 5;
    static constexpr int WhiteKing = 6;

    static constexpr int BlackPawn = 0x81;
    static constexpr int BlackKnight = 0x82;
    static constexpr int BlackBishop = 0x83;
    static constexpr int BlackRook = 0x84;
    static constexpr int BlackQueen = 0x85;
    static constexpr int BlackKing = 0x86;

    // For printing and for notation
    const int graphical_figure[14][2] = {
        {000, 46},  // "."
        {001, 80},  // "P"
        {002, 78},  // "N"
        {003, 66},  // "B"
        {004, 82},  // "R"
        {005, 81},  // "Q"
        {006, 75},  // "K"
        {129, 112}, // "p"
        {130, 110}, // "n"
        {131, 98},  // "b"
        {132, 114}, // "r"
        {133, 113}, // "q"
        {134, 107}, // "k"
        {191, 88},  // "X"
    };

    void is_really_legal(Chess &chess);
    void castling(Chess &chess);
};
