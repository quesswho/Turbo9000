use engine::position::Position;
use engine::search::{search, Limits};
use engine::ttable::TranspositionTable;

/// A position the search should find its way through: name, fen, plies to
/// search, and the move the engine is expected to settle on.
const PUZZLES: &[(&str, &str, u32, &str)] = &[(
    "the exchange on h5",
    "r2qk2r/ppp1bp1p/2n1b1pP/7n/3P3R/2N2N2/PP3PP1/R1BQKB2 w Qkq - 2 12",
    8,
    "h4h5",
)];

/// Run with `cargo test -p engine --release --test puzzles -- --ignored
/// --nocapture` to see the report; it never runs as part of the plain suite.
#[test]
#[ignore]
fn report() {
    let mut solved = 0;
    for &(name, fen, depth, expected) in PUZZLES {
        let mut position: Position = fen.parse().expect("bad fen");
        let table = TranspositionTable::new(16);
        let found = search(&mut position, Limits::depth(depth), &table, &[])
            .best
            .expect("no move")
            .to_string();
        if found == expected {
            solved += 1;
            println!("PASS {name}: found {found} (expected {expected}, depth {depth})");
        } else {
            println!("FAIL {name}: found {found} (expected {expected}, depth {depth})");
        }
    }
    println!("{solved}/{} solved", PUZZLES.len());
}