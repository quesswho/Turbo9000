mod zobrist;

use zobrist::{PIECE_SQUARE_KEYS, SIDE_TO_MOVE_KEY};

pub const NAME: &str = "Turbo9000";

pub fn hello() -> String {
    format!("Hello World from {NAME}")
}

pub type Square = u8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    White,
    Black,
}

impl Color {
    const fn index(self) -> usize {
        match self {
            Self::White => 0,
            Self::Black => 1,
        }
    }

    const fn opposite(self) -> Self {
        match self {
            Self::White => Self::Black,
            Self::Black => Self::White,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PieceKind {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

impl PieceKind {
    const fn index(self) -> usize {
        match self {
            Self::Pawn => 0,
            Self::Knight => 1,
            Self::Bishop => 2,
            Self::Rook => 3,
            Self::Queen => 4,
            Self::King => 5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece {
    pub color: Color,
    pub kind: PieceKind,
}

impl Piece {
    const fn zobrist_key(self, square: Square) -> u64 {
        PIECE_SQUARE_KEYS[self.color.index()][self.kind.index()][square as usize]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChessMove {
    pub from: Square,
    pub to: Square,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoveError {
    InvalidSquare(Square),
    NoPieceOnFromSquare(Square),
    IllegalSelfCapture,
    EmptyHistory,
}

#[derive(Debug, Clone, Copy)]
struct UndoMove {
    from: Square,
    to: Square,
    moved_piece: Piece,
    captured_piece: Option<Piece>,
    side_to_move: Color,
}

#[derive(Debug, Clone)]
pub struct Position {
    board: [Option<Piece>; 64],
    side_to_move: Color,
    hash: u64,
    history: Vec<UndoMove>,
}

impl Position {
    pub fn empty(side_to_move: Color) -> Self {
        let hash = match side_to_move {
            Color::White => 0,
            Color::Black => SIDE_TO_MOVE_KEY,
        };

        Self {
            board: [None; 64],
            side_to_move,
            hash,
            history: Vec::new(),
        }
    }

    pub fn place_piece(&mut self, square: Square, piece: Piece) -> Result<(), MoveError> {
        let square_idx = Self::square_index(square)?;
        if self.board[square_idx].is_some() {
            return Err(MoveError::IllegalSelfCapture);
        }
        self.board[square_idx] = Some(piece);
        self.hash ^= piece.zobrist_key(square);
        Ok(())
    }

    pub fn piece_at(&self, square: Square) -> Result<Option<Piece>, MoveError> {
        Ok(self.board[Self::square_index(square)?])
    }

    pub fn hash(&self) -> u64 {
        self.hash
    }

    pub fn side_to_move(&self) -> Color {
        self.side_to_move
    }

    pub fn make_move(&mut self, mv: ChessMove) -> Result<(), MoveError> {
        let from_idx = Self::square_index(mv.from)?;
        let to_idx = Self::square_index(mv.to)?;

        let moved_piece = self.board[from_idx].ok_or(MoveError::NoPieceOnFromSquare(mv.from))?;
        let captured_piece = self.board[to_idx];
        if captured_piece.is_some_and(|piece| piece.color == moved_piece.color) {
            return Err(MoveError::IllegalSelfCapture);
        }

        self.hash ^= moved_piece.zobrist_key(mv.from);
        if let Some(piece) = captured_piece {
            self.hash ^= piece.zobrist_key(mv.to);
        }

        self.board[from_idx] = None;
        self.board[to_idx] = Some(moved_piece);
        self.hash ^= moved_piece.zobrist_key(mv.to);

        let previous_side_to_move = self.side_to_move;
        self.side_to_move = self.side_to_move.opposite();
        self.hash ^= SIDE_TO_MOVE_KEY;

        self.history.push(UndoMove {
            from: mv.from,
            to: mv.to,
            moved_piece,
            captured_piece,
            side_to_move: previous_side_to_move,
        });

        Ok(())
    }

    pub fn unmake_move(&mut self) -> Result<(), MoveError> {
        let undo = self.history.pop().ok_or(MoveError::EmptyHistory)?;
        let from_idx = Self::square_index(undo.from)?;
        let to_idx = Self::square_index(undo.to)?;

        self.side_to_move = undo.side_to_move;
        self.hash ^= SIDE_TO_MOVE_KEY;

        self.hash ^= undo.moved_piece.zobrist_key(undo.to);
        self.board[to_idx] = undo.captured_piece;
        if let Some(piece) = undo.captured_piece {
            self.hash ^= piece.zobrist_key(undo.to);
        }

        self.board[from_idx] = Some(undo.moved_piece);
        self.hash ^= undo.moved_piece.zobrist_key(undo.from);

        Ok(())
    }

    pub fn recompute_hash(&self) -> u64 {
        let mut hash = if self.side_to_move == Color::Black {
            SIDE_TO_MOVE_KEY
        } else {
            0
        };

        for square in 0_u8..64 {
            if let Some(piece) = self.board[square as usize] {
                hash ^= piece.zobrist_key(square);
            }
        }

        hash
    }

    fn square_index(square: Square) -> Result<usize, MoveError> {
        if square < 64 {
            Ok(square as usize)
        } else {
            Err(MoveError::InvalidSquare(square))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incremental_hash_matches_recompute_after_moves() {
        let mut position = Position::empty(Color::White);
        position
            .place_piece(
                4,
                Piece {
                    color: Color::White,
                    kind: PieceKind::King,
                },
            )
            .unwrap();
        position
            .place_piece(
                60,
                Piece {
                    color: Color::Black,
                    kind: PieceKind::King,
                },
            )
            .unwrap();
        position
            .place_piece(
                8,
                Piece {
                    color: Color::White,
                    kind: PieceKind::Rook,
                },
            )
            .unwrap();
        position
            .place_piece(
                16,
                Piece {
                    color: Color::Black,
                    kind: PieceKind::Knight,
                },
            )
            .unwrap();

        assert_eq!(position.hash(), position.recompute_hash());

        position.make_move(ChessMove { from: 8, to: 16 }).unwrap();
        assert_eq!(position.hash(), position.recompute_hash());

        position.make_move(ChessMove { from: 60, to: 52 }).unwrap();
        assert_eq!(position.hash(), position.recompute_hash());
    }

    #[test]
    fn unmake_move_restores_position_and_hash() {
        let mut position = Position::empty(Color::White);
        position
            .place_piece(
                4,
                Piece {
                    color: Color::White,
                    kind: PieceKind::King,
                },
            )
            .unwrap();
        position
            .place_piece(
                60,
                Piece {
                    color: Color::Black,
                    kind: PieceKind::King,
                },
            )
            .unwrap();
        position
            .place_piece(
                0,
                Piece {
                    color: Color::White,
                    kind: PieceKind::Rook,
                },
            )
            .unwrap();
        position
            .place_piece(
                8,
                Piece {
                    color: Color::Black,
                    kind: PieceKind::Knight,
                },
            )
            .unwrap();

        let initial_hash = position.hash();
        let initial_side = position.side_to_move();

        position.make_move(ChessMove { from: 0, to: 8 }).unwrap();
        position.unmake_move().unwrap();

        assert_eq!(position.hash(), initial_hash);
        assert_eq!(position.side_to_move(), initial_side);
        assert_eq!(
            position.piece_at(0).unwrap(),
            Some(Piece {
                color: Color::White,
                kind: PieceKind::Rook
            })
        );
        assert_eq!(
            position.piece_at(8).unwrap(),
            Some(Piece {
                color: Color::Black,
                kind: PieceKind::Knight
            })
        );
        assert_eq!(position.hash(), position.recompute_hash());
    }
}
