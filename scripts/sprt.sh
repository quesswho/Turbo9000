#!/bin/sh
# Play the working tree against a baseline revision until SPRT decides.
set -eu

if [ $# -lt 1 ]; then
    echo "usage: $0 <baseline-ref> [fastchess options...]" >&2
    exit 1
fi

cd "$(dirname "$0")/.."

sha=$(git rev-parse --short "$1")
shift

book=${BOOK:-books/8moves_v3.pgn}
baseline=.sprt/bin/$sha

if [ ! -x external/fastchess/fastchess ]; then
    echo "no fastchess: make -C external/fastchess -j" >&2
    exit 1
fi

if [ ! -f "$book" ]; then
    echo "no opening book: scripts/get-book.sh" >&2
    exit 1
fi

cargo build --release

if [ ! -x "$baseline" ]; then
    mkdir -p .sprt/bin
    git worktree add --detach .sprt/src "$sha" >/dev/null
    cargo build --release --manifest-path .sprt/src/Cargo.toml --target-dir .sprt/target
    cp .sprt/target/release/turbo9000 "$baseline"
    git worktree remove --force .sprt/src
fi

exec external/fastchess/fastchess \
    -engine cmd=target/release/turbo9000 name=new \
    -engine cmd="$baseline" name="base-$sha" \
    -each tc="${TC:-8+0.08}" \
    -openings file="$book" format=pgn order=random \
    -rounds 100000 -games 2 -repeat \
    -concurrency "${CONCURRENCY:-5}" \
    -sprt elo0="${ELO0:-0}" elo1="${ELO1:-10}" alpha=0.05 beta=0.05 model=normalized \
    -pgnout file=.sprt/sprt.pgn \
    -config outname=.sprt/config.json \
    "$@"
