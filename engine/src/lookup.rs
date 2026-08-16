//! Filling the magic table walks every occupancy subset of every square, which
//! trips the compiler's const evaluation budget. It costs about 16 seconds on
//! a release build and nothing at runtime.
#![allow(long_running_const_eval)]

use crate::position::{bit, file_of, rank_of, square, BitBoard, Square, EMPTY};

const KNIGHT_DELTAS: [(i8, i8); 8] = [
    (1, 2),
    (2, 1),
    (2, -1),
    (1, -2),
    (-1, -2),
    (-2, -1),
    (-2, 1),
    (-1, 2),
];

const KING_DELTAS: [(i8, i8); 8] = [
    (0, 1),
    (1, 1),
    (1, 0),
    (1, -1),
    (0, -1),
    (-1, -1),
    (-1, 0),
    (-1, 1),
];

const ROOK_DELTAS: [(i8, i8); 4] = [(0, 1), (1, 0), (0, -1), (-1, 0)];
const BISHOP_DELTAS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, -1), (-1, 1)];

const fn on_board(file: i8, rank: i8) -> bool {
    0 <= file && file < 8 && 0 <= rank && rank < 8
}

const fn step_attacks(deltas: &[(i8, i8); 8]) -> [BitBoard; 64] {
    let mut table = [EMPTY; 64];
    let mut from = 0;
    while from < 64 {
        let file = file_of(from as Square) as i8;
        let rank = rank_of(from as Square) as i8;
        let mut attacks = EMPTY;
        let mut i = 0;
        while i < deltas.len() {
            let (df, dr) = deltas[i];
            let (f, r) = (file + df, rank + dr);
            if on_board(f, r) {
                attacks |= bit(square(f as u8, r as u8));
            }
            i += 1;
        }
        table[from] = attacks;
        from += 1;
    }
    table
}

pub static KNIGHT_ATTACKS: [BitBoard; 64] = step_attacks(&KNIGHT_DELTAS);
pub static KING_ATTACKS: [BitBoard; 64] = step_attacks(&KING_DELTAS);

const FILE_A: BitBoard = 0x0101_0101_0101_0101;
const FILE_H: BitBoard = 0x8080_8080_8080_8080;

pub const fn pawn_attacks<const WHITE: bool>(pawns: BitBoard) -> BitBoard {
    let (west, east) = (pawns & !FILE_A, pawns & !FILE_H);
    if WHITE {
        (west << 7) | (east << 9)
    } else {
        (east >> 7) | (west >> 9)
    }
}

pub const fn pawn_push<const WHITE: bool>(pawns: BitBoard) -> BitBoard {
    if WHITE { pawns << 8 } else { pawns >> 8 }
}

const fn slider_attacks(from: Square, occupied: BitBoard, deltas: &[(i8, i8); 4]) -> BitBoard {
    let mut attacks = EMPTY;
    let mut i = 0;
    while i < deltas.len() {
        let (df, dr) = deltas[i];
        let mut f = file_of(from) as i8 + df;
        let mut r = rank_of(from) as i8 + dr;
        while on_board(f, r) {
            let to = square(f as u8, r as u8);
            attacks |= bit(to);
            if occupied & bit(to) != EMPTY {
                break;
            }
            f += df;
            r += dr;
        }
        i += 1;
    }
    attacks
}

/// The squares whose occupancy changes where a ray stops. The last square of
/// each ray is left out: a blocker there is not blocking anything.
const fn relevant_masks(deltas: &[(i8, i8); 4]) -> [BitBoard; 64] {
    let mut table = [EMPTY; 64];
    let mut from = 0;
    while from < 64 {
        let mut mask = EMPTY;
        let mut i = 0;
        while i < deltas.len() {
            let (df, dr) = deltas[i];
            let mut f = file_of(from as Square) as i8 + df;
            let mut r = rank_of(from as Square) as i8 + dr;
            while on_board(f, r) && on_board(f + df, r + dr) {
                mask |= bit(square(f as u8, r as u8));
                f += df;
                r += dr;
            }
            i += 1;
        }
        table[from] = mask;
        from += 1;
    }
    table
}

/// The squares a slider has to cross to get from one square to the other, the
/// endpoints left out. Empty when the two squares share no rank, file or
/// diagonal.
const fn between_squares() -> [[BitBoard; 64]; 64] {
    let mut table = [[EMPTY; 64]; 64];
    let mut a = 0;
    while a < 64 {
        let mut b = 0;
        while b < 64 {
            let (from, to) = (a as Square, b as Square);
            if slider_attacks(from, EMPTY, &ROOK_DELTAS) & bit(to) != EMPTY {
                table[a][b] = slider_attacks(from, bit(to), &ROOK_DELTAS)
                    & slider_attacks(to, bit(from), &ROOK_DELTAS);
            } else if slider_attacks(from, EMPTY, &BISHOP_DELTAS) & bit(to) != EMPTY {
                table[a][b] = slider_attacks(from, bit(to), &BISHOP_DELTAS)
                    & slider_attacks(to, bit(from), &BISHOP_DELTAS);
            }
            b += 1;
        }
        a += 1;
    }
    table
}

/// The whole rank, file or diagonal through two squares, endpoints included
const fn line_squares() -> [[BitBoard; 64]; 64] {
    let mut table = [[EMPTY; 64]; 64];
    let mut a = 0;
    while a < 64 {
        let mut b = 0;
        while b < 64 {
            let (from, to) = (a as Square, b as Square);
            let ends = bit(from) | bit(to);
            if slider_attacks(from, EMPTY, &ROOK_DELTAS) & bit(to) != EMPTY {
                table[a][b] = (slider_attacks(from, EMPTY, &ROOK_DELTAS)
                    & slider_attacks(to, EMPTY, &ROOK_DELTAS))
                    | ends;
            } else if slider_attacks(from, EMPTY, &BISHOP_DELTAS) & bit(to) != EMPTY {
                table[a][b] = (slider_attacks(from, EMPTY, &BISHOP_DELTAS)
                    & slider_attacks(to, EMPTY, &BISHOP_DELTAS))
                    | ends;
            }
            b += 1;
        }
        a += 1;
    }
    table
}

pub static BETWEEN: [[BitBoard; 64]; 64] = between_squares();
pub static LINE: [[BitBoard; 64]; 64] = line_squares();

const ROOK_SHIFT: u32 = 64 - 12;
const BISHOP_SHIFT: u32 = 64 - 9;

#[derive(Clone, Copy)]
struct Magic {
    not_mask: BitBoard,
    factor: u64,
    offset: u32,
}

impl Magic {
    /// `wrapping_mul` because the overflow is the hash.
    const fn index(&self, occupied: BitBoard, shift: u32) -> usize {
        let hash = (occupied | self.not_mask).wrapping_mul(self.factor) >> shift;
        self.offset as usize + hash as usize
    }
}

/// Black magics found by Volker Annuss and Niklas Fiekas
/// Table uses overlapping packing
/// <https://talkchess.com/forum/viewtopic.php?t=64790>
const MAGIC_TABLE_SIZE: usize = 87988;

const BISHOP_INIT: [(u64, u32); 64] = [
    (0xa7020080601803d8, 60984),
    (0x13802040400801f1, 66046),
    (0x0a0080181001f60c, 32910),
    (0x1840802004238008, 16369),
    (0xc03fe00100000000, 42115),
    (0x24c00bffff400000, 835),
    (0x0808101f40007f04, 18910),
    (0x100808201ec00080, 25911),
    (0xffa2feffbfefb7ff, 63301),
    (0x083e3ee040080801, 16063),
    (0xc0800080181001f8, 17481),
    (0x0440007fe0031000, 59361),
    (0x2010007ffc000000, 18735),
    (0x1079ffe000ff8000, 61249),
    (0x3c0708101f400080, 68938),
    (0x080614080fa00040, 61791),
    (0x7ffe7fff817fcff9, 21893),
    (0x7ffebfffa01027fd, 62068),
    (0x53018080c00f4001, 19829),
    (0x407e0001000ffb8a, 26091),
    (0x201fe000fff80010, 15815),
    (0xffdfefffde39ffef, 16419),
    (0xcc8808000fbf8002, 59777),
    (0x7ff7fbfff8203fff, 16288),
    (0x8800013e8300c030, 33235),
    (0x0420009701806018, 15459),
    (0x7ffeff7f7f01f7fd, 15863),
    (0x8700303010c0c006, 75555),
    (0xc800181810606000, 79445),
    (0x20002038001c8010, 15917),
    (0x087ff038000fc001, 8512),
    (0x00080c0c00083007, 73069),
    (0x00000080fc82c040, 16078),
    (0x000000407e416020, 19168),
    (0x00600203f8008020, 11056),
    (0xd003fefe04404080, 62544),
    (0xa00020c018003088, 80477),
    (0x7fbffe700bffe800, 75049),
    (0x107ff00fe4000f90, 32947),
    (0x7f8fffcff1d007f8, 59172),
    (0x0000004100f88080, 55845),
    (0x00000020807c4040, 61806),
    (0x00000041018700c0, 73601),
    (0x0010000080fc4080, 15546),
    (0x1000003c80180030, 45243),
    (0xc10000df80280050, 20333),
    (0xffffffbfeff80fdc, 33402),
    (0x000000101003f812, 25917),
    (0x0800001f40808200, 32875),
    (0x084000101f3fd208, 4639),
    (0x080000000f808081, 17077),
    (0x0004000008003f80, 62324),
    (0x08000001001fe040, 18159),
    (0x72dd000040900a00, 61436),
    (0xfffffeffbfeff81d, 57073),
    (0xcd8000200febf209, 61025),
    (0x100000101ec10082, 81259),
    (0x7fbaffffefe0c02f, 64083),
    (0x7f83fffffff07f7f, 56114),
    (0xfff1fffffff7ffc1, 57058),
    (0x0878040000ffe01f, 58912),
    (0x945e388000801012, 22194),
    (0x0840800080200fda, 70880),
    (0x100000c05f582008, 11140),
];

const ROOK_INIT: [(u64, u32); 64] = [
    (0x80280013ff84ffff, 10890),
    (0x5ffbfefdfef67fff, 50579),
    (0xffeffaffeffdffff, 62020),
    (0x003000900300008a, 67322),
    (0x0050028010500023, 80251),
    (0x0020012120a00020, 58503),
    (0x0030006000c00030, 51175),
    (0x0058005806b00002, 83130),
    (0x7fbff7fbfbeafffc, 50430),
    (0x0000140081050002, 21613),
    (0x0000180043800048, 72625),
    (0x7fffe800021fffb8, 80755),
    (0xffffcffe7fcfffaf, 69753),
    (0x00001800c0180060, 26973),
    (0x4f8018005fd00018, 84972),
    (0x0000180030620018, 31958),
    (0x00300018010c0003, 69272),
    (0x0003000c0085ffff, 48372),
    (0xfffdfff7fbfefff7, 65477),
    (0x7fc1ffdffc001fff, 43972),
    (0xfffeffdffdffdfff, 57154),
    (0x7c108007befff81f, 53521),
    (0x20408007bfe00810, 30534),
    (0x0400800558604100, 16548),
    (0x0040200010080008, 46407),
    (0x0010020008040004, 11841),
    (0xfffdfefff7fbfff7, 21112),
    (0xfebf7dfff8fefff9, 44214),
    (0xc00000ffe001ffe0, 57925),
    (0x4af01f00078007c3, 29574),
    (0xbffbfafffb683f7f, 17309),
    (0x0807f67ffa102040, 40143),
    (0x200008e800300030, 64659),
    (0x0000008780180018, 70469),
    (0x0000010300180018, 62917),
    (0x4000008180180018, 60997),
    (0x008080310005fffa, 18554),
    (0x4000188100060006, 14385),
    (0xffffff7fffbfbfff, 0),
    (0x0000802000200040, 38091),
    (0x20000202ec002800, 25122),
    (0xfffff9ff7cfff3ff, 60083),
    (0x000000404b801800, 72209),
    (0x2000002fe03fd000, 67875),
    (0xffffff6ffe7fcffd, 56290),
    (0xbff7efffbfc00fff, 43807),
    (0x000000100800a804, 73365),
    (0x6054000a58005805, 76398),
    (0x0829000101150028, 20024),
    (0x00000085008a0014, 9513),
    (0x8000002b00408028, 24324),
    (0x4000002040790028, 22996),
    (0x7800002010288028, 23213),
    (0x0000001800e08018, 56002),
    (0xa3a80003f3a40048, 22809),
    (0x2003d80000500028, 44545),
    (0xfffff37eefefdfbe, 36072),
    (0x40000280090013c1, 4750),
    (0xbf7ffeffbffaf71f, 6014),
    (0xfffdffff777b7d6e, 36054),
    (0x48300007e8080c02, 78538),
    (0xafe0000fff780402, 28745),
    (0xee73fffbffbb77fe, 8555),
    (0x0002000308482882, 1009),
];

const fn magics(init: &[(u64, u32); 64], deltas: &[(i8, i8); 4]) -> [Magic; 64] {
    let masks = relevant_masks(deltas);
    let mut table = [Magic {
        not_mask: EMPTY,
        factor: 0,
        offset: 0,
    }; 64];
    let mut from = 0;
    while from < 64 {
        let (factor, offset) = init[from];
        table[from] = Magic {
            not_mask: !masks[from],
            factor,
            offset,
        };
        from += 1;
    }
    table
}

static ROOK_MAGICS: [Magic; 64] = magics(&ROOK_INIT, &ROOK_DELTAS);
static BISHOP_MAGICS: [Magic; 64] = magics(&BISHOP_INIT, &BISHOP_DELTAS);

const fn fill(
    table: &mut [BitBoard; MAGIC_TABLE_SIZE],
    init: &[(u64, u32); 64],
    deltas: &[(i8, i8); 4],
    shift: u32,
) {
    let magics = magics(init, deltas);
    let mut from = 0;
    while from < 64 {
        let mask = !magics[from].not_mask;
        let mut occupied = EMPTY;
        loop {
            let attacks = slider_attacks(from as Square, occupied, deltas);
            let index = magics[from].index(occupied, shift);
            if table[index] != EMPTY && table[index] != attacks {
                panic!("magic constants collide");
            }
            table[index] = attacks;
            // Carry rippler, walking every subset of the mask.
            occupied = occupied.wrapping_sub(mask) & mask;
            if occupied == EMPTY {
                break;
            }
        }
        from += 1;
    }
}

const fn attack_table() -> [BitBoard; MAGIC_TABLE_SIZE] {
    let mut table = [EMPTY; MAGIC_TABLE_SIZE];
    fill(&mut table, &ROOK_INIT, &ROOK_DELTAS, ROOK_SHIFT);
    fill(&mut table, &BISHOP_INIT, &BISHOP_DELTAS, BISHOP_SHIFT);
    table
}

static ATTACKS: [BitBoard; MAGIC_TABLE_SIZE] = attack_table();

pub fn rook_attacks(from: Square, occupied: BitBoard) -> BitBoard {
    ATTACKS[ROOK_MAGICS[from as usize].index(occupied, ROOK_SHIFT)]
}

pub fn bishop_attacks(from: Square, occupied: BitBoard) -> BitBoard {
    ATTACKS[BISHOP_MAGICS[from as usize].index(occupied, BISHOP_SHIFT)]
}

pub fn queen_attacks(from: Square, occupied: BitBoard) -> BitBoard {
    rook_attacks(from, occupied) | bishop_attacks(from, occupied)
}
