use std::mem;

use crate::position::{Color, Piece, Position, Square};
use crate::search::Score;

pub const HIDDEN: usize = 128;

const INPUTS: usize = Color::COUNT * Piece::COUNT * 64;

const QA: i32 = 255;
const QB: i32 = 64;

const SCALE: i32 = 400;

#[repr(C)]
struct Network {
    feature_weights: [[i16; HIDDEN]; INPUTS],
    feature_bias: [i16; HIDDEN],
    output_weights: [[i16; HIDDEN]; Color::COUNT],
    output_bias: i16,
}

const NET: Network = unsafe { mem::transmute(*include_bytes!("net.bin")) };

const fn feature(perspective: Color, color: Color, piece: Piece, square: Square) -> usize {
    let theirs = (perspective.index() != color.index()) as usize;
    let square = if perspective.is_white() { square } else { square ^ 56 };
    (theirs * Piece::COUNT + piece.index()) * 64 + square as usize
}

fn activate(value: i16, weight: i16) -> i32 {
    let clipped = value.clamp(0, QA as i16);
    i32::from(clipped * weight) * i32::from(clipped)
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Accumulator {
    values: [[i16; HIDDEN]; Color::COUNT],
}

impl Accumulator {
    pub const EMPTY: Self = Self {
        values: [NET.feature_bias; Color::COUNT],
    };

    pub fn add(&mut self, piece: Piece, color: Color, square: Square) {
        self.update::<true>(piece, color, square);
    }

    pub fn remove(&mut self, piece: Piece, color: Color, square: Square) {
        self.update::<false>(piece, color, square);
    }

    fn update<const ADD: bool>(&mut self, piece: Piece, color: Color, square: Square) {
        for perspective in Color::ALL {
            let weights = &NET.feature_weights[feature(perspective, color, piece, square)];
            let values = &mut self.values[perspective.index()];
            for (value, &weight) in values.iter_mut().zip(weights) {
                *value = if ADD { *value + weight } else { *value - weight };
            }
        }
    }
}

pub fn evaluate(position: &Position) -> Score {
    let accumulator = position.accumulator();
    let us = position.side_to_move();

    let mut sum = 0;
    for (perspective, weights) in [us, us.flip()].into_iter().zip(&NET.output_weights) {
        let values = &accumulator.values[perspective.index()];
        for (&value, &weight) in values.iter().zip(weights) {
            sum += activate(value, weight);
        }
    }

    let output = i64::from(sum) / i64::from(QA) + i64::from(NET.output_bias);
    (output * i64::from(SCALE) / i64::from(QA * QB)) as Score
}
