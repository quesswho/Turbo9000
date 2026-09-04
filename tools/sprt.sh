#!/bin/sh
# Play the working tree against a baseline revision until SPRT decides.
# Without a baseline-ref, the newest version tag (v*) is used.
set -eu

if [ $# -ge 1 ]; then
    ref=$1
    shift
else
    ref=$(git tag -l 'v*' --sort=-v:refname | head -n 1)
    if [ -z "$ref" ]; then
        echo "no version tags: tag a baseline or pass a ref explicitly" >&2
        exit 1
    fi
fi

cd "$(dirname "$0")/.."

sha=$(git rev-parse --short "$ref")

book=${BOOK:-books/turbo.epd}
baseline=.sprt/bin/$sha

if [ ! -x external/fastchess/fastchess ]; then
    echo "no fastchess: make -C external/fastchess -j" >&2
    exit 1
fi

if [ ! -f "$book" ]; then
    echo "no opening book: cargo run --release --bin bookgen -- $book 30000" >&2
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
    -openings file="$book" format=epd order=random \
    -rounds 100000 -games 2 -repeat \
    -concurrency "${CONCURRENCY:-$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 1)}" \
    -sprt elo0="${ELO0:-0}" elo1="${ELO1:-10}" alpha=0.05 beta=0.05 model=normalized \
    -pgnout file=.sprt/sprt.pgn \
    -config outname=.sprt/config.json \
    "$@"
