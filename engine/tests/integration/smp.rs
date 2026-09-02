use engine::position::Position;
use engine::search::{search, Limits, MATE};
use engine::ttable::TranspositionTable;

fn best_move(fen: &str, moves: u32, threads: usize) -> String {
    let mut position: Position = fen.parse().expect("bad fen");
    let table = TranspositionTable::new(1);
    search(&mut position, Limits::depth(moves * 2).threads(threads), &table, &[])
        .best
        .expect("no move")
        .to_string()
}

fn score_of(fen: &str, depth: u32, threads: usize) -> i32 {
    let mut position: Position = fen.parse().expect("bad fen");
    let table = TranspositionTable::new(1);
    search(&mut position, Limits::depth(depth).threads(threads), &table, &[]).score
}

fn nodes(fen: &str, depth: u32, threads: usize) -> u64 {
    let mut position: Position = fen.parse().expect("bad fen");
    let table = TranspositionTable::new(1);
    search(&mut position, Limits::depth(depth).threads(threads), &table, &[]).nodes
}

#[test]
fn several_threads_find_the_same_forced_mates() {
    assert_eq!(best_move("6k1/5ppp/8/8/8/8/8/R5K1 w - - 0 1", 1, 4), "a1a8");
    assert_eq!(score_of("7k/8/5K2/8/8/8/8/R7 w - - 0 1", 4, 4), MATE - 3);
}

#[test]
fn a_clock_bound_stops_every_thread() {
    use std::time::{Duration, Instant};
    let mut position = Position::starting();
    let table = TranspositionTable::new(1);
    let start = Instant::now();
    let report = search(
        &mut position,
        Limits::time(Duration::from_millis(100)).threads(4),
        &table,
        &[],
    );
    assert!(report.best.is_some());
    assert!(report.depth >= 1, "{}", report.depth);
    assert!(start.elapsed() < Duration::from_millis(500), "{:?}", start.elapsed());
}

/// Each thread searches its own tree, so more threads mean more nodes at a
/// fixed depth.
#[test]
fn several_threads_search_more_nodes() {
    let fen = "r1bqkbnr/pppp1ppp/2n5/4p3/2B1P3/5N2/PPPP1PPP/RNBQK2R w KQkq - 4 4";
    let one = nodes(fen, 6, 1);
    let four = nodes(fen, 6, 4);
    assert!(four > one, "{four} nodes on four threads, {one} on one");
}