#!/usr/bin/env bash
# Единая проверка проекта.
#
# Структурные правила (<=250 строк на файл, <=4 файла в директории) гоняются
# гейтом в build.rs каждого крейта, поэтому шаг 1 ловит их и без этого скрипта.
set -euo pipefail

cd "$(dirname "$0")/.."

echo "==> 1/4 structure rules + build.rs gate"
cargo check --workspace --all-targets

echo "==> 2/4 rustfmt"
cargo fmt --all -- --check

echo "==> 3/4 clippy (warnings are errors)"
cargo clippy --workspace --all-targets -- -D warnings

echo "==> 4/4 tests"
cargo test --workspace

echo "OK"
