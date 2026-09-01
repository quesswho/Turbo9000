use std::time::{Duration, Instant};

use engine::position::Position;
use engine::search::{search, Limits};
use engine::ttable::TranspositionTable;

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