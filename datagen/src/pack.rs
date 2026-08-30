use engine::position::{BitBoard, Piece, Position, Square};
use engine::search::Score;

/// One `bulletformat::ChessBoard`: `occ`, `pcs`, `score`, `result`, `ksq`,
/// `opp_ksq`, `extra`, laid out little endian and side to move relative.
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

/// Black to move is stored mirrored, so the mover always faces up the board.
fn orient(board: BitBoard, flip: bool) -> BitBoard {
    if flip { board.swap_bytes() } else { board }
}

fn orient_square(square: Square, flip: bool) -> Square {
    if flip { square ^ 56 } else { square }
}

/// The result is left as a placeholder, it is only known once the game ends.
pub fn pack(position: &Position, score: Score) -> [u8; RECORD] {
    let us = position.side_to_move();
    let flip = !us.is_white();

    let mut record = [0; RECORD];
    let occupied = orient(position.occupied(), flip);
    record[..8].copy_from_slice(&occupied.to_le_bytes());

    let opponent = position.color(us.flip());
    let mut bits = occupied;
    let mut index = 0;
    while bits != 0 {
        let square = bits.trailing_zeros() as Square;
        bits &= bits - 1;
        let bit = 1 << orient_square(square, flip);
        let color = u8::from(bit & opponent != 0) << 3;
        let kind = KINDS
            .iter()
            .position(|&piece| position.pieces_of_kind(piece) & bit != 0)
            .expect("occupied square holds no piece") as u8;
        record[8 + index / 2] |= (color | kind) << (4 * (index & 1));
        index += 1;
    }

    record[24..26].copy_from_slice(&(score as i16).to_le_bytes());
    record[27] = orient_square(position.king_square(us), flip);
    record[28] = orient_square(position.king_square(us.flip()), flip) ^ 56;
    record
}

/// `outcome` is white relative, the record stores it for the side to move.
pub fn set_outcome(record: &mut [u8; RECORD], outcome: Outcome, white_to_move: bool) {
    let result = outcome as u8;
    record[RESULT] = if white_to_move { result } else { 2 - result };
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let us = position.side_to_move();
        let flip = !us.is_white();

        assert_eq!(
            u64::from_le_bytes(record[..8].try_into().expect("eight bytes")),
            orient(position.occupied(), flip),
            "{fen}"
        );

        let squares = nibbles(&record);
        assert_eq!(squares.len() as u32, position.occupied().count_ones(), "{fen}");
        for (square, nibble) in squares {
            let color = if nibble & 8 != 0 { us.flip() } else { us };
            let piece = KINDS[(nibble & 7) as usize];
            let origin = orient_square(square, flip);
            assert!(
                position.pieces(piece, color) & (1 << origin) != 0,
                "{fen}: square {square} decoded as {nibble}"
            );
        }

        assert_eq!(
            i16::from_le_bytes(record[24..26].try_into().expect("two bytes")),
            score as i16,
            "{fen}"
        );
        assert_eq!(record[27], orient_square(position.king_square(us), flip), "{fen}");
        assert_eq!(
            record[28],
            orient_square(position.king_square(us.flip()), flip) ^ 56,
            "{fen}"
        );
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

    #[test]
    fn a_black_to_move_position_round_trips() {
        round_trip("8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 b - - 0 1", 50);
    }

    /// The record is mover relative, so a position and its mirror with the
    /// colours swapped pack to the same bytes.
    #[test]
    fn black_to_move_packs_as_its_mirror() {
        let black: Position = "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 b - - 0 1"
            .parse()
            .expect("bad fen");
        let white: Position = "8/4p1p1/8/1r3P1K/kp5R/3P4/2P5/8 w - - 0 1"
            .parse()
            .expect("bad fen");
        assert_eq!(pack(&black, 50), pack(&white, 50));
    }

    #[test]
    fn the_outcome_is_stored_for_the_mover() {
        let position = Position::starting();
        let mut record = pack(&position, 0);
        assert_eq!(record[RESULT], 0);

        set_outcome(&mut record, Outcome::WhiteWin, true);
        assert_eq!(record[RESULT], 2);
        set_outcome(&mut record, Outcome::WhiteWin, false);
        assert_eq!(record[RESULT], 0);
        set_outcome(&mut record, Outcome::Draw, false);
        assert_eq!(record[RESULT], 1);
    }
}
