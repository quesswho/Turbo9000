use std::time::{Duration, Instant};

use engine::position::Position;
use engine::search::{search, Limits};
use engine::ttable::TranspositionTable;

fn check(fen: &str, moves: u32, expected: &str) {
    let mut position: Position = fen.parse().expect("bad fen");
    let table = TranspositionTable::new(1);
    let found = search(&mut position, Limits::depth(moves * 2), &table)
        .best
        .expect("no move");
    assert_eq!(found.to_string(), expected, "{fen}");
}

#[test]
fn back_rank() {
    check("6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1", 1, "a1a8");
}

#[test]
fn rook_and_king() {
    check("7k/8/5K2/8/8/8/8/R7 w - - 0 1", 2, "f6g6");
}

#[test]
fn rook_sacrifice() {
    check("kbK5/pp6/1P6/8/8/8/8/R6R w - - 0 1", 2, "a1a6");
}

#[test]
fn a_clock_bound_stops_the_search() {
    let mut position = Position::starting();
    let table = TranspositionTable::new(1);
    let start = Instant::now();
    let report = search(&mut position, Limits::time(Duration::from_millis(100)), &table);
    assert!(report.best.is_some());
    assert!(report.depth >= 1, "{}", report.depth);
    assert!(start.elapsed() < Duration::from_millis(500), "{:?}", start.elapsed());
}
