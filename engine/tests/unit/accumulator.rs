use engine::movegen::{MoveList, generate_all};
use engine::nnue::evaluate;
use engine::position::{Black, Position, Side, White};

/// The incremental updates have to leave behind whatever a full refresh would
/// have built, at every node and after every move is taken back again.
fn check<Us: Side>(position: &mut Position, depth: u32) {
    let mut refreshed = position.clone();
    refreshed.refresh_accumulator();
    assert_eq!(
        position.accumulator(),
        refreshed.accumulator(),
        "{}",
        position.to_fen()
    );

    if depth == 0 {
        return;
    }

    let mut list = MoveList::new();
    generate_all::<Us>(position, &mut list);
    for &mv in list.moves() {
        let undo = position.make_move(mv);
        check::<Us::Them>(position, depth - 1);
        position.unmake_move(mv, undo);
    }
}

fn walk(fen: &str, depth: u32) {
    let mut position: Position = fen.parse().expect("bad fen");
    if position.side_to_move().is_white() {
        check::<White>(&mut position, depth);
    } else {
        check::<Black>(&mut position, depth);
    }
}

/// Castling, king walks and promotions, the moves that move a king out of its
/// bucket or add a piece under a stale one.
#[test]
fn the_incremental_accumulator_matches_a_refresh() {
    walk("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1", 2);
    walk("8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 b - - 0 1", 3);
    walk("n1n5/PPPk4/8/8/8/8/4Kppp/5N1N b - - 0 1", 2);
}

/// The features are mirrored onto the king's file, so a position and its
/// horizontal mirror share every feature and evaluate the same.
#[test]
fn a_horizontal_mirror_evaluates_the_same() {
    const PAIRS: [(&str, &str); 4] = [
        ("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w - - 0 1",
         "rnbkqbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBKQBNR w - - 0 1"),
        ("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w - - 0 1",
         "r2k3r/1bpqpp1p/1pnp2nb/3NP3/3P2p1/p1Q2N2/PPPBBPPP/R2K3R w - - 0 1"),
        ("8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 b - - 0 1",
         "8/5p2/4p3/r5PK/k1p3R1/8/1P1P4/8 b - - 0 1"),
        ("8/8/8/4k3/8/8/4K3/4R3 w - - 0 1",
         "8/8/8/3k4/8/8/3K4/3R4 w - - 0 1"),
    ];

    for (fen, mirrored) in PAIRS {
        let position: Position = fen.parse().expect("bad fen");
        let mirrored: Position = mirrored.parse().expect("bad fen");
        assert_eq!(evaluate(&position), evaluate(&mirrored), "{fen}");
    }
}
