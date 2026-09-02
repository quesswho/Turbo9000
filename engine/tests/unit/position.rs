use engine::position::Position;

fn roundtrip(fen: &str) {
    let position: Position = fen.parse().expect("bad fen");
    assert_eq!(position.to_fen(), fen);
}

#[test]
fn a_fen_survives_a_roundtrip() {
    roundtrip("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
    roundtrip("rnbqkbnr/pp1ppppp/8/2p5/4P3/8/PPPP1PPP/RNBQKBNR w KQkq c6 0 1");
    roundtrip("8/8/8/4k3/8/8/4K3/8 b - - 13 1");
    roundtrip("r3k2r/8/8/8/8/8/8/R3K2R w Qk - 99 1");
}

#[test]
fn the_starting_position_writes_the_standard_fen() {
    assert_eq!(
        Position::starting().to_fen(),
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
    );
}
