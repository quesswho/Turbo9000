# Trainer

Trains the `768 -> HIDDEN x2 -> 1` net that `engine/src/nnue.rs` evaluates
with, on the bulletformat records `datagen` writes.

Its own workspace, so `cargo build` at the repository root never compiles the
training backend. Data lives outside the repository, in
`../../../turbo9000-data/`.

## Requirements

- Rust >= 1.87, bullet's MSRV. A toolchain that leaves the system rustc alone:

      export CARGO_HOME=/home/miles/Dev/.rust/cargo
      export RUSTUP_HOME=/home/miles/Dev/.rust/rustup
      export PATH=/home/miles/Dev/.rust/cargo/bin:$PATH

- A backend. Upstream bullet needs CUDA, ROCm or Metal. On a machine with
  none of those, the `cpu` feature builds the host backend added by
  github.com/quesswho/turbo-trainer, a fork of github.com/jw1912/bullet that
  compiles the generated kernels with the system c++ and runs them under
  OpenMP. `Cargo.toml` pins that fork by revision.

## 1. Shuffle the data

`datagen` writes records game by game, so consecutive positions differ by one
move. Training on that order gives badly correlated batches. From a checkout
of the fork:

    cargo r -r --bin bullet-utils -- shuffle \
        --input ../turbo9000-data/<revision>/data.bin \
        --output ../turbo9000-data/shuffled.bin \
        --mem-used-mb 4096

    cargo r -r --bin bullet-utils -- validate \
        --input ../turbo9000-data/shuffled.bin

## 2. Train

    cargo run --release --features cpu        # or cuda, rocm, metal

The CPU backend runs at roughly 95k positions/sec at `HL = 128`, 50k at 256,
so 40 superbatches take about 70 minutes. A GTX 1080 with `--features cuda`
takes a few minutes.

Checkpoints land in `checkpoints/turbo9000-01-<superbatch>/`, every 5
superbatches. A run that dies partway resumes from its last one:

    RESUME_FROM=checkpoints/turbo9000-01-35 START_SUPERBATCH=36 \
        cargo r -r --features cpu

## 3. Install the net

`quantised.bin` is padded to a multiple of 64 bytes. `engine/src/nnue.rs`
transmutes exactly 197378 bytes at `HL = 128`, so drop the 62 bytes of
padding:

    head -c 197378 checkpoints/turbo9000-01-40/quantised.bin \
        > ../../engine/src/net.bin

Then SPRT against the previous revision with `tools/sprt.sh`.

## Settings that matter

- `WDL_PROPORTION` is the weight on the **game result**, not on the score.
  bullet computes `target = blend * result + (1 - blend) * sigmoid(score)`,
  so higher means more game result. This is the opposite polarity to
  nnue-pytorch's `lambda`. Generation 0 scores are a depth 6 material
  readout, 20% of them exactly 0 and 45% exact multiples of 100, so the
  result carried most of the signal and the value is 0.8. Once the data is
  regenerated with a net in the search the scores are worth more and this
  should come down.
- `HL` must equal `HIDDEN` in `engine/src/nnue.rs`. A mismatch is a compile
  error in the engine, not silent corruption.
- Weights are saved column major, which for `l0w` of shape `HL x 768` is
  feature major, matching `[[i16; HIDDEN]; 768]` in `nnue.rs`. Do not
  transpose.
