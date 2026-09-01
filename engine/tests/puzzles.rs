use engine::position::Position;
use engine::search::{search, Limits};
use engine::ttable::TranspositionTable;

/// Searches `fen` to `depth` plies and asserts the move the search settles on.
fn solve(fen: &str, depth: u32, expected: &str) {
    let mut position: Position = fen.parse().expect("bad fen");
    let table = TranspositionTable::new(16);
    let found = search(&mut position, Limits::depth(depth), &table, &[])
        .best
        .expect("no move");
    assert_eq!(found.to_string(), expected, "{fen}");
}

#[test]
fn the_exchange_on_h5() {
    solve(
        "r2qk2r/ppp1bp1p/2n1b1pP/7n/3P3R/2N2N2/PP3PP1/R1BQKB2 w Qkq - 2 12",
        8,
        "h4h5",
    );
}
