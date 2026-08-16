use crate::lookup;
use crate::moves::Move;
use crate::position::{bit, pop_square, BitBoard, Position, Side, EMPTY, NO_EN_PASSANT};

/// Stages of staged move generation, in the order of the search.
/// Each stage is entered at most once, and only the `Generate*` stages
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stage {
    TtMove,

    /// Gathers the [`CheckMasks`] the generators mask with, and decides
    /// whether the evasion path replaces the noisy and quiet split.
    Init,

    GenerateNoisy,
    /// Captures and promotions that do not lose material by static exchange.
    /// The losers are set aside for [`Stage::BadNoisy`].
    GoodNoisy,

    Killer1,
    Killer2,
    /// The quiet move that refuted the opponent's previous move.
    CounterMove,

    GenerateQuiet,
    Quiet,

    /// Captures static exchange evaluation says lose material, tried last
    /// because they are usually bad but occasionally the only way through.
    BadNoisy,

    /// In check the evasion generator replaces the noisy and quiet split
    /// entirely, since the legal moves are too few for staging to pay off.
    GenerateEvasion,
    Evasion,

    Done,
}

pub const NO_CHECK: BitBoard = !EMPTY;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CheckMasks {
    pub danger: BitBoard,
    pub active: BitBoard,
    pub rook_pin: BitBoard,
    pub bishop_pin: BitBoard,
    pub en_passant: BitBoard,
    pub en_passant_check: BitBoard,
}

impl CheckMasks {
    pub const fn in_check(&self) -> bool {
        self.active != NO_CHECK
    }

    pub const fn double_check(&self) -> bool {
        self.active == EMPTY
    }
}

pub fn check_masks<Us: Side>(position: &Position) -> CheckMasks {
    let us = Us::COLOR;
    let them = <Us::Them>::COLOR;

    let king = position.king(us);
    let king_square = position.king_square(us);
    let occupied = position.occupied();
    let ours = position.color(us);

    let their_pawns = position.pawns(them);
    let their_knights = position.knights(them);
    let their_rooks = position.rooks(them) | position.queens(them);
    let their_bishops = position.bishops(them) | position.queens(them);

    // Our own pawn attacks run backwards from the king onto the pawns checking it.
    let pawn_checkers = lookup::pawn_attacks::<Us>(king) & their_pawns;
    let checkers = pawn_checkers
        | (lookup::KNIGHT_ATTACKS[king_square as usize] & their_knights)
        | (lookup::rook_attacks(king_square, occupied) & their_rooks)
        | (lookup::bishop_attacks(king_square, occupied) & their_bishops);

    let active = match checkers.count_ones() {
        0 => NO_CHECK,
        1 => lookup::BETWEEN[king_square as usize][checkers.trailing_zeros() as usize] | checkers,
        _ => EMPTY,
    };

    // The king does not shield the square behind itself from the piece checking it.
    let without_king = occupied ^ king;
    let mut danger = lookup::pawn_attacks::<Us::Them>(their_pawns)
        | lookup::KING_ATTACKS[position.king_square(them) as usize];

    let mut knights = their_knights;
    while knights != EMPTY {
        danger |= lookup::KNIGHT_ATTACKS[pop_square(&mut knights) as usize];
    }
    let mut rooks = their_rooks;
    while rooks != EMPTY {
        danger |= lookup::rook_attacks(pop_square(&mut rooks), without_king);
    }
    let mut bishops = their_bishops;
    while bishops != EMPTY {
        danger |= lookup::bishop_attacks(pop_square(&mut bishops), without_king);
    }

    // Snipers are sliders that would reach the king if the board were bare. A
    // single piece of ours in the way is pinned, and the ray is the only place
    // it may go.
    let pins = |mut snipers: BitBoard| {
        let mut mask = EMPTY;
        while snipers != EMPTY {
            let sniper = pop_square(&mut snipers);
            let ray = lookup::BETWEEN[king_square as usize][sniper as usize];
            let blockers = ray & occupied;
            if blockers.count_ones() == 1 && blockers & ours != EMPTY {
                mask |= ray | bit(sniper);
            }
        }
        mask
    };
    let rook_pin = pins(lookup::rook_attacks(king_square, EMPTY) & their_rooks);
    let bishop_pin = pins(lookup::bishop_attacks(king_square, EMPTY) & their_bishops);

    let mut en_passant = if position.en_passant() == NO_EN_PASSANT {
        EMPTY
    } else {
        bit(position.en_passant())
    };
    let victim = lookup::pawn_push::<Us::Them>(en_passant);

    if en_passant != EMPTY {
        let mut takers = lookup::pawn_attacks::<Us::Them>(en_passant) & position.pawns(us);

        // The capture clears two squares of one rank at once, which no pin mask
        // describes, so every taker is tried against the board it leaves behind.
        while takers != EMPTY {
            let taker = bit(pop_square(&mut takers));
            let after = (occupied & !(taker | victim)) | en_passant;
            if lookup::rook_attacks(king_square, after) & their_rooks != EMPTY {
                en_passant = EMPTY;
                break;
            }
        }

        // Losing the victim can open a diagonal too, which does not depend on
        // which pawn takes it.
        let after = (occupied & !victim) | en_passant;
        if lookup::bishop_attacks(king_square, after) & their_bishops != EMPTY {
            en_passant = EMPTY;
        }
    }

    // `active` only marks the square the checking pawn stands on, so a check from
    // a double push needs the skipped square spelled out separately.
    let en_passant_check = if pawn_checkers & victim != EMPTY {
        en_passant
    } else {
        EMPTY
    };

    CheckMasks {
        danger,
        active,
        rook_pin,
        bishop_pin,
        en_passant,
        en_passant_check,
    }
}

pub const MAX_MOVES: usize = 218;

#[derive(Clone)]
pub struct MoveList {
    moves: [Move; MAX_MOVES],
    scores: [i32; MAX_MOVES],
    len: usize,
}

/// Generates legal moves only
pub fn generate<Us: Side, const NOISY: bool, const QUIET: bool>(
    position: &Position,
    masks: &CheckMasks,
    list: &mut MoveList,
) {
    king_moves::<Us, NOISY, QUIET>(position, masks, list);

    // Nothing but stepping out of the way answers two checkers at once.
    if masks.double_check() {
        return;
    }

    pawn_moves::<Us, NOISY, QUIET>(position, masks, list);
    piece_moves::<Us, NOISY, QUIET>(position, masks, list);
}

/// King steps and, in the quiet half, castles.
fn king_moves<Us: Side, const NOISY: bool, const QUIET: bool>(
    position: &Position,
    masks: &CheckMasks,
    list: &mut MoveList,
) {
    todo!()
}

fn pawn_moves<Us: Side, const NOISY: bool, const QUIET: bool>(
    position: &Position,
    masks: &CheckMasks,
    list: &mut MoveList,
) {
    todo!()
}

fn piece_moves<Us: Side, const NOISY: bool, const QUIET: bool>(
    position: &Position,
    masks: &CheckMasks,
    list: &mut MoveList,
) {
    todo!()
}
