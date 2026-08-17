use crate::lookup;
use crate::moves::{Move, MoveFlags};
use crate::position::{
    bit, pop_square, square, BitBoard, CastlingRights, Position, Side, Square, EMPTY,
    NO_EN_PASSANT,
};

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

impl MoveList {
    pub const fn new() -> Self {
        Self {
            moves: [Move::new(0, 0, MoveFlags::Quiet); MAX_MOVES],
            scores: [0; MAX_MOVES],
            len: 0,
        }
    }

    pub const fn len(&self) -> usize {
        self.len
    }

    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }

    pub fn push(&mut self, mv: Move) {
        debug_assert!(self.len < MAX_MOVES, "more moves than a position can hold");
        self.moves[self.len] = mv;
        self.scores[self.len] = 0;
        self.len += 1;
    }

    pub fn moves(&self) -> &[Move] {
        &self.moves[..self.len]
    }
}

impl Default for MoveList {
    fn default() -> Self {
        Self::new()
    }
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

/// Fills the list with every legal move and reports whether we are in check.
pub fn generate_all<Us: Side>(position: &Position, list: &mut MoveList) -> bool {
    list.clear();
    let masks = check_masks::<Us>(position);
    if masks.in_check() {
        generate::<Us, true, true>(position, &masks, list);
    } else {
        generate::<Us, true, false>(position, &masks, list);
        generate::<Us, false, true>(position, &masks, list);
    }
    masks.in_check()
}

/// Mirrors a first rank board onto the side's own back rank.
const fn home<Us: Side>(board: BitBoard) -> BitBoard {
    if Us::COLOR.is_white() { board } else { board << 56 }
}

const fn king_origin<Us: Side>() -> Square {
    if Us::COLOR.is_white() { square(4, 0) } else { square(4, 7) }
}

const fn king_side<Us: Side>() -> CastlingRights {
    if Us::COLOR.is_white() {
        CastlingRights::WHITE_KING_SIDE
    } else {
        CastlingRights::BLACK_KING_SIDE
    }
}

const fn queen_side<Us: Side>() -> CastlingRights {
    if Us::COLOR.is_white() {
        CastlingRights::WHITE_QUEEN_SIDE
    } else {
        CastlingRights::BLACK_QUEEN_SIDE
    }
}

fn serialize(list: &mut MoveList, from: Square, mut targets: BitBoard, theirs: BitBoard) {
    while targets != EMPTY {
        let to = pop_square(&mut targets);
        let flags = if bit(to) & theirs == EMPTY {
            MoveFlags::Quiet
        } else {
            MoveFlags::Capture
        };
        list.push(Move::new(from, to, flags));
    }
}

/// King steps and, in the quiet half, castles.
fn king_moves<Us: Side, const NOISY: bool, const QUIET: bool>(
    position: &Position,
    masks: &CheckMasks,
    list: &mut MoveList,
) {
    let theirs = position.color(<Us::Them>::COLOR);
    let mut targets = EMPTY;
    if NOISY {
        targets |= theirs;
    }
    if QUIET {
        targets |= position.empty_squares();
    }

    // The king walks off the ray instead of covering it, so `active` does not
    // apply to it.
    let from = position.king_square(Us::COLOR);
    let attacks = lookup::KING_ATTACKS[from as usize] & targets & !masks.danger;
    serialize(list, from, attacks, theirs);

    if QUIET {
        castles::<Us>(position, masks, list);
    }
}

/// The king square is part of the path, which keeps a king in check from
/// castling out of it.
fn castles<Us: Side>(position: &Position, masks: &CheckMasks, list: &mut MoveList) {
    // The rights are only still set if the king never moved.
    let from = const { king_origin::<Us>() };
    let occupied = position.occupied();
    let rights = position.castling();

    if rights.contains(const { king_side::<Us>() })
        && occupied & const { home::<Us>(0b0110_0000) } == EMPTY
        && masks.danger & const { home::<Us>(0b0111_0000) } == EMPTY
    {
        list.push(Move::new(from, from + 2, MoveFlags::KingCastle));
    }

    if rights.contains(const { queen_side::<Us>() })
        && occupied & const { home::<Us>(0b0000_1110) } == EMPTY
        && masks.danger & const { home::<Us>(0b0001_1100) } == EMPTY
    {
        list.push(Move::new(from, from - 2, MoveFlags::QueenCastle));
    }
}

/// Square delta of a pawn step forward and `files` towards the h file.
const fn pawn_delta<Us: Side>(files: i8) -> i8 {
    if Us::COLOR.is_white() { 8 + files } else { -8 + files }
}

fn pawn_moves<Us: Side, const NOISY: bool, const QUIET: bool>(
    position: &Position,
    masks: &CheckMasks,
    list: &mut MoveList,
) {
    const PROMOTIONS: [MoveFlags; 4] = [
        MoveFlags::PromoQueen,
        MoveFlags::PromoKnight,
        MoveFlags::PromoRook,
        MoveFlags::PromoBishop,
    ];
    const PROMOTION_CAPTURES: [MoveFlags; 4] = [
        MoveFlags::PromoCaptureQueen,
        MoveFlags::PromoCaptureKnight,
        MoveFlags::PromoCaptureRook,
        MoveFlags::PromoCaptureBishop,
    ];

    let pawns = position.pawns(Us::COLOR);
    let king_square = position.king_square(Us::COLOR);
    let pinned = masks.rook_pin | masks.bishop_pin;
    let last_rank = const { home::<Us::Them>(0xff) };

    // The whole set steps at once, so a move's origin is its target shifted back.
    let mut emit = |mut targets: BitBoard, delta: i8, flags: MoveFlags| {
        while targets != EMPTY {
            let to = pop_square(&mut targets);
            let from = (to as i8 - delta) as Square;
            if bit(from) & pinned != EMPTY
                && lookup::LINE[king_square as usize][from as usize] & bit(to) == EMPTY
            {
                continue;
            }
            if bit(to) & last_rank == EMPTY {
                list.push(Move::new(from, to, flags));
                continue;
            }
            let promotions = if flags == MoveFlags::Capture {
                PROMOTION_CAPTURES
            } else {
                PROMOTIONS
            };
            for promotion in promotions {
                list.push(Move::new(from, to, promotion));
            }
        }
    };

    let empty = position.empty_squares();
    let single = lookup::pawn_push::<Us>(pawns) & empty;
    let push = const { pawn_delta::<Us>(0) };

    if QUIET {
        let double_rank =
            const { lookup::pawn_push::<Us>(lookup::pawn_push::<Us>(home::<Us>(0xff))) };
        let double = lookup::pawn_push::<Us>(single & double_rank) & empty;
        emit(single & !last_rank & masks.active, push, MoveFlags::Quiet);
        emit(double & masks.active, 2 * push, MoveFlags::DoublePush);
    }

    if NOISY {
        let victims = position.color(<Us::Them>::COLOR) & masks.active;
        emit(single & last_rank & masks.active, push, MoveFlags::Quiet);
        emit(
            lookup::pawn_attacks_west::<Us>(pawns) & victims,
            const { pawn_delta::<Us>(-1) },
            MoveFlags::Capture,
        );
        emit(
            lookup::pawn_attacks_east::<Us>(pawns) & victims,
            const { pawn_delta::<Us>(1) },
            MoveFlags::Capture,
        );

        // The pawn taken does not stand on the square the capture lands on, so
        // `active` alone cannot say whether the capture answers a check.
        if masks.en_passant & (masks.active | masks.en_passant_check) != EMPTY {
            let to = position.en_passant();
            let mut takers = lookup::pawn_attacks::<Us::Them>(masks.en_passant) & pawns;
            while takers != EMPTY {
                let from = pop_square(&mut takers);
                if bit(from) & pinned != EMPTY
                    && lookup::LINE[king_square as usize][from as usize] & masks.en_passant == EMPTY
                {
                    continue;
                }
                list.push(Move::new(from, to, MoveFlags::EnPassant));
            }
        }
    }
}

fn piece_moves<Us: Side, const NOISY: bool, const QUIET: bool>(
    position: &Position,
    masks: &CheckMasks,
    list: &mut MoveList,
) {
    let us = Us::COLOR;
    let theirs = position.color(<Us::Them>::COLOR);
    let occupied = position.occupied();
    let king_square = position.king_square(us);
    let pinned = masks.rook_pin | masks.bishop_pin;

    let mut targets = EMPTY;
    if NOISY {
        targets |= theirs;
    }
    if QUIET {
        targets |= position.empty_squares();
    }
    targets &= masks.active;

    // A pinned knight has no move that stays on its line.
    let mut knights = position.knights(us) & !pinned;
    while knights != EMPTY {
        let from = pop_square(&mut knights);
        let attacks = lookup::KNIGHT_ATTACKS[from as usize] & targets;
        serialize(list, from, attacks, theirs);
    }

    let mut diagonal = position.bishops(us) | position.queens(us);
    while diagonal != EMPTY {
        let from = pop_square(&mut diagonal);
        let mut attacks = lookup::bishop_attacks(from, occupied) & targets;
        if bit(from) & pinned != EMPTY {
            attacks &= lookup::LINE[king_square as usize][from as usize];
        }
        serialize(list, from, attacks, theirs);
    }

    let mut straight = position.rooks(us) | position.queens(us);
    while straight != EMPTY {
        let from = pop_square(&mut straight);
        let mut attacks = lookup::rook_attacks(from, occupied) & targets;
        if bit(from) & pinned != EMPTY {
            attacks &= lookup::LINE[king_square as usize][from as usize];
        }
        serialize(list, from, attacks, theirs);
    }
}
