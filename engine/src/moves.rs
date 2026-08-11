use std::fmt;

use crate::position::{file_of, rank_of, Piece, Square};

/// Bit 3 marks a promotion, bit 2 a capture, so the two tests are single masks.
#[repr(u16)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum MoveFlags {
    Quiet = 0b0000,
    DoublePush = 0b0001,
    KingCastle = 0b0010,
    QueenCastle = 0b0011,
    Capture = 0b0100,
    EnPassant = 0b0101,
    PromoKnight = 0b1000,
    PromoBishop = 0b1001,
    PromoRook = 0b1010,
    PromoQueen = 0b1011,
    PromoCaptureKnight = 0b1100,
    PromoCaptureBishop = 0b1101,
    PromoCaptureRook = 0b1110,
    PromoCaptureQueen = 0b1111,
}

/// `from` in bits 0..6, `to` in bits 6..12, flags in bits 12..16.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Move(u16);

impl Move {
    const fn new(from: Square, to: Square, flags: MoveFlags) -> Self {
        Self(from as u16 | (to as u16) << 6 | (flags as u16) << 12)
    }

    pub const fn quiet(from: Square, to: Square) -> Self {
        Self::new(from, to, MoveFlags::Quiet)
    }

    pub const fn capture(from: Square, to: Square) -> Self {
        Self::new(from, to, MoveFlags::Capture)
    }

    pub const fn double_push(from: Square, to: Square) -> Self {
        Self::new(from, to, MoveFlags::DoublePush)
    }

    pub const fn en_passant(from: Square, to: Square) -> Self {
        Self::new(from, to, MoveFlags::EnPassant)
    }

    pub const fn king_castle(from: Square, to: Square) -> Self {
        Self::new(from, to, MoveFlags::KingCastle)
    }

    pub const fn queen_castle(from: Square, to: Square) -> Self {
        Self::new(from, to, MoveFlags::QueenCastle)
    }

    pub const fn promotion(from: Square, to: Square, piece: Piece, capture: bool) -> Self {
        debug_assert!(piece.index() >= 1 && piece.index() <= 4);
        let mut flags = MoveFlags::PromoKnight as u16 | (piece.index() as u16 - 1);
        if capture {
            flags |= MoveFlags::Capture as u16;
        }
        Self(from as u16 | (to as u16) << 6 | flags << 12)
    }

    pub const fn from(self) -> Square {
        (self.0 & 0x3f) as Square
    }

    pub const fn to(self) -> Square {
        (self.0 >> 6 & 0x3f) as Square
    }

    pub const fn flags(self) -> MoveFlags {
        match self.0 >> 12 {
            0b0000 => MoveFlags::Quiet,
            0b0001 => MoveFlags::DoublePush,
            0b0010 => MoveFlags::KingCastle,
            0b0011 => MoveFlags::QueenCastle,
            0b0100 => MoveFlags::Capture,
            0b0101 => MoveFlags::EnPassant,
            0b1000 => MoveFlags::PromoKnight,
            0b1001 => MoveFlags::PromoBishop,
            0b1010 => MoveFlags::PromoRook,
            0b1011 => MoveFlags::PromoQueen,
            0b1100 => MoveFlags::PromoCaptureKnight,
            0b1101 => MoveFlags::PromoCaptureBishop,
            0b1110 => MoveFlags::PromoCaptureRook,
            0b1111 => MoveFlags::PromoCaptureQueen,
            _ => panic!("invalid move flags"),
        }
    }

    pub const fn is_capture(self) -> bool {
        matches!(
            self.flags(),
            MoveFlags::Capture
                | MoveFlags::PromoCaptureKnight
                | MoveFlags::PromoCaptureBishop
                | MoveFlags::PromoCaptureRook
                | MoveFlags::PromoCaptureQueen
        )
    }

    pub const fn is_en_passant(self) -> bool {
        matches!(self.flags(), MoveFlags::EnPassant)
    }

    pub const fn is_double_push(self) -> bool {
        matches!(self.flags(), MoveFlags::DoublePush)
    }

    pub const fn is_king_castle(self) -> bool {
        matches!(self.flags(), MoveFlags::KingCastle)
    }

    pub const fn is_queen_castle(self) -> bool {
        matches!(self.flags(), MoveFlags::QueenCastle)
    }

    pub const fn is_castle(self) -> bool {
        self.is_king_castle() || self.is_queen_castle()
    }

    pub const fn promoted_piece(self) -> Option<Piece> {
        match self.flags() {
            MoveFlags::PromoKnight | MoveFlags::PromoCaptureKnight => Some(Piece::Knight),
            MoveFlags::PromoBishop | MoveFlags::PromoCaptureBishop => Some(Piece::Bishop),
            MoveFlags::PromoRook | MoveFlags::PromoCaptureRook => Some(Piece::Rook),
            MoveFlags::PromoQueen | MoveFlags::PromoCaptureQueen => Some(Piece::Queen),
            _ => None,
        }
    }
}

/// Long algebraic notation
impl fmt::Display for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let square = |s: Square| [(b'a' + file_of(s)) as char, (b'1' + rank_of(s)) as char];
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
        write!(f, "Move({self}, flags {:04b})", self.flags() as u16)
    }
}
