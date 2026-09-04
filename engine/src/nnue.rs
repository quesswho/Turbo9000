use std::mem;

use crate::position::{Color, Piece, Position, Square, file_of};
use crate::search::Score;

pub const HIDDEN: usize = 512;

const FEATURES: usize = Color::COUNT * Piece::COUNT * 64;

/// The king bucket of each square, seen from the king's own side and mirrored
/// onto the files a to d, so index `rank * 4 + file`.
#[rustfmt::skip]
const KING_BUCKETS: [usize; 32] = [
    0, 0, 1, 1,
    0, 0, 1, 1,
    2, 2, 2, 2,
    2, 2, 2, 2,
    3, 3, 3, 3,
    3, 3, 3, 3,
    3, 3, 3, 3,
    3, 3, 3, 3,
];

const KING_BUCKET_COUNT: usize = 4;

const OUTPUT_BUCKETS: usize = 8;

/// Pieces per output bucket, as bullet's `MaterialCount` divides them.
const PIECES_PER_BUCKET: u32 = 32u32.div_ceil(OUTPUT_BUCKETS as u32);

const QA: i32 = 255;
const QB: i32 = 64;

const SCALE: i32 = 400;

#[repr(C, align(16))]
struct Network {
    feature_weights: [[i16; HIDDEN]; FEATURES * KING_BUCKET_COUNT],
    feature_bias: [i16; HIDDEN],
    output_weights: [[[i16; HIDDEN]; Color::COUNT]; OUTPUT_BUCKETS],
    output_bias: [i16; OUTPUT_BUCKETS],
}

static NET: Network = unsafe { mem::transmute(*include_bytes!("net.bin")) };

/// How one side sees the board: the weights of its king bucket, and the
/// transform that puts its own side at the bottom and its king on files a to d.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct View {
    offset: usize,
    xor: Square,
}

impl View {
    const fn new(perspective: Color, king: Square) -> Self {
        let flip = if perspective.is_white() { 0 } else { 56 };
        let king = king ^ flip;
        let mirror = if file_of(king) > 3 { 7 } else { 0 };
        let bucket = KING_BUCKETS[(king / 8 * 4 + (file_of(king) ^ mirror)) as usize];
        Self {
            offset: bucket * FEATURES,
            xor: flip | mirror,
        }
    }
}

const fn feature(
    view: View,
    perspective: Color,
    color: Color,
    piece: Piece,
    square: Square,
) -> usize {
    let theirs = (perspective.index() != color.index()) as usize;
    let square = square ^ view.xor;
    view.offset + (theirs * Piece::COUNT + piece.index()) * 64 + square as usize
}

const fn output_bucket(pieces: u32) -> usize {
    ((pieces - 2) / PIECES_PER_BUCKET) as usize
}

fn activate(value: i16, weight: i16) -> i32 {
    let clipped = value.clamp(0, QA as i16);
    i32::from(clipped * weight) * i32::from(clipped)
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Accumulator {
    values: [[i16; HIDDEN]; Color::COUNT],
    views: [View; Color::COUNT],
}

impl Accumulator {
    pub fn empty() -> Self {
        Self {
            values: [NET.feature_bias; Color::COUNT],
            views: [
                View::new(Color::ALL[0], 0),
                View::new(Color::ALL[1], 56),
            ],
        }
    }

    pub fn add(&mut self, piece: Piece, color: Color, square: Square) {
        self.update::<true>(piece, color, square);
    }

    pub fn remove(&mut self, piece: Piece, color: Color, square: Square) {
        self.update::<false>(piece, color, square);
    }

    /// A piece that stays on the board in one pass, since both of its features
    /// share the accumulator loads and stores.
    pub fn move_piece(&mut self, piece: Piece, color: Color, from: Square, to: Square) {
        for perspective in Color::ALL {
            let view = self.views[perspective.index()];
            let sub = &NET.feature_weights[feature(view, perspective, color, piece, from)];
            let add = &NET.feature_weights[feature(view, perspective, color, piece, to)];
            let values = &mut self.values[perspective.index()];
            for ((value, &sub), &add) in values.iter_mut().zip(sub).zip(add) {
                *value += add - sub;
            }
        }
    }

    /// True while the side still sees the board through the view its values
    /// were accumulated with.
    pub fn sees(&self, perspective: Color, king: Square) -> bool {
        self.views[perspective.index()] == View::new(perspective, king)
    }

    /// Drops one side back to the bias, ready for its features to be added
    /// through the view its king now gives it.
    pub fn reset(&mut self, perspective: Color, king: Square) {
        self.values[perspective.index()] = NET.feature_bias;
        self.views[perspective.index()] = View::new(perspective, king);
    }

    pub fn add_for(&mut self, perspective: Color, piece: Piece, color: Color, square: Square) {
        self.accumulate::<true>(perspective, piece, color, square);
    }

    fn update<const ADD: bool>(&mut self, piece: Piece, color: Color, square: Square) {
        for perspective in Color::ALL {
            self.accumulate::<ADD>(perspective, piece, color, square);
        }
    }

    fn accumulate<const ADD: bool>(
        &mut self,
        perspective: Color,
        piece: Piece,
        color: Color,
        square: Square,
    ) {
        let view = self.views[perspective.index()];
        let weights = &NET.feature_weights[feature(view, perspective, color, piece, square)];
        let values = &mut self.values[perspective.index()];
        for (value, &weight) in values.iter_mut().zip(weights) {
            *value = if ADD { *value + weight } else { *value - weight };
        }
    }
}

pub fn evaluate(position: &Position) -> Score {
    let accumulator = position.accumulator();
    let us = position.side_to_move();
    let bucket = output_bucket(position.occupied().count_ones());

    let mut sum = 0;
    for (perspective, weights) in [us, us.flip()]
        .into_iter()
        .zip(&NET.output_weights[bucket])
    {
        let values = &accumulator.values[perspective.index()];
        for (&value, &weight) in values.iter().zip(weights) {
            sum += activate(value, weight);
        }
    }

    let output = i64::from(sum) / i64::from(QA) + i64::from(NET.output_bias[bucket]);
    (output * i64::from(SCALE) / i64::from(QA * QB)) as Score
}
