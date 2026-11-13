#!/bin/bash

if [[ $# -ne 1 || ! "$1" =~ ^[0-9]+$ ]]; then
    echo "Usage: $0 <number>"
    exit 1
fi

chapter=$(printf "%02d" "$1")

sed -i \
  -e "s/chapter[0-9]*\.rs/chapter${chapter}.rs/" \
  -e "s/default = \[\"chapter[0-9]*\"\]/default = [\"chapter${chapter}\"]/" \
  Cargo.toml

sed -i \
  -e "s|path = \"src/chapter[0-9]*\.rs\"|path = \"src/chapter${chapter}.rs\"|" \
  -e "s/\(default = \[.*\)chapter[0-9]*\(.*\]\)/\1chapter${chapter}\2/" \
  Cargo.toml
