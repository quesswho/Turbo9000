use engine::position::Position;
use engine::search::{search, Limits};
use engine::ttable::TranspositionTable;

/// The budget is soft, so the iteration that passes it still runs to the end.
/// One iteration costs a few times the one before it, never a hundred.
const OVERSHOOT: u64 = 20;

#[test]
fn a_node_budget_stops_the_search() {
    let mut position = Position::starting();
    let table = TranspositionTable::new(1);
    let budget = 5_000;
    let report = search(&mut position, Limits::nodes(budget), &table, &[]);
    assert!(report.best.is_some());
    assert!(report.nodes >= budget, "{} nodes", report.nodes);
    assert!(report.nodes < budget * OVERSHOOT, "{} nodes", report.nodes);
}

#[test]
fn a_bigger_budget_searches_deeper() {
    let mut position = Position::starting();
    let table = TranspositionTable::new(1);
    let shallow = search(&mut position, Limits::nodes(1_000), &table, &[]).depth;
    let deep = search(&mut position, Limits::nodes(200_000), &table, &[]).depth;
    assert!(deep > shallow, "{deep} is no deeper than {shallow}");
}

/// Every completed iteration leaves a score, so a budget spent to the last
/// node still answers with the move of a full iteration rather than a
/// half searched one.
#[test]
fn a_budgeted_search_reports_the_depth_it_finished() {
    let mut position = Position::starting();
    let table = TranspositionTable::new(1);
    let report = search(&mut position, Limits::nodes(20_000), &table, &[]);
    assert!(report.depth >= 1, "{}", report.depth);
}
