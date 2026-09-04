//! Verifies the Zobrist hash maintained by `make_move` / `unmake_move`.
//!
//! Two properties are checked throughout:
//! 1. The incrementally updated hash always equals a fresh `compute_hash()`.
//! 2. Making a move and unmaking it restores the exact previous hash.

use engine::movegen::{generate_all, MoveList};
use engine::moves::Move;
use engine::position::{square, Black, ColoredPiece, Position, Side, White, NO_EN_PASSANT};

// Standard perft fixtures, chosen because together they exercise captures,
// promotions, castling and en passant heavily.
const STARTPOS: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const KIWIPETE: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1";
const POSITION_3: &str = "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1";
const POSITION_4: &str = "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1";
const POSITION_5: &str = "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8";
const POSITION_6: &str = "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10";

/// The incremental hash must agree with a from-scratch recompute.
fn assert_hash_consistent(position: &Position) {
    assert_eq!(
        position.hash(),
        position.compute_hash(),
        "incremental hash diverged:\n{position}"
    );
}

/// Asserts consistency at every node visited and after every unmake, so any
/// piece move that forgets its XOR is caught somewhere in the tree.
fn walk<Us: Side>(position: &mut Position, depth: u32) {
    assert_hash_consistent(position);
    if depth == 0 {
        return;
    }
    let mut list = MoveList::new();
    generate_all::<Us>(position, &mut list);
    for &mv in list.moves() {
        let undo = position.make_move(mv);
        walk::<Us::Them>(position, depth - 1);
        position.unmake_move(mv, undo);
        assert_hash_consistent(position);
    }
}

/// Walks each fixture to a modest depth, comparing incremental against
/// recomputed hashes along the way.
#[test]
fn hash_stays_consistent_through_walks() {
    const SAMPLES: [(&str, u32); 6] = [
        (STARTPOS, 4),
        (KIWIPETE, 3),
        (POSITION_3, 4),
        (POSITION_4, 3),
        (POSITION_5, 3),
        (POSITION_6, 3),
    ];
    for &(fen, depth) in SAMPLES.iter() {
        let mut position: Position = fen.parse().expect("bad fen");
        if position.side_to_move().is_white() {
            walk::<White>(&mut position, depth);
        } else {
            walk::<Black>(&mut position, depth);
        }
    }
}

fn find_move(list: &[Move], from: u8, to: u8) -> Move {
    *list
        .iter()
        .find(|mv| mv.from() == from && mv.to() == to)
        .expect("expected move was not generated")
}

/// Plays a knight out and back on both sides, ending on the start position
/// again but via a different sequence of moves.
fn knights_out_and_back(from: u8, middle: u8, mirror_from: u8, mirror_middle: u8) -> Position {
    let mut position = Position::starting();
    let mut list = MoveList::new();

    generate_all::<White>(&position, &mut list);
    let _ = position.make_move(find_move(list.moves(), from, middle));
    generate_all::<Black>(&position, &mut list);
    let _ = position.make_move(find_move(list.moves(), mirror_from, mirror_middle));
    generate_all::<White>(&position, &mut list);
    let _ = position.make_move(find_move(list.moves(), middle, from));
    generate_all::<Black>(&position, &mut list);
    let _ = position.make_move(find_move(list.moves(), mirror_middle, mirror_from));

    position
}

/// Positions reached by different move orders (transpositions) must share one
/// hash, or a transposition table would serve stale entries.
#[test]
fn transpositions_agree() {
    // The same position built by FEN parsing and by make_move calls must hash
    // identically.
    let start = Position::starting();
    let parsed: Position = STARTPOS.parse().expect("bad fen");
    assert_eq!(start.hash(), parsed.hash());

    // 1.Nf3 Nf6 2.Ng1 Ng8 and 1.Nc3 Nc6 2.Nb1 Nb8 both return to the starting
    // placement, so their hashes must equal the start hash again.
    let via_kingside = knights_out_and_back(square(6, 0), square(5, 2), square(6, 7), square(5, 5));
    let via_queenside = knights_out_and_back(square(1, 0), square(2, 2), square(1, 7), square(2, 5));
    assert_eq!(via_kingside.hash(), start.hash());
    assert_eq!(via_queenside.hash(), start.hash());
}

/// Makes a move, checks the incremental hash against a recompute mid-way, then
/// unmakes and requires the exact original hash back.
fn round_trip(position: &mut Position, mv: Move) {
    let before = position.hash();
    let undo = position.make_move(mv);
    assert_hash_consistent(position);
    position.unmake_move(mv, undo);
    assert_eq!(position.hash(), before, "unmake did not restore:\n{position}");
}

// Bare kings and rooks, so all four castling moves are legal and nothing else
// distracts the hash updates from king plus rook and the castling key swap.
const CASTLE_FEN: &str = "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1";

/// Castling moves two pieces (king and rook) and drops castling rights in one
/// step. Each castle is round tripped for both colors.
#[test]
fn castling_round_trips() {
    // King origin and destination per side: e1 to g1/c1, e8 to g8/c8.
    const WHITE_CASTLES: [(u8, u8); 2] = [
        (square(4, 0), square(6, 0)),
        (square(4, 0), square(2, 0)),
    ];
    const BLACK_CASTLES: [(u8, u8); 2] = [
        (square(4, 7), square(6, 7)),
        (square(4, 7), square(2, 7)),
    ];

    let mut position: Position = CASTLE_FEN.parse().expect("bad fen");
    let mut list = MoveList::new();
    generate_all::<White>(&position, &mut list);
    for &(from, to) in WHITE_CASTLES.iter() {
        let mv = find_move(list.moves(), from, to);
        assert!(mv.is_castle());
        round_trip(&mut position, mv);
    }

    const BLACK_FEN: &str = "r3k2r/8/8/8/8/8/8/R3K2R b KQkq - 0 1";
    let mut position: Position = BLACK_FEN.parse().expect("bad fen");
    let mut list = MoveList::new();
    generate_all::<Black>(&position, &mut list);
    for &(from, to) in BLACK_CASTLES.iter() {
        let mv = find_move(list.moves(), from, to);
        assert!(mv.is_castle());
        round_trip(&mut position, mv);
    }
}

// White pawn on e5, black pawns on d5 and f5, and the ep square f6 from
// black's just played f7-f5, so exf6 e.p. is available at the root.
const EN_PASSANT_FEN: &str = "rnbqkbnr/ppp1p1pp/8/3pPp2/8/8/PPPP1PPP/RNBQKBNR w KQkq f6 0 3";

/// Covers both halves of en passant hashing: a double push must set the ep
/// square (and with it the ep file key), and an actual ep capture must remove
/// the victim from the square behind the target, not the target itself.
#[test]
fn en_passant_round_trips() {
    // A double push from the start position lands the ep key on e3.
    let mut position = Position::starting();
    let mut list = MoveList::new();
    generate_all::<White>(&position, &mut list);

    let mv = find_move(list.moves(), square(4, 1), square(4, 3));
    assert!(mv.is_double_push() && !mv.is_en_passant());
    let undo = position.make_move(mv);
    assert_hash_consistent(&position);
    assert_eq!(position.en_passant(), square(4, 2));
    position.unmake_move(mv, undo);

    // Now the capture itself: exf6 removes the pawn standing on f5.
    let mut position: Position = EN_PASSANT_FEN.parse().expect("bad fen");
    generate_all::<White>(&position, &mut list);
    let mv = find_move(list.moves(), square(4, 4), square(5, 5));
    assert!(mv.is_en_passant());

    let before = position.hash();
    let undo = position.make_move(mv);
    assert_hash_consistent(&position);
    assert_eq!(position.piece_at(square(5, 5)), Some(ColoredPiece::WhitePawn));
    assert_eq!(position.piece_at(square(5, 4)), None);
    position.unmake_move(mv, undo);
    assert_hash_consistent(&position);
    assert_eq!(position.hash(), before);
}

// The a7 pawn can promote quietly to a8 or by capturing the knight on b8,
// four choices each way.
const PROMOTION_FEN: &str = "1n5k/P7/8/8/8/8/8/K7 w - - 0 1";

/// Every promotion move (quiet and capture, all four pieces) must hash the
/// pawn out and the promoted piece in, and unmake must restore the pawn.
#[test]
fn promotions_round_trip() {
    let mut position: Position = PROMOTION_FEN.parse().expect("bad fen");
    let mut list = MoveList::new();
    generate_all::<White>(&position, &mut list);

    let promotions: Vec<Move> = list
        .moves()
        .iter()
        .copied()
        .filter(|mv| mv.is_promotion())
        .collect();
    assert_eq!(promotions.len(), 8, "quiet and capture promotions expected");

    for mv in promotions {
        let before = position.hash();
        let undo = position.make_move(mv);
        assert_hash_consistent(&position);
        let expected = ColoredPiece::new(mv.promoted_piece(), White::COLOR);
        assert_eq!(position.piece_at(mv.to()), Some(expected));
        position.unmake_move(mv, undo);
        assert_hash_consistent(&position);
        assert_eq!(position.hash(), before);
    }
}

/// A pass changes the side to move and clears any en passant square, and
/// unmaking it must give back the position it was made from, untouched.
#[test]
fn null_moves_round_trip() {
    for fen in [STARTPOS, KIWIPETE, EN_PASSANT_FEN, POSITION_4] {
        let mut position: Position = fen.parse().expect("bad fen");
        let before = position.clone();

        let undo = position.make_null();
        assert_hash_consistent(&position);
        assert_ne!(position.side_to_move(), before.side_to_move());
        assert_eq!(position.en_passant(), NO_EN_PASSANT);

        position.unmake_null(undo);
        assert_eq!(position, before, "unmake_null did not restore:\n{position}");
    }
}
