use std::mem;

use crate::position::{Color, ColoredPiece, Piece, Position, Square, file_of};
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

const WEIGHTS: Network = unsafe { mem::transmute(*include_bytes!("net.bin")) };

static NET: Network = WEIGHTS;

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

const OUTPUT_WEIGHT_BOUND: u16 = (i16::MAX / QA as i16) as u16;

const _: () = {
    let weights: [i16; OUTPUT_BUCKETS * Color::COUNT * HIDDEN] =
        unsafe { mem::transmute(WEIGHTS.output_weights) };
    let mut index = 0;
    while index < weights.len() {
        assert!(
            weights[index].unsigned_abs() <= OUTPUT_WEIGHT_BOUND,
            "an output weight overflows the i16 multiply in activate"
        );
        index += 1;
    }
};

/// The features one move turns on and off. A castle moves two pieces and a
/// promotion capture destroys two, so two of each is as many as a move needs.
#[derive(Clone, Copy)]
pub struct Delta {
    added: [(ColoredPiece, Square); 2],
    removed: [(ColoredPiece, Square); 2],
    added_len: usize,
    removed_len: usize,
}

impl Delta {
    pub const fn new() -> Self {
        // The slots past each length are never read.
        const UNUSED: (ColoredPiece, Square) = (ColoredPiece::WhitePawn, 0);
        Self {
            added: [UNUSED; 2],
            removed: [UNUSED; 2],
            added_len: 0,
            removed_len: 0,
        }
    }

    pub fn add(&mut self, colored: ColoredPiece, square: Square) {
        self.added[self.added_len] = (colored, square);
        self.added_len += 1;
    }

    pub fn remove(&mut self, colored: ColoredPiece, square: Square) {
        self.removed[self.removed_len] = (colored, square);
        self.removed_len += 1;
    }
}

/// One pass over the row, so a capture or a castle costs the same loads and
/// stores as a quiet move.
fn combine<const ADDED: usize, const REMOVED: usize>(
    values: &mut [i16; HIDDEN],
    previous: &[i16; HIDDEN],
    added: [&[i16; HIDDEN]; ADDED],
    removed: [&[i16; HIDDEN]; REMOVED],
) {
    for (index, value) in values.iter_mut().enumerate() {
        let mut sum = previous[index];
        for weights in added {
            sum += weights[index];
        }
        for weights in removed {
            sum -= weights[index];
        }
        *value = sum;
    }
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

    /// Writes what `previous` becomes once `delta` is applied, leaving
    /// `previous` untouched so a move is taken back by reading it again.
    pub fn apply(&mut self, previous: &Self, perspective: Color, delta: &Delta) {
        let index = perspective.index();
        let view = previous.views[index];
        self.views[index] = view;

        let row = |(colored, square): (ColoredPiece, Square)| {
            let feature = feature(view, perspective, colored.color(), colored.piece(), square);
            &NET.feature_weights[feature]
        };
        let previous = &previous.values[index];
        let values = &mut self.values[index];
        match (
            &delta.added[..delta.added_len],
            &delta.removed[..delta.removed_len],
        ) {
            ([to], [from]) => combine(values, previous, [row(*to)], [row(*from)]),
            ([to], [victim, from]) => {
                combine(values, previous, [row(*to)], [row(*victim), row(*from)])
            }
            ([to, rook_to], [from, rook_from]) => combine(
                values,
                previous,
                [row(*to), row(*rook_to)],
                [row(*from), row(*rook_from)],
            ),
            _ => unreachable!("a move turns at most two features on and two off"),
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
        let view = self.views[perspective.index()];
        let weights = &NET.feature_weights[feature(view, perspective, color, piece, square)];
        let values = &mut self.values[perspective.index()];
        for (value, &weight) in values.iter_mut().zip(weights) {
            *value += weight;
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
