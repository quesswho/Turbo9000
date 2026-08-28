use std::fmt;
use std::mem;

use crate::position::{file_of, rank_of, Piece, Square};

/// Bit 3 marks a promotion, bit 2 a capture and low two
/// bits decide the promotion piece.
#[repr(u16)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MoveFlags {
    Quiet = 0b0000,
    DoublePush = 0b0001,

    KingCastle = 0b0010,
    QueenCastle = 0b0011,

    Capture = 0b0100,
    EnPassant = 0b0101,

    /// Keeps `flags()` a total transmute.
    Unused0110 = 0b0110,
    Unused0111 = 0b0111,

    PromoKnight = 0b1000,
    PromoBishop = 0b1001,
    PromoRook = 0b1010,
    PromoQueen = 0b1011,

    PromoCaptureKnight = 0b1100,
    PromoCaptureBishop = 0b1101,
    PromoCaptureRook = 0b1110,
    PromoCaptureQueen = 0b1111,
}

/// Moves are stored in u16:
/// `from` in bits 0..6, `to` in bits 6..12, flags in bits 12..16.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Move(u16);

impl Move {
    pub const NULL: Self = Self(0);

    pub const fn new(from: Square, to: Square, flags: MoveFlags) -> Self {
        debug_assert!(from < 64 && to < 64);
        Self(from as u16 | (to as u16) << 6 | (flags as u16) << 12)
    }

    pub const fn to_bits(self) -> u16 {
        self.0
    }

    pub const fn from_bits(bits: u16) -> Self {
        Self(bits)
    }

    pub const fn from(self) -> Square {
        (self.0 & 0b0011_1111) as Square
    }

    pub const fn to(self) -> Square {
        (self.0 >> 6 & 0b0011_1111) as Square
    }

    pub const fn flags(self) -> MoveFlags {
        unsafe { mem::transmute(self.0 >> 12) }
    }

    pub const fn is_capture(self) -> bool {
        self.0 & (MoveFlags::Capture as u16) << 12 > 0
    }

    pub const fn is_promotion(self) -> bool {
        self.0 & (MoveFlags::PromoKnight as u16) << 12 > 0
    }

    pub const fn is_en_passant(self) -> bool {
        self.0 >> 12 == MoveFlags::EnPassant as u16
    }

    pub const fn is_double_push(self) -> bool {
        self.0 >> 12 == MoveFlags::DoublePush as u16
    }

    pub const fn is_king_castle(self) -> bool {
        self.0 >> 12 == MoveFlags::KingCastle as u16
    }

    pub const fn is_queen_castle(self) -> bool {
        self.0 >> 12 == MoveFlags::QueenCastle as u16
    }

    pub const fn is_castle(self) -> bool {
        self.is_king_castle() || self.is_queen_castle()
    }

    /// The move must be a promotion.
    pub const fn promoted_piece(self) -> Piece {
        const PIECES: [Piece; 4] = [Piece::Knight, Piece::Bishop, Piece::Rook, Piece::Queen];
        debug_assert!(self.is_promotion());
        PIECES[(self.0 >> 12 & 0b11) as usize]
    }
}

/// Long algebraic notation
impl fmt::Display for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let square = |s: Square| [(b'a' + file_of(s)) as char, (b'1' + rank_of(s)) as char];
        let [from_file, from_rank] = square(self.from());
        let [to_file, to_rank] = square(self.to());
        write!(f, "{from_file}{from_rank}{to_file}{to_rank}")?;
        if self.is_promotion() {
            write!(f, "{}", self.promoted_piece().to_char())?;
        }
        Ok(())
    }
}

impl fmt::Debug for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Move({self}, flags {:04b})", self.flags() as u16)
    }
}
