use std::fmt;

use crate::position::{Piece, Square, file_of, rank_of};

/// Bit 3 marks a promotion, bit 2 a capture, so the two tests are single masks.
const QUIET: u16 = 0b0000;
const DOUBLE_PUSH: u16 = 0b0001;
const KING_CASTLE: u16 = 0b0010;
const QUEEN_CASTLE: u16 = 0b0011;
const CAPTURE: u16 = 0b0100;
const EN_PASSANT: u16 = 0b0101;
const PROMO: u16 = 0b1000;

const CAPTURE_BIT: u16 = 0b0100;
const PROMO_BIT: u16 = 0b1000;

/// `from` in bits 0..6, `to` in bits 6..12, flags in bits 12..16.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Move(u16);

impl Move {
    const fn new(from: Square, to: Square, flags: u16) -> Self {
        Self(from as u16 | (to as u16) << 6 | flags << 12)
    }

    pub const fn quiet(from: Square, to: Square) -> Self {
        Self::new(from, to, QUIET)
    }

    pub const fn capture(from: Square, to: Square) -> Self {
        Self::new(from, to, CAPTURE)
    }

    pub const fn double_push(from: Square, to: Square) -> Self {
        Self::new(from, to, DOUBLE_PUSH)
    }

    pub const fn en_passant(from: Square, to: Square) -> Self {
        Self::new(from, to, EN_PASSANT)
    }

    pub const fn king_castle(from: Square, to: Square) -> Self {
        Self::new(from, to, KING_CASTLE)
    }

    pub const fn queen_castle(from: Square, to: Square) -> Self {
        Self::new(from, to, QUEEN_CASTLE)
    }

    pub const fn promotion(from: Square, to: Square, piece: Piece, capture: bool) -> Self {
        debug_assert!(piece.index() >= 1 && piece.index() <= 4);
        let flags = PROMO | (piece.index() as u16 - 1) | if capture { CAPTURE_BIT } else { 0 };
        Self::new(from, to, flags)
    }

    pub const fn from(self) -> Square {
        (self.0 & 0x3f) as Square
    }

    pub const fn to(self) -> Square {
        (self.0 >> 6 & 0x3f) as Square
    }

    const fn flags(self) -> u16 {
        self.0 >> 12
    }

    pub const fn is_capture(self) -> bool {
        self.flags() & CAPTURE_BIT != 0
    }

    pub const fn is_en_passant(self) -> bool {
        self.flags() == EN_PASSANT
    }

    pub const fn is_double_push(self) -> bool {
        self.flags() == DOUBLE_PUSH
    }

    pub const fn is_king_castle(self) -> bool {
        self.flags() == KING_CASTLE
    }

    pub const fn is_queen_castle(self) -> bool {
        self.flags() == QUEEN_CASTLE
    }

    pub const fn is_castle(self) -> bool {
        self.is_king_castle() || self.is_queen_castle()
    }

    pub const fn promoted_piece(self) -> Option<Piece> {
        if self.flags() & PROMO_BIT == 0 {
            return None;
        }
        Some(match self.flags() & 0b0011 {
            0 => Piece::Knight,
            1 => Piece::Bishop,
            2 => Piece::Rook,
            _ => Piece::Queen,
        })
    }
}

/// Long algebraic notation
impl fmt::Display for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let square = |s: Square| {
            [
                (b'a' + file_of(s)) as char,
                (b'1' + rank_of(s)) as char,
            ]
        };
        let [from_file, from_rank] = square(self.from());
        let [to_file, to_rank] = square(self.to());
        write!(f, "{from_file}{from_rank}{to_file}{to_rank}")?;
        match self.promoted_piece() {
            Some(piece) => write!(f, "{}", piece.to_char()),
            None => Ok(()),
        }
    }
}

impl fmt::Debug for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Move({self}, flags {:04b})", self.flags())
    }
}
