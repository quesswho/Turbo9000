use engine::position::Position;
use engine::search::search;

fn check(fen: &str, moves: u32, expected: &str) {
    let mut position: Position = fen.parse().expect("bad fen");
    let found = search(&mut position, moves * 2).best.expect("no move");
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
