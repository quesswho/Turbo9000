use engine::position::Position;
use engine::search::{search, Limits};
use engine::ttable::TranspositionTable;

#[test]
fn self_play_does_not_shuffle_into_a_repetition() {
    let mut position = Position::starting();
    let table = TranspositionTable::new(1);
    let mut history = Vec::new();

    for ply in 0..60 {
        let hash = position.hash();
        let seen = history.iter().filter(|&&past| past == hash).count();
        assert!(seen < 2, "threefold repetition after {ply} plies");
        let mv = search(&mut position, Limits::depth(4), &table, &history)
            .best
            .expect("no move");
        history.push(hash);
        position.make_move(mv);
    }
}