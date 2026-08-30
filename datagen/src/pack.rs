use engine::position::{Black, Piece, Position, Side, White};
use engine::search::Score;

/// One `bulletformat::ChessBoard`: `occ`, `pcs`, `score`, `result`, `ksq`,
/// `opp_ksq`, `extra`, laid out little endian and white relative.
pub const RECORD: usize = 32;

const RESULT: usize = 26;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    BlackWin = 0,
    Draw = 1,
    WhiteWin = 2,
}

const KINDS: [Piece; Piece::COUNT] = [
    Piece::Pawn,
    Piece::Knight,
    Piece::Bishop,
    Piece::Rook,
    Piece::Queen,
    Piece::King,
];

/// The result is left as a placeholder, it is only known once the game ends.
pub fn pack(position: &Position, score: Score) -> [u8; RECORD] {
    let mut record = [0; RECORD];
    record[..8].copy_from_slice(&position.occupied().to_le_bytes());

    let black = position.color(Black::COLOR);
    let mut occupied = position.occupied();
    let mut index = 0;
    while occupied != 0 {
        let bit = occupied & occupied.wrapping_neg();
        occupied &= occupied - 1;
        let color = u8::from(bit & black != 0) << 3;
        let kind = KINDS
            .iter()
            .position(|&piece| position.pieces_of_kind(piece) & bit != 0)
            .expect("occupied square holds no piece") as u8;
        record[8 + index / 2] |= (color | kind) << (4 * (index & 1));
        index += 1;
    }

    let white = if position.side_to_move().is_white() { score } else { -score };
    record[24..26].copy_from_slice(&(white as i16).to_le_bytes());
    record[27] = position.king_square(White::COLOR);
    record[28] = position.king_square(Black::COLOR) ^ 56;
    record
}

pub fn set_outcome(record: &mut [u8; RECORD], outcome: Outcome) {
    record[RESULT] = outcome as u8;
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine::position::Square;

    fn nibbles(record: &[u8; RECORD]) -> Vec<(Square, u8)> {
        let occupied = u64::from_le_bytes(record[..8].try_into().expect("eight bytes"));
        let mut bits = occupied;
        let mut index = 0;
        let mut squares = Vec::new();
        while bits != 0 {
            let square = bits.trailing_zeros() as Square;
            bits &= bits - 1;
            squares.push((square, (record[8 + index / 2] >> (4 * (index & 1))) & 0xf));
            index += 1;
        }
        squares
    }

    fn round_trip(fen: &str, score: Score) {
        let position: Position = fen.parse().expect("bad fen");
        let record = pack(&position, score);

        assert_eq!(
            u64::from_le_bytes(record[..8].try_into().expect("eight bytes")),
            position.occupied(),
            "{fen}"
        );

        let squares = nibbles(&record);
        assert_eq!(squares.len() as u32, position.occupied().count_ones(), "{fen}");
        for (square, nibble) in squares {
            let color = if nibble & 8 != 0 { Black::COLOR } else { White::COLOR };
            let piece = KINDS[(nibble & 7) as usize];
            assert!(
                position.pieces(piece, color) & (1u64 << square) != 0,
                "{fen}: square {square} decoded as {nibble}"
            );
        }

        let white = if position.side_to_move().is_white() { score } else { -score };
        assert_eq!(
            i16::from_le_bytes(record[24..26].try_into().expect("two bytes")),
            white as i16,
            "{fen}"
        );
        assert_eq!(record[27], position.king_square(White::COLOR), "{fen}");
        assert_eq!(record[28], position.king_square(Black::COLOR) ^ 56, "{fen}");
    }

    #[test]
    fn the_starting_position_round_trips() {
        round_trip("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1", 0);
    }

    #[test]
    fn a_crowded_position_round_trips() {
        round_trip(
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
            123,
        );
    }

    /// An odd piece count leaves the last piece in a high nibble.
    #[test]
    fn an_odd_piece_count_round_trips() {
        const FEN: &str = "8/8/8/4k3/8/8/4K3/4R3 w - - 0 1";
        let position: Position = FEN.parse().expect("bad fen");
        assert_eq!(position.occupied().count_ones() % 2, 1);
        round_trip(FEN, -45);
    }

    /// The record is white relative, so black's score changes sign.
    #[test]
    fn black_to_move_flips_the_score() {
        let position: Position = "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 b - - 0 1"
            .parse()
            .expect("bad fen");
        let record = pack(&position, 50);
        assert_eq!(
            i16::from_le_bytes(record[24..26].try_into().expect("two bytes")),
            -50
        );
    }

    #[test]
    fn the_outcome_lands_in_its_own_byte() {
        let position = Position::starting();
        let mut record = pack(&position, 0);
        assert_eq!(record[RESULT], 0);
        set_outcome(&mut record, Outcome::WhiteWin);
        assert_eq!(record[RESULT], 2);
        set_outcome(&mut record, Outcome::Draw);
        assert_eq!(record[RESULT], 1);
    }
}
