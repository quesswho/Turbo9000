use std::time::{Duration, Instant};

use engine::movegen::find_move;
use engine::position::Position;
use engine::search::{search, Limits};
use engine::ttable::TranspositionTable;

fn check(fen: &str, moves: u32, expected: &str) {
    let mut position: Position = fen.parse().expect("bad fen");
    let table = TranspositionTable::new(1);
    let found = search(&mut position, Limits::depth(moves * 2), &table, &[])
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

fn score_of(fen: &str, depth: u32) -> i32 {
    let mut position: Position = fen.parse().expect("bad fen");
    let table = TranspositionTable::new(1);
    search(&mut position, Limits::depth(depth), &table, &[]).score
}

/// A rook up, but the fifty move rule lands before it can be converted.
#[test]
fn the_fifty_move_rule_is_a_draw() {
    assert!(score_of("8/8/8/4k3/8/8/4K3/4R3 w - - 0 60", 4) > 300);
    assert_eq!(score_of("8/8/8/4k3/8/8/4K3/4R3 w - - 99 60", 4), 0);
}

/// After the shuffle white is a queen down with `Ka1b1` as its only legal
/// move, and that move returns to a position already played. Salvaging the
/// draw depends on seeing the repetition, so the score is 0 and not -900.
#[test]
fn a_repetition_is_scored_as_a_draw() {
    let start = "7k/8/8/8/8/8/6q1/1K6 b - - 9 40";
    let mut position: Position = start.parse().expect("bad fen");
    let mut history = Vec::new();
    for text in ["h8h7", "b1a1", "h7h8"] {
        let mv = find_move(&position, text).expect("illegal setup move");
        history.push(position.hash());
        position.make_move(mv);
    }

    let table = TranspositionTable::new(1);
    let report = search(&mut position, Limits::depth(2), &table, &history);
    assert_eq!(report.best.expect("no move").to_string(), "a1b1");
    assert_eq!(report.score, 0, "missed the repetition and took the loss");
}

#[test]
fn a_clock_bound_stops_the_search() {
    let mut position = Position::starting();
    let table = TranspositionTable::new(1);
    let start = Instant::now();
    let report = search(&mut position, Limits::time(Duration::from_millis(100)), &table, &[]);
    assert!(report.best.is_some());
    assert!(report.depth >= 1, "{}", report.depth);
    assert!(start.elapsed() < Duration::from_millis(500), "{:?}", start.elapsed());
}
