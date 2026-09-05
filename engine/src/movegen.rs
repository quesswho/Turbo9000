use crate::lookup;
use crate::moves::{Move, MoveFlags};
use crate::position::{
    bit, pop_square, square, BitBoard, Black, CastlingRights, Piece, Position, Side, Square, White,
    EMPTY, NO_EN_PASSANT,
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

/// The squares a piece of ours has to land on to check their king.
pub struct CheckSquares {
    pawn: BitBoard,
    knight: BitBoard,
    bishop: BitBoard,
    rook: BitBoard,
}

impl CheckSquares {
    pub fn new<Us: Side>(position: &Position) -> Self {
        let them = <Us::Them>::COLOR;
        let square = position.king_square(them);
        let occupied = position.occupied();
        Self {
            pawn: lookup::pawn_attacks::<Us::Them>(position.king(them)),
            knight: lookup::KNIGHT_ATTACKS[square as usize],
            bishop: lookup::bishop_attacks(square, occupied),
            rook: lookup::rook_attacks(square, occupied),
        }
    }

    /// Direct checks only, so a discovered check reads as no check at all.
    pub fn given_by(&self, piece: Piece, to: Square) -> bool {
        let squares = match piece {
            Piece::Pawn => self.pawn,
            Piece::Knight => self.knight,
            Piece::Bishop => self.bishop,
            Piece::Rook => self.rook,
            Piece::Queen => self.bishop | self.rook,
            Piece::King => EMPTY,
        };
        squares & bit(to) != EMPTY
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CheckMasks {
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

    let active = if checkers == EMPTY {
        NO_CHECK
    } else if checkers & (checkers - 1) == EMPTY {
        lookup::BETWEEN[king_square as usize][checkers.trailing_zeros() as usize] | checkers
    } else {
        EMPTY
    };

    // Snipers are sliders that would reach the king if the board were bare. A
    // single piece of ours in the way is pinned, and the ray is the only place
    // it may go.
    let pins = |mut snipers: BitBoard| {
        let mut mask = EMPTY;
        while snipers != EMPTY {
            let sniper = pop_square(&mut snipers);
            let ray = lookup::BETWEEN[king_square as usize][sniper as usize];
            let blockers = ray & occupied;
            if blockers & ours != EMPTY && blockers & (blockers - 1) == EMPTY {
                mask |= ray | bit(sniper);
            }
        }
        mask
    };
    let rook_pin = pins(lookup::ROOK_RAYS[king_square as usize] & their_rooks);
    let bishop_pin = pins(lookup::BISHOP_RAYS[king_square as usize] & their_bishops);

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
        active,
        rook_pin,
        bishop_pin,
        en_passant,
        en_passant_check,
    }
}

/// Every square the enemy covers, with our king lifted off the board so that a
/// slider is not stopped by the very piece it is checking. Walking the enemy
/// sliders is the most expensive part of generation, so only the king asks for
/// it, and only when it has somewhere to go.
fn danger<Us: Side>(position: &Position) -> BitBoard {
    let them = <Us::Them>::COLOR;
    let without_king = position.occupied() ^ position.king(Us::COLOR);

    let mut danger = lookup::pawn_attacks::<Us::Them>(position.pawns(them))
        | lookup::KING_ATTACKS[position.king_square(them) as usize];

    let mut knights = position.knights(them);
    while knights != EMPTY {
        danger |= lookup::KNIGHT_ATTACKS[pop_square(&mut knights) as usize];
    }
    let mut rooks = position.rooks(them) | position.queens(them);
    while rooks != EMPTY {
        danger |= lookup::rook_attacks(pop_square(&mut rooks), without_king);
    }
    let mut bishops = position.bishops(them) | position.queens(them);
    while bishops != EMPTY {
        danger |= lookup::bishop_attacks(pop_square(&mut bishops), without_king);
    }
    danger
}

pub const MAX_MOVES: usize = 218;

const TT_RANK: i32 = i32::MAX;
const CAPTURE_RANK: i32 = 1 << 20;
const PROMOTION_RANK: i32 = 1 << 19;
const KILLER_RANK: i32 = 1 << 18;

/// Bounds a history score so a quiet move can never outrank a killer.
pub const HISTORY_MAX: i32 = 1 << 14;

const BAD_CAPTURE_RANK: i32 = 1 << 17;

/// The king is never taken, so the exchange gives it no value.
pub const PIECE_VALUES: [i32; Piece::COUNT] = [100, 320, 330, 500, 900, 0];

const SEE_ORDER: [Piece; Piece::COUNT] = [
    Piece::Pawn,
    Piece::Knight,
    Piece::Bishop,
    Piece::Rook,
    Piece::Queen,
    Piece::King,
];

/// Everything bearing on `square`, seen through `occupied` rather than through
/// the board, so an exchange sees the sliders behind what it has taken.
fn attackers_to(position: &Position, square: Square, occupied: BitBoard) -> BitBoard {
    let target = bit(square);
    let straight = position.pieces_of_kind(Piece::Rook) | position.pieces_of_kind(Piece::Queen);
    let diagonal = position.pieces_of_kind(Piece::Bishop) | position.pieces_of_kind(Piece::Queen);

    // Pawn attacks run backwards from the square onto the pawns bearing on it.
    (lookup::pawn_attacks::<Black>(target) & position.pawns(White::COLOR))
        | (lookup::pawn_attacks::<White>(target) & position.pawns(Black::COLOR))
        | (lookup::KNIGHT_ATTACKS[square as usize] & position.pieces_of_kind(Piece::Knight))
        | (lookup::KING_ATTACKS[square as usize] & position.pieces_of_kind(Piece::King))
        | (lookup::rook_attacks(square, occupied) & straight)
        | (lookup::bishop_attacks(square, occupied) & diagonal)
}

/// The file the pawn was taken towards, on the rank it was taken from.
const fn en_passant_victim(mv: Move) -> Square {
    (mv.from() & 56) | (mv.to() & 7)
}

/// Compute whether an exchange wins at least `threshold`.
pub fn see_ge(position: &Position, mv: Move, threshold: i32) -> bool {
    let (from, to) = (mv.from(), mv.to());

    let mut gain = if mv.is_en_passant() {
        PIECE_VALUES[Piece::Pawn.index()]
    } else {
        match position.piece_at(to) {
            Some(on) => PIECE_VALUES[on.piece().index()],
            None => 0,
        }
    };

    let mut next = position.piece_at(from).map_or(Piece::Pawn, |on| on.piece());
    if mv.is_promotion() {
        next = mv.promoted_piece();
        gain += PIECE_VALUES[next.index()] - PIECE_VALUES[Piece::Pawn.index()];
    }

    let mut swap = gain - threshold;
    if swap < 0 {
        return false;
    }
    swap = PIECE_VALUES[next.index()] - swap;
    if swap <= 0 {
        return true;
    }

    let mut occupied = position.occupied() ^ bit(from) ^ bit(to);
    if mv.is_en_passant() {
        occupied ^= bit(en_passant_victim(mv));
    }

    let straight = position.pieces_of_kind(Piece::Rook) | position.pieces_of_kind(Piece::Queen);
    let diagonal = position.pieces_of_kind(Piece::Bishop) | position.pieces_of_kind(Piece::Queen);
    let mut attackers = attackers_to(position, to, occupied);

    let mut side = position.side_to_move();
    let mut winning = 1;
    loop {
        side = side.flip();
        attackers &= occupied;
        let ours = attackers & position.color(side);
        if ours == EMPTY {
            break;
        }
        winning ^= 1;

        let mut cheapest = Piece::King;
        let mut board = EMPTY;
        for piece in SEE_ORDER {
            let candidates = ours & position.pieces(piece, side);
            if candidates != EMPTY {
                (cheapest, board) = (piece, candidates);
                break;
            }
        }

        if cheapest == Piece::King {
            // The king may not take into a square the other side still holds.
            if attackers & position.color(side.flip()) != EMPTY {
                winning ^= 1;
            }
            break;
        }

        swap = PIECE_VALUES[cheapest.index()] - swap;
        if swap < winning {
            break;
        }

        occupied ^= board & board.wrapping_neg();
        // Only the line the taken piece stood on can open, and a knight bearing
        // on a square is never on a line through it.
        match cheapest {
            Piece::Pawn | Piece::Bishop => {
                attackers |= lookup::bishop_attacks(to, occupied) & diagonal;
            }
            Piece::Rook => attackers |= lookup::rook_attacks(to, occupied) & straight,
            Piece::Queen => {
                attackers |= lookup::bishop_attacks(to, occupied) & diagonal;
                attackers |= lookup::rook_attacks(to, occupied) & straight;
            }
            _ => {}
        }
    }
    winning != 0
}

/// `MVV_LVA[victim][attacker]`: most valuable victim, cheapest attacker.
const MVV_LVA: [[i32; Piece::COUNT]; Piece::COUNT] = {
    let mut table = [[0; Piece::COUNT]; Piece::COUNT];
    let mut victim = 0;
    while victim < Piece::COUNT {
        let mut attacker = 0;
        while attacker < Piece::COUNT {
            table[victim][attacker] = CAPTURE_RANK + PIECE_VALUES[victim] * 16 - attacker as i32;
            attacker += 1;
        }
        victim += 1;
    }
    table
};

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
        self.len += 1;
    }

    pub fn moves(&self) -> &[Move] {
        &self.moves[..self.len]
    }

    pub fn moves_mut(&mut self) -> &mut [Move] {
        &mut self.moves[..self.len]
    }

    /// A `tt_move` that is not in the list simply never matches.
    pub fn score(
        &mut self,
        position: &Position,
        tt_move: Move,
        killers: [Move; 2],
        history: &[[i32; 64]; 64],
    ) {
        for i in 0..self.len {
            self.scores[i] = rank(position, self.moves[i], tt_move, killers, history);
        }
    }

    /// Whether the move picked into `index` is a capture the exchange says
    /// loses material.
    pub fn loses_material(&self, index: usize) -> bool {
        (BAD_CAPTURE_RANK..KILLER_RANK).contains(&self.scores[index])
    }

    /// Swaps the best move from `start` onwards into `start` and returns it.
    pub fn pick(&mut self, start: usize) -> Move {
        let scores = &self.scores[start..self.len];
        let mut best = 0;
        let mut top = scores[0];
        for i in 1..scores.len() {
            if scores[i] > top {
                (best, top) = (i, scores[i]);
            }
        }
        let best = start + best;
        self.moves.swap(start, best);
        self.scores.swap(start, best);
        self.moves[start]
    }
}

fn rank(
    position: &Position,
    mv: Move,
    tt_move: Move,
    killers: [Move; 2],
    history: &[[i32; 64]; 64],
) -> i32 {
    if mv == tt_move {
        TT_RANK
    } else if mv.is_capture() {
        // En passant leaves `to` empty, and the victim is always a pawn.
        let victim = position.piece_at(mv.to()).map_or(Piece::Pawn, |on| on.piece());
        let attacker = position.piece_at(mv.from()).map_or(Piece::Pawn, |on| on.piece());
        let score = MVV_LVA[victim.index()][attacker.index()];
        // A victim worth at least the attacker cannot lose material.
        if PIECE_VALUES[victim.index()] >= PIECE_VALUES[attacker.index()]
            || see_ge(position, mv, 0)
        {
            score
        } else {
            score - CAPTURE_RANK + BAD_CAPTURE_RANK
        }
    } else if mv.is_promotion() {
        PROMOTION_RANK + mv.promoted_piece().index() as i32
    } else if mv == killers[0] {
        KILLER_RANK + 1
    } else if mv == killers[1] {
        KILLER_RANK
    } else {
        history[mv.from() as usize][mv.to() as usize]
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
    king_moves::<Us, NOISY, QUIET>(position, list);

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
    generate::<Us, true, true>(position, &masks, list);
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

const fn home_rights<Us: Side>() -> CastlingRights {
    CastlingRights(king_side::<Us>().0 | queen_side::<Us>().0)
}

fn emit_all(list: &mut MoveList, from: Square, mut targets: BitBoard, flags: MoveFlags) {
    while targets != EMPTY {
        list.push(Move::new(from, pop_square(&mut targets), flags));
    }
}

/// `attacks` is already inside the requested halves, so splitting it on the
/// enemy board settles every flag without a test per move.
fn serialize<const NOISY: bool, const QUIET: bool>(
    list: &mut MoveList,
    from: Square,
    attacks: BitBoard,
    theirs: BitBoard,
) {
    if NOISY {
        emit_all(list, from, attacks & theirs, MoveFlags::Capture);
    }
    if QUIET {
        emit_all(list, from, attacks & !theirs, MoveFlags::Quiet);
    }
}

/// King steps and, in the quiet half, castles.
fn king_moves<Us: Side, const NOISY: bool, const QUIET: bool>(
    position: &Position,
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
    let steps = lookup::KING_ATTACKS[from as usize] & targets;
    let castling = QUIET && position.castling().contains_either(const { home_rights::<Us>() });
    if steps == EMPTY && !castling {
        return;
    }

    let danger = danger::<Us>(position);
    serialize::<NOISY, QUIET>(list, from, steps & !danger, theirs);

    if QUIET {
        castles::<Us>(position, danger, list);
    }
}

/// The king square is part of the path, which keeps a king in check from
/// castling out of it.
fn castles<Us: Side>(position: &Position, danger: BitBoard, list: &mut MoveList) {
    // The rights are only still set if the king never moved.
    let from = const { king_origin::<Us>() };
    let occupied = position.occupied();
    let rights = position.castling();

    if rights.contains(const { king_side::<Us>() })
        && occupied & const { home::<Us>(0b0110_0000) } == EMPTY
        && danger & const { home::<Us>(0b0111_0000) } == EMPTY
    {
        list.push(Move::new(from, from + 2, MoveFlags::KingCastle));
    }

    if rights.contains(const { queen_side::<Us>() })
        && occupied & const { home::<Us>(0b0000_1110) } == EMPTY
        && danger & const { home::<Us>(0b0001_1100) } == EMPTY
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

    // The whole set steps at once, so a move's origin is its target shifted
    // back, and whether it promotes is settled for the set, not per move.
    let mut emit = |mut targets: BitBoard, delta: i8, flags: MoveFlags, promotes: bool| {
        while targets != EMPTY {
            let to = pop_square(&mut targets);
            let from = (to as i8 - delta) as Square;
            if bit(from) & pinned != EMPTY
                && lookup::LINE[king_square as usize][from as usize] & bit(to) == EMPTY
            {
                continue;
            }
            if !promotes {
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
        emit(single & !last_rank & masks.active, push, MoveFlags::Quiet, false);
        emit(double & masks.active, 2 * push, MoveFlags::DoublePush, false);
    }

    if NOISY {
        let victims = position.color(<Us::Them>::COLOR) & masks.active;
        emit(single & last_rank & masks.active, push, MoveFlags::Quiet, true);
        let west = const { pawn_delta::<Us>(-1) };
        let east = const { pawn_delta::<Us>(1) };
        let west_targets = lookup::pawn_attacks_west::<Us>(pawns) & victims;
        let east_targets = lookup::pawn_attacks_east::<Us>(pawns) & victims;
        emit(west_targets & !last_rank, west, MoveFlags::Capture, false);
        emit(east_targets & !last_rank, east, MoveFlags::Capture, false);
        emit(west_targets & last_rank, west, MoveFlags::Capture, true);
        emit(east_targets & last_rank, east, MoveFlags::Capture, true);

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
        serialize::<NOISY, QUIET>(list, from, attacks, theirs);
    }

    let mut diagonal = position.bishops(us) | position.queens(us);
    while diagonal != EMPTY {
        let from = pop_square(&mut diagonal);
        let mut attacks = lookup::bishop_attacks(from, occupied) & targets;
        if bit(from) & pinned != EMPTY {
            attacks &= lookup::LINE[king_square as usize][from as usize];
        }
        serialize::<NOISY, QUIET>(list, from, attacks, theirs);
    }

    let mut straight = position.rooks(us) | position.queens(us);
    while straight != EMPTY {
        let from = pop_square(&mut straight);
        let mut attacks = lookup::rook_attacks(from, occupied) & targets;
        if bit(from) & pinned != EMPTY {
            attacks &= lookup::LINE[king_square as usize][from as usize];
        }
        serialize::<NOISY, QUIET>(list, from, attacks, theirs);
    }
}

/// The legal move written in long algebraic notation, as UCI sends them.
pub fn find_move(position: &Position, text: &str) -> Option<Move> {
    let mut list = MoveList::new();
    if position.side_to_move().is_white() {
        generate_all::<White>(position, &mut list);
    } else {
        generate_all::<Black>(position, &mut list);
    }
    list.moves().iter().copied().find(|mv| mv.to_string() == text)
}
