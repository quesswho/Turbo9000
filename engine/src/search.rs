use crate::movegen::{generate_all, MoveList};
use crate::moves::Move;
use crate::position::{Black, Piece, Position, Side, White};

pub type Score = i32;

pub const MATE: Score = 30_000;

const MATERIAL: [(Piece, Score); 5] = [
    (Piece::Pawn, 100),
    (Piece::Knight, 320),
    (Piece::Bishop, 330),
    (Piece::Rook, 500),
    (Piece::Queen, 900),
];

/// TODO: extremely simplified evaluation for now, move eval to eval.rs 
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

/// The best move for the side to move, or `None` when it has none.
pub fn search(position: &mut Position, depth: u32) -> Option<Move> {
    debug_assert!(depth >= 1, "a search shallower than one ply has no move");
    let mut lists = vec![MoveList::new(); depth as usize];
    if position.side_to_move().is_white() {
        root::<White>(position, depth, &mut lists)
    } else {
        root::<Black>(position, depth, &mut lists)
    }
}

fn root<Us: Side>(position: &mut Position, depth: u32, lists: &mut [MoveList]) -> Option<Move> {
    let (list, deeper) = lists.split_first_mut().unwrap();
    generate_all::<Us>(position, list);

    let mut best = None;
    let mut best_score = Score::MIN;
    for &mv in list.moves() {
        let undo = position.make_move(mv);
        let score = -negamax::<Us::Them>(position, depth - 1, 1, deeper);
        position.unmake_move(mv, undo);
        if score > best_score {
            (best, best_score) = (Some(mv), score);
        }
    }
    best
}

fn negamax<Us: Side>(
    position: &mut Position,
    depth: u32,
    ply: u32,
    lists: &mut [MoveList],
) -> Score {
    if depth == 0 {
        return evaluate(position);
    }
    
    // Every ply keeps its own movelist
    let (list, deeper) = lists.split_first_mut().unwrap();
    let in_check = generate_all::<Us>(position, list);
    if list.is_empty() {
        return if in_check { ply as Score - MATE } else { 0 };
    }

    let mut best = Score::MIN;
    for &mv in list.moves() {
        let undo = position.make_move(mv);
        let score = -negamax::<Us::Them>(position, depth - 1, ply + 1, deeper);
        position.unmake_move(mv, undo);
        best = best.max(score);
    }
    best
}
