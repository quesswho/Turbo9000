use std::fmt;
use std::str::FromStr;

use crate::moves::Move;
use crate::zobrist;

/// One bit per square, indexed little-endian rank-file: bit 0 is A1, bit 63 is H8.
pub type BitBoard = u64;

pub type Square = u8;

pub const EMPTY: BitBoard = 0;

/// Denote 64 as no en-passant
pub const NO_EN_PASSANT: Square = 64;

/// White is `false` and Black is `true`, so a color indexes the color-indexed
/// boards directly and flipping is a negation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Color(bool);

impl Color {
    pub const COUNT: usize = 2;
    pub const ALL: [Self; Self::COUNT] = [White::COLOR, Black::COLOR];

    pub const fn index(self) -> usize {
        self.0 as usize
    }

    pub const fn flip(self) -> Color {
        Self(!self.0)
    }

    pub const fn is_white(self) -> bool {
        !self.0
    }
}

/// Color as a type so that generic code gets resolved at compile time.
pub trait Side {
    const COLOR: Color;
    type Them: Side<Them = Self>;
}

pub enum White {}
pub enum Black {}

impl Side for White {
    const COLOR: Color = Color(false);
    type Them = Black;
}

impl Side for Black {
    const COLOR: Color = Color(true);
    type Them = White;
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Piece {
    Pawn = 0,
    Knight = 1,
    Bishop = 2,
    Rook = 3,
    Queen = 4,
    King = 5,
}

impl Piece {
    pub const COUNT: usize = 6;

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn from_char(c: char) -> Option<Self> {
        match c {
            'p' | 'P' => Some(Piece::Pawn),
            'n' | 'N' => Some(Piece::Knight),
            'b' | 'B' => Some(Piece::Bishop),
            'r' | 'R' => Some(Piece::Rook),
            'q' | 'Q' => Some(Piece::Queen),
            'k' | 'K' => Some(Piece::King),
            _ => None,
        }
    }

    pub const fn to_char(self) -> char {
        match self {
            Piece::Pawn => 'p',
            Piece::Knight => 'n',
            Piece::Bishop => 'b',
            Piece::Rook => 'r',
            Piece::Queen => 'q',
            Piece::King => 'k',
        }
    }
}

/// Laid out as piece * 2 + color
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum ColoredPiece {
    WhitePawn = 0,
    BlackPawn = 1,
    WhiteKnight = 2,
    BlackKnight = 3,
    WhiteBishop = 4,
    BlackBishop = 5,
    WhiteRook = 6,
    BlackRook = 7,
    WhiteQueen = 8,
    BlackQueen = 9,
    WhiteKing = 10,
    BlackKing = 11,
}

impl ColoredPiece {
    const BY_COLOR_PIECE: [[ColoredPiece; Piece::COUNT]; Color::COUNT] = [
        [
            ColoredPiece::WhitePawn,
            ColoredPiece::WhiteKnight,
            ColoredPiece::WhiteBishop,
            ColoredPiece::WhiteRook,
            ColoredPiece::WhiteQueen,
            ColoredPiece::WhiteKing,
        ],
        [
            ColoredPiece::BlackPawn,
            ColoredPiece::BlackKnight,
            ColoredPiece::BlackBishop,
            ColoredPiece::BlackRook,
            ColoredPiece::BlackQueen,
            ColoredPiece::BlackKing,
        ],
    ];

    const PIECE_OF: [Piece; 12] = [
        Piece::Pawn,
        Piece::Pawn,
        Piece::Knight,
        Piece::Knight,
        Piece::Bishop,
        Piece::Bishop,
        Piece::Rook,
        Piece::Rook,
        Piece::Queen,
        Piece::Queen,
        Piece::King,
        Piece::King,
    ];

    pub const fn new(piece: Piece, color: Color) -> Self {
        Self::BY_COLOR_PIECE[color.index()][piece.index()]
    }

    pub const fn piece(self) -> Piece {
        Self::PIECE_OF[self as usize]
    }

    pub const fn color(self) -> Color {
        Color(self as u8 & 1 != 0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct CastlingRights(pub u8);

impl CastlingRights {
    pub const NONE: Self = Self(0);
    pub const WHITE_KING_SIDE: Self = Self(0b0001);
    pub const WHITE_QUEEN_SIDE: Self = Self(0b0010);
    pub const BLACK_KING_SIDE: Self = Self(0b0100);
    pub const BLACK_QUEEN_SIDE: Self = Self(0b1000);
    pub const ALL: Self = Self(0b1111);

    pub const fn contains(self, rights: Self) -> bool {
        self.0 & rights.0 == rights.0
    }

    pub fn add(&mut self, rights: Self) {
        self.0 |= rights.0;
    }

    pub fn remove(&mut self, rights: Self) {
        self.0 &= !rights.0;
    }
}

pub const fn square(file: u8, rank: u8) -> Square {
    rank * 8 + file
}

pub const fn bit(square: Square) -> BitBoard {
    debug_assert!(square < 64, "no bit for a square off the board");
    1u64 << square
}

/// Takes the lowest square off the board and hands it back.
pub const fn pop_square(board: &mut BitBoard) -> Square {
    debug_assert!(*board != EMPTY, "no square to pop off an empty board");
    let square = board.trailing_zeros() as Square;
    *board &= *board - 1;
    square
}

pub const fn file_of(square: Square) -> u8 {
    square % 8
}

pub const fn rank_of(square: Square) -> u8 {
    square / 8
}

/// Rights surviving a move touching a square, as a mask to AND with. Covers
/// both the king or rook leaving and a rook being captured where it stands.
const CASTLE_MASK: [u8; 64] = {
    let mut mask = [CastlingRights::ALL.0; 64];
    mask[0] = CastlingRights::ALL.0 & !CastlingRights::WHITE_QUEEN_SIDE.0;
    mask[4] = CastlingRights::ALL.0
        & !(CastlingRights::WHITE_KING_SIDE.0 | CastlingRights::WHITE_QUEEN_SIDE.0);
    mask[7] = CastlingRights::ALL.0 & !CastlingRights::WHITE_KING_SIDE.0;
    mask[56] = CastlingRights::ALL.0 & !CastlingRights::BLACK_QUEEN_SIDE.0;
    mask[60] = CastlingRights::ALL.0
        & !(CastlingRights::BLACK_KING_SIDE.0 | CastlingRights::BLACK_QUEEN_SIDE.0);
    mask[63] = CastlingRights::ALL.0 & !CastlingRights::BLACK_KING_SIDE.0;
    mask
};

/// Where the pawn taken by an en passant capture actually stands.
const fn en_passant_victim(to: Square, capturing: Color) -> Square {
    if capturing.is_white() { to - 8 } else { to + 8 }
}

/// Rook travel for a castle, given the king's origin.
const fn castle_rook(from: Square, king_side: bool) -> (Square, Square) {
    let rank = rank_of(from);
    if king_side {
        (square(7, rank), square(5, rank))
    } else {
        (square(0, rank), square(3, rank))
    }
}

/// Piece boards are indexed [color][piece], so that each colored boards is contiguous
/// and maintains good memory locality.
///
/// Position holds enough information to construct a FEN.
/// In make_move() we return a `Undo` for the caller to keep until it unmakes,
/// this simplifies irreversible moves.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Position {
    pieces: [[BitBoard; Piece::COUNT]; Color::COUNT],
    colors: [BitBoard; Color::COUNT],
    occupied: BitBoard,
    mailbox: [Option<ColoredPiece>; 64],
    side_to_move: Color,
    castling: CastlingRights,
    en_passant: Square,
    halfmove_clock: u8,
    hash: u64,
}

/// Undo state returned by `make_move` and passed back to
/// `unmake_move`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Undo {
    pub captured: Option<ColoredPiece>,
    pub castling: CastlingRights,
    pub en_passant: Square,
    pub halfmove_clock: u8,
    pub hash: u64,
}

impl Position {
    pub const fn empty() -> Self {
        Self {
            pieces: [[EMPTY; Piece::COUNT]; Color::COUNT],
            colors: [EMPTY; Color::COUNT],
            occupied: EMPTY,
            mailbox: [None; 64],
            side_to_move: White::COLOR,
            castling: CastlingRights::NONE,
            en_passant: NO_EN_PASSANT,
            halfmove_clock: 0,
            hash: 0,
        }
    }

    pub fn starting() -> Self {
        const WHITE_PAWNS: BitBoard = 0x0000_0000_0000_ff00;
        const WHITE_KNIGHTS: BitBoard = 0x0000_0000_0000_0042;
        const WHITE_BISHOPS: BitBoard = 0x0000_0000_0000_0024;
        const WHITE_ROOKS: BitBoard = 0x0000_0000_0000_0081;
        const WHITE_QUEENS: BitBoard = 0x0000_0000_0000_0008;
        const WHITE_KING: BitBoard = 0x0000_0000_0000_0010;

        let white = 0x0000_0000_0000_ffff;
        let black = 0xffff_0000_0000_0000;

        let mut position = Self {
            pieces: [
                [
                    WHITE_PAWNS,
                    WHITE_KNIGHTS,
                    WHITE_BISHOPS,
                    WHITE_ROOKS,
                    WHITE_QUEENS,
                    WHITE_KING,
                ],
                [
                    WHITE_PAWNS << 40,
                    WHITE_KNIGHTS << 56,
                    WHITE_BISHOPS << 56,
                    WHITE_ROOKS << 56,
                    WHITE_QUEENS << 56,
                    WHITE_KING << 56,
                ],
            ],
            colors: [white, black],
            occupied: white | black,
            mailbox: [None; 64],
            side_to_move: White::COLOR,
            castling: CastlingRights::ALL,
            en_passant: NO_EN_PASSANT,
            halfmove_clock: 0,
            hash: 0,
        };
        position.rebuild_mailbox();
        position.hash = position.compute_hash();
        position
    }

    /// Needed only after setting the piece boards directly, as a FEN parser would.
    pub fn rebuild_mailbox(&mut self) {
        self.mailbox = [None; 64];
        for (color_index, boards) in self.pieces.iter().enumerate() {
            let color = Color::ALL[color_index];
            for (piece_index, &board) in boards.iter().enumerate() {
                let piece = ColoredPiece::PIECE_OF[piece_index * 2];
                let mut board = board;
                while board != EMPTY {
                    let square = pop_square(&mut board) as usize;
                    self.mailbox[square] = Some(ColoredPiece::new(piece, color));
                }
            }
        }
    }

    pub const fn pieces(&self, piece: Piece, color: Color) -> BitBoard {
        self.pieces[color.index()][piece.index()]
    }

    pub const fn pieces_of_kind(&self, piece: Piece) -> BitBoard {
        self.pieces[White::COLOR.index()][piece.index()]
            | self.pieces[Black::COLOR.index()][piece.index()]
    }

    pub const fn color(&self, color: Color) -> BitBoard {
        self.colors[color.index()]
    }

    pub const fn occupied(&self) -> BitBoard {
        self.occupied
    }

    pub const fn empty_squares(&self) -> BitBoard {
        !self.occupied
    }

    pub const fn piece_at(&self, square: Square) -> Option<ColoredPiece> {
        self.mailbox[square as usize]
    }

    pub const fn pawns(&self, color: Color) -> BitBoard {
        self.pieces(Piece::Pawn, color)
    }

    pub const fn knights(&self, color: Color) -> BitBoard {
        self.pieces(Piece::Knight, color)
    }

    pub const fn bishops(&self, color: Color) -> BitBoard {
        self.pieces(Piece::Bishop, color)
    }

    pub const fn rooks(&self, color: Color) -> BitBoard {
        self.pieces(Piece::Rook, color)
    }

    pub const fn queens(&self, color: Color) -> BitBoard {
        self.pieces(Piece::Queen, color)
    }

    pub const fn king(&self, color: Color) -> BitBoard {
        self.pieces(Piece::King, color)
    }

    pub const fn king_square(&self, color: Color) -> Square {
        self.king(color).trailing_zeros() as Square
    }

    pub const fn side_to_move(&self) -> Color {
        self.side_to_move
    }

    pub fn set_side_to_move(&mut self, color: Color) {
        self.side_to_move = color;
    }

    pub const fn castling(&self) -> CastlingRights {
        self.castling
    }

    pub fn castling_mut(&mut self) -> &mut CastlingRights {
        &mut self.castling
    }

    /// [`NO_EN_PASSANT`] unless the previous move was a double push.
    pub const fn en_passant(&self) -> Square {
        self.en_passant
    }

    pub fn set_en_passant(&mut self, square: Square) {
        debug_assert!(square <= NO_EN_PASSANT);
        self.en_passant = square;
    }

    pub const fn halfmove_clock(&self) -> u8 {
        self.halfmove_clock
    }

    /// Clamped, since a FEN may carry any number but the rule caps at 100.
    pub fn set_halfmove_clock(&mut self, clock: u32) {
        debug_assert!(clock <= 100, "halfmove clock beyond the fifty move rule");
        self.halfmove_clock = clock.min(100) as u8;
    }

    pub const fn hash(&self) -> u64 {
        self.hash
    }

    pub fn compute_hash(&self) -> u64 {
        let mut hash = EMPTY;
        let mut occupied = self.occupied;
        while occupied != EMPTY {
            let square = pop_square(&mut occupied);
            let colored = self.mailbox[square as usize].expect("occupied square without a piece");
            hash ^= zobrist::piece_key(colored, square);
        }
        hash ^= zobrist::castling_key(self.castling);
        if self.en_passant != NO_EN_PASSANT {
            hash ^= zobrist::en_passant_key(file_of(self.en_passant));
        }
        if !self.side_to_move.is_white() {
            hash ^= zobrist::side_key();
        }
        hash
    }

    /// The one place state is captured, so unmake cannot miss a field.
    pub const fn undo(&self, captured: Option<ColoredPiece>) -> Undo {
        Undo {
            captured,
            castling: self.castling,
            en_passant: self.en_passant,
            halfmove_clock: self.halfmove_clock,
            hash: self.hash,
        }
    }

    /// The one place it is put back. Restores state only, not the boards.
    pub fn restore(&mut self, undo: Undo) {
        self.castling = undo.castling;
        self.en_passant = undo.en_passant;
        self.halfmove_clock = undo.halfmove_clock;
        self.hash = undo.hash;
    }

    /// The square must be empty.
    pub fn put_piece(&mut self, square: Square, piece: Piece, color: Color) {
        debug_assert!(self.occupied & bit(square) == EMPTY);
        let mask = bit(square);
        let colored = ColoredPiece::new(piece, color);
        self.pieces[color.index()][piece.index()] |= mask;
        self.colors[color.index()] |= mask;
        self.occupied |= mask;
        self.mailbox[square as usize] = Some(colored);
        self.hash ^= zobrist::piece_key(colored, square);
    }

    /// The piece and color must match what stands on the square.
    pub fn remove_piece(&mut self, square: Square, piece: Piece, color: Color) {
        debug_assert!(self.piece_at(square) == Some(ColoredPiece::new(piece, color)));
        let mask = !bit(square);
        self.pieces[color.index()][piece.index()] &= mask;
        self.colors[color.index()] &= mask;
        self.occupied &= mask;
        self.mailbox[square as usize] = None;
        self.hash ^= zobrist::piece_key(ColoredPiece::new(piece, color), square);
    }

    pub fn move_piece(&mut self, from: Square, to: Square, piece: Piece, color: Color) {
        debug_assert!(self.piece_at(from) == Some(ColoredPiece::new(piece, color)));
        debug_assert!(self.occupied & bit(to) == EMPTY);
        let mask = bit(from) | bit(to);
        let colored = ColoredPiece::new(piece, color);
        self.pieces[color.index()][piece.index()] ^= mask;
        self.colors[color.index()] ^= mask;
        self.occupied ^= mask;
        self.mailbox[from as usize] = None;
        self.mailbox[to as usize] = Some(colored);
        self.hash ^= zobrist::piece_key(colored, from) ^ zobrist::piece_key(colored, to);
    }

    /// Applies a move without checking legality, returning what it destroyed.
    /// The move must be pseudo legal and correctly flagged.
    pub fn make_move(&mut self, mv: Move) -> Undo {
        let us = self.side_to_move;
        let them = us.flip();
        let from = mv.from();
        let to = mv.to();
        let moving = self.mailbox[from as usize]
            .expect("no piece on the origin square")
            .piece();

        let captured = if mv.is_en_passant() {
            Some(ColoredPiece::new(Piece::Pawn, them))
        } else {
            self.mailbox[to as usize]
        };
        let undo = self.undo(captured);

        if let Some(captured) = captured {
            let victim = if mv.is_en_passant() {
                en_passant_victim(to, us)
            } else {
                to
            };
            self.remove_piece(victim, captured.piece(), them);
        }

        if mv.is_promotion() {
            self.remove_piece(from, Piece::Pawn, us);
            self.put_piece(to, mv.promoted_piece(), us);
        } else {
            self.move_piece(from, to, moving, us);
            if mv.is_castle() {
                let (rook_from, rook_to) = castle_rook(from, mv.is_king_castle());
                self.move_piece(rook_from, rook_to, Piece::Rook, us);
            }
        }

        self.hash ^= zobrist::castling_key(self.castling);
        self.castling.0 &= CASTLE_MASK[from as usize] & CASTLE_MASK[to as usize];
        self.hash ^= zobrist::castling_key(self.castling);

        if self.en_passant != NO_EN_PASSANT {
            self.hash ^= zobrist::en_passant_key(file_of(self.en_passant));
        }
        self.en_passant = if mv.is_double_push() {
            (from + to) / 2
        } else {
            NO_EN_PASSANT
        };
        if self.en_passant != NO_EN_PASSANT {
            self.hash ^= zobrist::en_passant_key(file_of(self.en_passant));
        }
        self.halfmove_clock = if captured.is_some() || moving == Piece::Pawn {
            0
        } else {
            self.halfmove_clock.saturating_add(1)
        };
        self.side_to_move = them;
        self.hash ^= zobrist::side_key();

        undo
    }

    /// Takes back the move `make_move` returned this `Undo` for.
    pub fn unmake_move(&mut self, mv: Move, undo: Undo) {
        self.side_to_move = self.side_to_move.flip();
        let us = self.side_to_move;
        let them = us.flip();
        let from = mv.from();
        let to = mv.to();

        if mv.is_promotion() {
            self.remove_piece(to, mv.promoted_piece(), us);
            self.put_piece(from, Piece::Pawn, us);
        } else {
            let moving = self.mailbox[to as usize]
                .expect("no piece on the destination square")
                .piece();
            self.move_piece(to, from, moving, us);
            if mv.is_castle() {
                let (rook_from, rook_to) = castle_rook(from, mv.is_king_castle());
                self.move_piece(rook_to, rook_from, Piece::Rook, us);
            }
        }

        if let Some(captured) = undo.captured {
            let victim = if mv.is_en_passant() {
                en_passant_victim(to, us)
            } else {
                to
            };
            self.put_piece(victim, captured.piece(), them);
        }

        self.restore(undo);
    }
}

impl Default for Position {
    fn default() -> Self {
        Self::starting()
    }
}

impl FromStr for Position {
    type Err = &'static str;

    fn from_str(fen: &str) -> Result<Self, Self::Err> {
        let mut fields = fen.split_whitespace();
        let mut position = Self::empty();

        let (mut file, mut rank) = (0, 7);
        for c in fields.next().ok_or("missing piece placement")?.chars() {
            match c {
                '/' => (file, rank) = (0, rank - 1),
                '1'..='8' => file += c as u8 - b'0',
                _ => {
                    let piece = Piece::from_char(c).ok_or("unknown piece")?;
                    let color = if c.is_ascii_uppercase() {
                        White::COLOR
                    } else {
                        Black::COLOR
                    };
                    position.put_piece(square(file, rank), piece, color);
                    file += 1;
                }
            }
        }

        position.side_to_move = match fields.next().ok_or("missing side to move")? {
            "w" => White::COLOR,
            "b" => Black::COLOR,
            _ => return Err("side to move is neither w nor b"),
        };

        for c in fields.next().ok_or("missing castling rights")?.chars() {
            position.castling.add(match c {
                'K' => CastlingRights::WHITE_KING_SIDE,
                'Q' => CastlingRights::WHITE_QUEEN_SIDE,
                'k' => CastlingRights::BLACK_KING_SIDE,
                'q' => CastlingRights::BLACK_QUEEN_SIDE,
                _ => continue,
            });
        }

        position.en_passant = match fields.next().ok_or("missing en passant square")?.as_bytes() {
            [file, rank] => square(file - b'a', rank - b'1'),
            _ => NO_EN_PASSANT,
        };

        position.halfmove_clock = fields.next().and_then(|c| c.parse().ok()).unwrap_or(0);
        position.hash = position.compute_hash();

        Ok(position)
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for rank in (0..8).rev() {
            write!(f, "{} ", rank + 1)?;
            for file in 0..8 {
                let symbol = match self.piece_at(square(file, rank)) {
                    Some(colored) if colored.color().is_white() => {
                        colored.piece().to_char().to_ascii_uppercase()
                    }
                    Some(colored) => colored.piece().to_char(),
                    None => '.',
                };
                write!(f, "{symbol} ")?;
            }
            writeln!(f)?;
        }
        write!(f, "  a b c d e f g h")
    }
}
