use crate::common::score_of;
use engine::movegen::find_move;
use engine::position::Position;
use engine::search::{search, Limits};
use engine::ttable::TranspositionTable;

/// A rook up, but the fifty move rule has already decided it once the clock
/// reaches ninety nine.
#[test]
fn the_fifty_move_rule_draws_an_expired_position() {
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