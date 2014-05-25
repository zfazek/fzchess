#ifndef EVAULATE_H
#define EVAULATE_H

//Values for evaluation
//empty, pawn, knight, bishop, rook, queen, king
int figure_value[7] = {0, 100, 330, 330, 500, 900, 0};
int end_game_threshold = 1200;
int king_castled       =   40;
int cant_castle        =  -10;
int double_bishops     =   50;
int double_pawn        =   15;
int friendly_pawn      =    5;
int pawn_advantage     =   10;




#endif
