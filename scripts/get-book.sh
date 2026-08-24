#!/bin/sh
# Fetch a 8move opening book generated from Stockfish
set -eu

url=https://github.com/official-stockfish/books/raw/master/8moves_v3.pgn.zip
root=$(dirname "$0")/..
archive=$(mktemp)
trap 'rm -f "$archive"' EXIT

curl -sSL -o "$archive" "$url"
unzip -o "$archive" -d "$root/books"
