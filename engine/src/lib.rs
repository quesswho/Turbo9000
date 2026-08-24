pub mod lookup;
pub mod movegen;
pub mod moves;
pub mod perft;
pub mod position;
pub mod search;
pub mod ttable;
pub mod zobrist;

pub const NAME: &str = "Turbo9000";

pub fn hello() -> String {
    format!("Hello World from {NAME}")
}
