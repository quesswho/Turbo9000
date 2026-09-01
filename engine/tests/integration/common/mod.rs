use engine::position::Position;
use engine::search::{search, Limits};
use engine::ttable::TranspositionTable;

/// Searches `fen` for `moves` white moves and asserts the move the search
/// settles on is `expected`.
pub fn check(fen: &str, moves: u32, expected: &str) {
    let mut position: Position = fen.parse().expect("bad fen");
    let table = TranspositionTable::new(1);
    let found = search(&mut position, Limits::depth(moves * 2), &table, &[])
        .best
        .expect("no move");
    assert_eq!(found.to_string(), expected, "{fen}");
}

/// The score the search assigns `fen` at `depth` plies.
pub fn score_of(fen: &str, depth: u32) -> i32 {
    let mut position: Position = fen.parse().expect("bad fen");
    let table = TranspositionTable::new(1);
    search(&mut position, Limits::depth(depth), &table, &[]).score
}