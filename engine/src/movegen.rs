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
