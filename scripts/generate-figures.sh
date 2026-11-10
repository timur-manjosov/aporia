#!/usr/bin/env bash
# Regenerates the committed convergence plots in docs/img/ with the default seed.
set -euo pipefail

cd "$(dirname "$0")/.."
cargo build --release --quiet
bin=target/release/aporia
mkdir -p docs/img

"$bin" monty-hall --svg docs/img/monty-hall.svg >/dev/null
"$bin" monty-hall --host ignorant --svg docs/img/monty-hall-ignorant.svg >/dev/null
"$bin" birthday --svg docs/img/birthday.svg >/dev/null
"$bin" bertrand --svg docs/img/bertrand.svg >/dev/null
"$bin" penney --svg docs/img/penney.svg >/dev/null
"$bin" parrondo --svg docs/img/parrondo.svg >/dev/null

echo "Figures written to docs/img/."
