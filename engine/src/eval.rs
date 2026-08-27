use crate::position::{Piece, Position};
use crate::search::Score;

const MATERIAL: [(Piece, Score); 5] = [
    (Piece::Pawn, 100),
    (Piece::Knight, 320),
    (Piece::Bishop, 330),
    (Piece::Rook, 500),
    (Piece::Queen, 900),
];

pub fn evaluate(position: &Position) -> Score {
    let us = position.side_to_move();
    let them = us.flip();
    MATERIAL
        .iter()
        .map(|&(piece, value)| {
            let ours = position.pieces(piece, us).count_ones() as Score;
            let theirs = position.pieces(piece, them).count_ones() as Score;
            value * (ours - theirs)
        })
        .sum()
}

