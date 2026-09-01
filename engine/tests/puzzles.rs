use engine::position::Position;
use engine::search::{search, Limits, Score};
use engine::ttable::TranspositionTable;

/// How a puzzle is judged: either the move the search must settle on, or a
/// score it must reach.
#[derive(Clone, Copy)]
enum Solve {
    Move(&'static str),
    Score(Score),
}

/// A position the search should find its way through: name, fen, plies to
/// search, and the expectation. Report-only, so a puzzle the engine cannot
/// solve yet is data rather than a failure.
const PUZZLES: &[(&str, &str, u32, Solve)] = &[
    (
        "the exchange on h5",
        "r2qk2r/ppp1bp1p/2n1b1pP/7n/3P3R/2N2N2/PP3PP1/R1BQKB2 w Qkq - 2 12",
        8,
        Solve::Move("h4h5"),
    ),
    (
        "a rook up inside the clock",
        "8/8/8/4k3/8/8/4K3/4R3 w - - 0 60",
        4,
        Solve::Score(300),
    ),
];

/// Run with `cargo test -p engine --release --test puzzles -- --ignored
/// --nocapture` to see the report; it never runs as part of the plain suite.
#[test]
#[ignore]
fn report() {
    let mut solved = 0;
    for &(name, fen, depth, solve) in PUZZLES {
        let mut position: Position = fen.parse().expect("bad fen");
        let table = TranspositionTable::new(16);
        let report = search(&mut position, Limits::depth(depth), &table, &[]);
        let pass = match solve {
            Solve::Move(expected) => report
                .best
                .is_some_and(|mv| mv.to_string() == expected),
            Solve::Score(threshold) => report.score > threshold,
        };
        let detail = match solve {
            Solve::Move(expected) => format!(
                "found {:?}, expected {expected}",
                report.best.map(|mv| mv.to_string())
            ),
            Solve::Score(threshold) => format!("score {}, expected > {threshold}", report.score),
        };
        if pass {
            solved += 1;
            println!("PASS {name}: {detail} (depth {depth})");
        } else {
            println!("FAIL {name}: {detail} (depth {depth})");
        }
    }
    println!("{solved}/{} solved", PUZZLES.len());
}