mod game;
mod pack;
mod rng;

use std::fs::File;
use std::io::{BufWriter, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::Instant;

use engine::ttable::TranspositionTable;

use crate::game::play;
use crate::rng::Rng;

/// Per thread, so the table never has to be shared.
const TABLE_MB: usize = 8;

const PROGRESS: u64 = 100_000;

fn main() {
    let mut arguments = std::env::args().skip(1);
    let (Some(output), Some(target)) = (arguments.next(), arguments.next()) else {
        eprintln!("usage: datagen <output> <positions> [threads] [depth] [seed]");
        std::process::exit(2);
    };
    let target: u64 = target.parse().expect("positions is not a number");
    let threads: usize = match arguments.next() {
        Some(text) => text.parse().expect("threads is not a number"),
        None => thread::available_parallelism().map_or(1, |count| count.get()),
    };
    let depth: u32 = match arguments.next() {
        Some(text) => text.parse().expect("depth is not a number"),
        None => 6,
    };
    let seed: u64 = match arguments.next() {
        Some(text) => text.parse().expect("seed is not a number"),
        None => 0x2545_F491_4F6C_DD1D,
    };

    let file = File::create(&output).expect("cannot create the output file");
    let writer = Mutex::new(BufWriter::new(file));
    let written = AtomicU64::new(0);
    let reported = AtomicU64::new(0);
    let start = Instant::now();

    thread::scope(|scope| {
        for index in 0..threads {
            let writer = &writer;
            let written = &written;
            let reported = &reported;
            scope.spawn(move || {
                let table = TranspositionTable::new(TABLE_MB);
                let mut rng = Rng::new(seed ^ (index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
                let mut records = Vec::new();
                while written.load(Ordering::Relaxed) < target {
                    records.clear();
                    play(&mut rng, &table, depth, &mut records);
                    if records.is_empty() {
                        continue;
                    }
                    let mut writer = writer.lock().expect("the writer is poisoned");
                    for record in &records {
                        writer.write_all(record).expect("cannot write a record");
                    }
                    drop(writer);

                    let total = written.fetch_add(records.len() as u64, Ordering::Relaxed)
                        + records.len() as u64;
                    let milestone = total / PROGRESS;
                    if milestone > reported.swap(milestone, Ordering::Relaxed) {
                        let elapsed = start.elapsed().as_secs_f64().max(1e-9);
                        eprintln!(
                            "{total} positions, {:.0}/s",
                            total as f64 / elapsed
                        );
                    }
                }
            });
        }
    });

    writer
        .lock()
        .expect("the writer is poisoned")
        .flush()
        .expect("cannot flush the output file");
    eprintln!(
        "wrote {} positions to {output} in {:.1}s",
        written.load(Ordering::Relaxed),
        start.elapsed().as_secs_f64()
    );
}
