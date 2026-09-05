use engine::movegen::{find_move, generate_all, see_ge, MoveList, PIECE_VALUES};
use engine::moves::Move;
use engine::position::{Piece, Position, Square};

/// Asserts the exchange on `text` is worth exactly `value`, by pinning it from
/// both sides.
fn see_is(fen: &str, text: &str, value: i32) {
    let position: Position = fen.parse().expect("bad fen");
    let mv = find_move(&position, text).expect("move is not legal here");
    assert!(see_ge(&position, mv, value), "{fen}: {text} is below {value}");
    assert!(
        !see_ge(&position, mv, value + 1),
        "{fen}: {text} reaches {}",
        value + 1
    );
}

#[test]
fn an_undefended_pawn_is_won_outright() {
    see_is("4k3/8/8/8/3p4/8/8/3RK3 w - - 0 1", "d1d4", 100);
}

#[test]
fn a_rook_does_not_take_a_pawn_a_pawn_defends() {
    see_is("4k3/8/8/2p5/3p4/8/8/3RK3 w - - 0 1", "d1d4", -400);
}

#[test]
fn pawn_takes_pawn_and_is_taken_back() {
    see_is("4k3/8/8/2p5/3p4/4P3/8/4K3 w - - 0 1", "e3d4", 0);
}

#[test]
fn a_battery_behind_the_rook_joins_the_exchange() {
    see_is("3rk3/8/8/8/3p4/8/3R4/3QK3 w - - 0 1", "d2d4", 100);
}

#[test]
fn the_king_may_not_take_into_a_defended_square() {
    see_is("4k3/8/8/2p5/3p4/8/8/3K4 w - - 0 1", "d1d2", 0);
}

#[test]
fn en_passant_takes_the_pawn_beside_the_square() {
    see_is("4k3/8/8/3pP3/8/8/8/4K3 w - d6 0 1", "e5d6", 100);
}

#[test]
fn a_defended_en_passant_capture_is_even() {
    see_is("4k3/2p5/8/3pP3/8/8/8/4K3 w - d6 0 1", "e5d6", 0);
}

/// The canonical pair from the public engine test suites.
#[test]
fn a_rook_wins_the_loose_pawn() {
    see_is("1k1r4/1pp4p/p7/4p3/8/P5P1/1PP4P/2K1R3 w - - 0 1", "e1e5", 100);
}

#[test]
fn a_knight_into_a_defended_pawn_drops_a_piece() {
    see_is(
        "1k1r3q/1ppn3p/p4b2/4p3/8/P2N2P1/1PP1R1BP/2K1Q3 w - - 0 1",
        "d3e5",
        -220,
    );
}

fn ray(occ: u64, from: i32, df: i32, dr: i32) -> Option<u8> {
    let (mut f, mut r) = (from % 8 + df, from / 8 + dr);
    while (0..8).contains(&f) && (0..8).contains(&r) {
        let sq = (r * 8 + f) as u8;
        if occ & (1u64 << sq) != 0 {
            return Some(sq);
        }
        f += df;
        r += dr;
    }
    None
}

/// Everything bearing on `sq`, found by walking rays rather than by lookup.
fn attackers_ref(pos: &Position, gone: u64, sq: Square) -> u64 {
    let occ = pos.occupied() & !gone;
    let mut set = 0u64;
    let target = sq as i32;
    for (df, dr) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
        let Some(hit) = ray(occ, target, df, dr) else {
            continue;
        };
        let Some(on) = pos.piece_at(hit) else { continue };
        let straight = df == 0 || dr == 0;
        let adjacent = (hit as i32 % 8 - target % 8).abs() <= 1
            && (hit as i32 / 8 - target / 8).abs() <= 1;
        let hits = match on.piece() {
            Piece::Rook => straight,
            Piece::Bishop => !straight,
            Piece::Queen => true,
            Piece::King => adjacent,
            Piece::Pawn => {
                let up = if on.color().is_white() { 1 } else { -1 };
                !straight && adjacent && dr == -up
            }
            Piece::Knight => false,
        };
        if hits {
            set |= 1u64 << hit;
        }
    }
    for (df, dr) in [(1, 2), (2, 1), (2, -1), (1, -2), (-1, -2), (-2, -1), (-2, 1), (-1, 2)] {
        let (f, r) = (target % 8 + df, target / 8 + dr);
        if !(0..8).contains(&f) || !(0..8).contains(&r) {
            continue;
        }
        let hit = (r * 8 + f) as u8;
        if occ & (1u64 << hit) != 0
            && pos.piece_at(hit).map(|on| on.piece()) == Some(Piece::Knight)
        {
            set |= 1u64 << hit;
        }
    }
    set
}

/// Recursive swap, recomputing the attackers from scratch at every step.
fn see_ref(pos: &Position, mv: Move) -> i32 {
    let to = mv.to();
    let mut gone = 1u64 << mv.from();
    let mut on_square = if mv.is_promotion() {
        mv.promoted_piece()
    } else {
        pos.piece_at(mv.from()).unwrap().piece()
    };
    let mut gain = vec![if mv.is_en_passant() {
        gone |= 1u64 << ((mv.from() & 56) | (mv.to() & 7));
        PIECE_VALUES[Piece::Pawn.index()]
    } else {
        pos.piece_at(to).map_or(0, |on| PIECE_VALUES[on.piece().index()])
    }];
    if mv.is_promotion() {
        gain[0] += PIECE_VALUES[on_square.index()] - PIECE_VALUES[Piece::Pawn.index()];
    }
    gone |= 1u64 << to;

    let mut side = pos.side_to_move().flip();
    loop {
        let set = attackers_ref(pos, gone, to) & pos.color(side) & !gone;
        let mut pick = None;
        for kind in [Piece::Pawn, Piece::Knight, Piece::Bishop, Piece::Rook, Piece::Queen, Piece::King] {
            let board = set & pos.pieces(kind, side);
            if board != 0 {
                pick = Some((kind, board.trailing_zeros() as u8));
                break;
            }
        }
        let Some((kind, square)) = pick else { break };
        if kind == Piece::King && attackers_ref(pos, gone, to) & pos.color(side.flip()) & !gone != 0
        {
            break;
        }
        gain.push(PIECE_VALUES[on_square.index()] - gain[gain.len() - 1]);
        gone |= 1u64 << square;
        on_square = kind;
        side = side.flip();
    }

    let mut index = gain.len() - 1;
    while index > 0 {
        gain[index - 1] = -std::cmp::max(-gain[index - 1], gain[index]);
        index -= 1;
    }
    gain[0]
}

#[test]
fn see_ge_agrees_with_a_reference_swap() {
    let book = std::fs::read_to_string("../books/turbo.epd").expect("book");
    let mut checked = 0u64;
    for line in book.lines().step_by(37).take(1200) {
        let fen = line.split(';').next().unwrap();
        let Ok(position) = fen.parse::<Position>() else { continue };
        let mut list = MoveList::new();
        if position.side_to_move().is_white() {
            generate_all::<engine::position::White>(&position, &mut list);
        } else {
            generate_all::<engine::position::Black>(&position, &mut list);
        }
        for &mv in list.moves() {
            let want = see_ref(&position, mv);
            for threshold in [-500, -300, -100, -1, 0, 1, 100, 300, 500] {
                assert_eq!(
                    see_ge(&position, mv, threshold),
                    want >= threshold,
                    "{fen} {mv} threshold {threshold}, reference {want}"
                );
            }
            checked += 1;
        }
    }
    assert!(checked > 20_000, "only {checked} moves checked");
    println!("checked {checked} moves");
}
