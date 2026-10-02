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

    // Possible direction of figure's move
    const int dir_rook[4] = {-10, -1, 1, 10};
    const int dir_knight[8] = {-21, -19, -12, -8, 8, 12, 19, 21};
    const int dir_bishop[4] = {-11, -9, 9, 11};
    const int dir_king[8] = {-11, -10, -9, -1, 1, 9, 10, 11};

    Table(Chess *chess);

    void list_legal_moves();
    void print_table();
    void reset_movelist();
    void setboard(const char *fen_position);
    bool is_attacked(const int field, const int color);
    bool is_not_enough_material();
    void update_table(const int move, const bool print, const bool fake = false);
    void unmake_table();
    bool third_occurance();

  private:
    Chess *chess;

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

    void is_really_legal();
    void castling();
};
