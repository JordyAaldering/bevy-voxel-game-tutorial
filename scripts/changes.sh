#!/bin/bash

if [ "$#" -eq 1 ]; then
    to=$1
    from=$((to - 1))
elif [ "$#" -eq 2 ]; then
    from=$1
    to=$2
else
    echo "Usage: $0 <chapter> [chapter]"
    echo "Shows the changes introduced by <chapter>, or those between <chapter> and [chapter]."
    exit 1
fi

git diff --no-index \
    "src/chapter$(printf '%02d' "$from").rs" \
    "src/chapter$(printf '%02d' "$to").rs"
