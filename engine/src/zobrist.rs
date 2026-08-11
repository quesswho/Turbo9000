const fn splitmix64(mut state: u64) -> u64 {
    state = state.wrapping_add(0x9E3779B97F4A7C15);
    let mut z = state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

const fn piece_seed(color: usize, kind: usize, square: usize) -> u64 {
    ((color as u64) << 20) ^ ((kind as u64) << 12) ^ ((square as u64) << 2) ^ 0xA5A5_A5A5_A5A5_A5A5
}

const fn generate_piece_square_keys() -> [[[u64; 64]; 6]; 2] {
    let mut keys = [[[0_u64; 64]; 6]; 2];
    let mut color = 0;
    while color < 2 {
        let mut kind = 0;
        while kind < 6 {
            let mut square = 0;
            while square < 64 {
                keys[color][kind][square] = splitmix64(piece_seed(color, kind, square));
                square += 1;
            }
            kind += 1;
        }
        color += 1;
    }
    keys
}

pub const PIECE_SQUARE_KEYS: [[[u64; 64]; 6]; 2] = generate_piece_square_keys();
pub const SIDE_TO_MOVE_KEY: u64 = splitmix64(0x0123_4567_89AB_CDEF);
