# Turbo9000
A terrible chess engine written in Rust.

## Play against Turbo9000
Turbo9000 has just enough support for UCI and can be played on any GUI that speaks it, for example knights.

    cargo build --release

Add `target/release/turbo9000` as a UCI engine.

## Testing
Games are played with fastchess against a baseline revision:

      git submodule update --init
      make -C external/fastchess -j
      scripts/get-book.sh
      scripts/sprt.sh HEAD~1

