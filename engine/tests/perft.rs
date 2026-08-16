use engine::perft::perft;
use engine::position::Position;

fn check(fen: &str, counts: &[u64]) {
    let mut position: Position = fen.parse().expect("bad fen");
    for (index, expected) in counts.iter().enumerate() {
        let depth = index as u32 + 1;
        assert_eq!(perft(&mut position, depth), *expected, "{fen} depth {depth}");
    }
}

const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const KIWIPETE: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";
const POSITION_3: &str = "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1";
const POSITION_4: &str = "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1";
const POSITION_4_MIRRORED: &str = "r2q1rk1/pP1p2pp/Q4n2/bbp1p3/Np6/1B3NBn/pPPP1PPP/R3K2R b KQ - 0 1";
const POSITION_5: &str = "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8";
const POSITION_6: &str = "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10";

#[test]
fn startpos() {
    check(STARTPOS, &[20, 400, 8902, 197_281, 4_865_609]);
}

#[test]
fn kiwipete() {
    check(KIWIPETE, &[48, 2039, 97_862, 4_085_603]);
}

#[test]
fn position_3() {
    check(POSITION_3, &[14, 191, 2812, 43_238, 674_624, 11_030_083]);
}

#[test]
fn position_4() {
    check(POSITION_4, &[6, 264, 9467, 422_333, 15_833_292]);
    check(POSITION_4_MIRRORED, &[6, 264, 9467, 422_333, 15_833_292]);
}

#[test]
fn position_5() {
    check(POSITION_5, &[44, 1486, 62_379, 2_103_487]);
}

#[test]
fn position_6() {
    check(POSITION_6, &[46, 2079, 89_890, 3_894_594]);
}
