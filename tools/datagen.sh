#!/bin/sh
# Generate one shard of training data.
#
#     tools/datagen.sh data/gen3-a.data 100000000
#
# Two shards that share a seed play the same games, so each draws its own from
# /dev/urandom and writes it to <output>.meta with the revision that generated
# it. Pass SEED to reproduce a shard, NODES for a budget other than the
# default, THREADS for fewer than all of them.
#
# Records are appended a game at a time and a killed run keeps every one it
# wrote, which is what makes this safe on a spot instance. The shard is cut
# back to whole records on the way out, so the shards of a generation merge by
#
#     cat data/gen3-*.data > data/gen3.data
set -eu

if [ $# -lt 2 ]; then
    echo "usage: tools/datagen.sh <output> <positions>" >&2
    exit 2
fi

output=$1
positions=$2

cd "$(dirname "$0")/.."

# One bulletformat record. A run cut off partway through one leaves a tail
# that no loader can read.
record=32

# The datagen default, repeated here so the shard can record what made it.
nodes=${NODES:-2000}
threads=${THREADS:-$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 1)}
seed=${SEED:-$(od -An -N8 -tu8 -v /dev/urandom | tr -d ' ')}

cargo build --release --bin datagen

revision=$(git rev-parse --short HEAD)
if ! git diff --quiet HEAD; then
    revision=$revision-dirty
fi
started=$(date -u +%Y-%m-%dT%H:%M:%SZ)

./target/release/datagen "$output" "$positions" "$threads" "$nodes" "$seed" &
worker=$!

# A spot instance gives two minutes' notice as a TERM. Take the notice, stop
# the worker, and still leave a whole shard behind.
trap 'kill "$worker" 2>/dev/null || true' HUP INT TERM

# A signal answers the first wait, so keep waiting until the worker has
# really gone and nothing is writing to the file any more.
while kill -0 "$worker" 2>/dev/null; do
    wait "$worker" 2>/dev/null || true
done
trap - HUP INT TERM

size=$(wc -c < "$output")
whole=$(( size / record * record ))
if [ "$whole" -ne "$size" ]; then
    echo "cutting $(( size - whole )) bytes of a partial record" >&2
    truncate -s "$whole" "$output"
fi

cat > "$output.meta" <<META
revision  $revision
nodes     $nodes
threads   $threads
seed      $seed
started   $started
positions $(( whole / record ))
META

echo "$(( whole / record )) positions in $output" >&2
