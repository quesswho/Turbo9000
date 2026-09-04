# Trainer

Trains the `768x4 -> HIDDEN x2 -> 8` net that `engine/src/nnue.rs` evaluates
with, on the bulletformat records `datagen` writes. The inputs are king
bucketed and horizontally mirrored, and the output layer is bucketed by piece
count.

Its own workspace, so `cargo build` at the repository root never compiles the
training backend. Data lives in `data/`, which git ignores.

## Requirements

- Rust >= 1.87, bullet's MSRV.

- A backend. The `cuda` feature links `cudart`, `nvrtc` and `cublas`, which
  the driver alone does not ship. On Fedora:

      sudo dnf config-manager addrepo --from-repofile=https://developer.download.nvidia.com/compute/cuda/repos/fedora44/x86_64/cuda-fedora44.repo
      sudo dnf install cuda-cudart-devel-13-3 cuda-nvrtc-devel-13-3 libcublas-devel-13-3
      export CUDA_PATH=/usr/local/cuda-13.3

  On a machine with no GPU at all, the `cpu` feature builds the host backend
  added by github.com/quesswho/turbo-trainer, a fork of github.com/jw1912/bullet
  that compiles the generated kernels with the system c++ and runs them under
  OpenMP. `Cargo.toml` pins that fork by revision.

## 1. Shuffle the data

`datagen` writes records game by game, so consecutive positions differ by one
move. Training on that order gives badly correlated batches. From a checkout
of the fork:

    cargo r -r --bin bullet-utils -- shuffle \
        --input ../Turbo9000/data/run2.data \
        --output ../Turbo9000/data/shuffled.data \
        --mem-used-mb 4096

    cargo r -r --bin bullet-utils -- validate \
        --input ../Turbo9000/data/shuffled.data

## 2. Train

    cargo run --release --features cuda         # or cpu, rocm, metal

Checkpoints land in `checkpoints/turbo9000-02-<superbatch>/`, every 5
superbatches. A run that dies partway resumes from its last one:

    RESUME_FROM=checkpoints/turbo9000-02-25 START_SUPERBATCH=26 \
        cargo r -r --features cuda

## 3. Install the net

`quantised.bin` is padded to a multiple of 64 bytes. `engine/src/nnue.rs`
transmutes exactly 3163152 bytes at `HL = 512`, so drop the 48 bytes of
padding:

    head -c 3163152 checkpoints/turbo9000-02-30/quantised.bin \
        > ../../engine/src/net.bin

Then SPRT against the previous revision with `tools/sprt.sh`.

## Settings that matter

- `WDL_PROPORTION` is the weight on the **game result**, not on the score.
  bullet computes `target = blend * result + (1 - blend) * sigmoid(score)`,
  so higher means more game result. This is the opposite polarity to
  nnue-pytorch's `lambda`. Generation 0 scores were a depth 6 material
  readout and the value was 0.8; generation 1 scores come out of a real
  search, only 1% of them exactly 0 against 20% before, so the score is worth
  much more and the value is 0.5.
- `HL`, `KING_BUCKETS` and `OUTPUT_BUCKETS` must match `engine/src/nnue.rs`.
  `HL` mismatches are a compile error in the engine, the bucket layouts are
  not, so they are duplicated with a comment on both sides.
- Weights are saved column major, which for `l0w` of shape `HL x 768*buckets`
  is feature major, matching `[[i16; HIDDEN]; FEATURES * KING_BUCKET_COUNT]`
  in `nnue.rs`. Do not transpose. `l1w` is the other way round: it has to be
  transposed so each output bucket's `2 * HL` weights are contiguous.
- `l0f` is a factoriser, a bucket agnostic copy of the input weights that all
  buckets train through, so a feature learns from every position and not only
  from the ones whose king sits in its bucket. It is folded into `l0w` when
  the net is saved, so the engine never sees it. Both are clipped to +-0.99
  rather than the default +-1.98, since their sum is what gets quantised.
