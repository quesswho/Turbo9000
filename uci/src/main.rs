use std::io::{self, BufRead};

use clap::{Parser, Subcommand, ValueEnum};

use engine::perft::perft;
use engine::position::Position;
use engine::NAME;

#[derive(Parser)]
#[command(multicall = true)]
struct Line {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Uci,
    Isready,
    Ucinewgame,
    Position {
        setup: Setup,
    },
    Go {
        #[command(subcommand)]
        kind: Go,
    },
    Quit,
}

#[derive(Clone, ValueEnum)]
enum Setup {
    Startpos,
}

#[derive(Subcommand)]
enum Go {
    Perft { depth: u32 },
}

fn main() {
    let mut position = Position::starting();

    for line in io::stdin().lock().lines().map_while(Result::ok) {
        let Ok(parsed) = Line::try_parse_from(line.split_whitespace()) else {
            continue;
        };

        match parsed.command {
            Command::Uci => {
                println!("id name {NAME}");
                println!("id author adrian-tudev, quesswho");
                println!("uciok");
            }
            Command::Isready => println!("readyok"),
            Command::Ucinewgame | Command::Position { .. } => position = Position::starting(),
            Command::Go {
                kind: Go::Perft { depth },
            } => println!("Nodes searched: {}", perft(&mut position, depth)),
            Command::Quit => break,
        }
    }
}
