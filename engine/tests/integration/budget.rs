use engine::position::Position;
use engine::search::{search, Limits};
use engine::ttable::TranspositionTable;

/// The budget is soft, so the iteration that passes it still runs to the end,
/// and the hard ceiling in the search lets it overrun by sixteen budgets at
/// most. Nothing may pass twenty.
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

/// A table warmed by a long game makes each iteration nearly free, so the soft
/// budget on its own lets the depth climb until one iteration is unaffordable
/// and the search never comes back. Cold, this position stops at depth 11 for
/// 2009 nodes; warm and unbounded it reached depth 21 for 12147, and a whole
/// game of warming ran away entirely.
#[test]
fn a_warm_table_does_not_run_away() {
    let table = TranspositionTable::new(8);
    let mut position: Position =
        "8/pp1k1p1p/1p1p1p1p/1P1P1P1P/1P1P1P1P/8/8/3K4 w - - 0 1".parse().expect("bad fen");
    for _ in 0..8 {
        search(&mut position, Limits::depth(22), &table, &[]);
    }
    let budget = 2_000;
    let report = search(&mut position, Limits::nodes(budget), &table, &[]);
    assert!(report.best.is_some());
    assert!(report.nodes < budget * OVERSHOOT, "{} nodes", report.nodes);
}
