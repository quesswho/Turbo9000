use crate::position::{CastlingRights, ColoredPiece, Square};

const PIECE_KEYS: usize = 12 * 64;
const CASTLING_KEYS: usize = 16;
const EN_PASSANT_KEYS: usize = 8;

const SEED: u64 = 0x2545_F491_4F6C_DD1D;

struct Keys {
    piece: [u64; PIECE_KEYS],
    castling: [u64; CASTLING_KEYS],
    en_passant: [u64; EN_PASSANT_KEYS],
    side: u64,
}

const fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

const fn build_keys() -> Keys {
    let mut state = SEED;
    let mut keys = Keys {
        piece: [0; PIECE_KEYS],
        castling: [0; CASTLING_KEYS],
        en_passant: [0; EN_PASSANT_KEYS],
        side: 0,
    };
    let mut i = 0;
    while i < keys.piece.len() {
        keys.piece[i] = splitmix64(&mut state);
        i += 1;
    }
    let mut i = 0;
    while i < keys.castling.len() {
        keys.castling[i] = splitmix64(&mut state);
        i += 1;
    }
    let mut i = 0;
    while i < keys.en_passant.len() {
        keys.en_passant[i] = splitmix64(&mut state);
        i += 1;
    }
    keys.side = splitmix64(&mut state);
    keys
}

static KEYS: Keys = build_keys();

pub const fn piece_key(piece: ColoredPiece, square: Square) -> u64 {
    KEYS.piece[piece as usize * 64 + square as usize]
}

pub const fn castling_key(rights: CastlingRights) -> u64 {
    KEYS.castling[rights.0 as usize]
}

pub const fn en_passant_key(file: u8) -> u64 {
    KEYS.en_passant[file as usize]
}

pub const fn side_key() -> u64 {
    KEYS.side
}
