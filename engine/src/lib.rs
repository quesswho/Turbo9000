pub mod movegen;
pub mod moves;
pub mod nnue;
pub mod perft;
pub mod position;
pub mod search;
pub mod ttable;

pub const NAME: &str = concat!("Turbo9000 ", env!("CARGO_PKG_VERSION"));

mod lookup;
mod zobrist;
