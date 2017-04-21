#!/usr/bin/env bash

set -eu

# The initial seed test case is the first and only argument.
seed="$1"

# Count how many lines are in the seed test case.
n=$(wc -l "$seed" | cut -d ' ' -f 1)

for (( i=0 ; i < n; i++ )); do
    # Read the '\n' from stdin and ignore it.
    read -r ignored

    # Generate the potential reduction without line $i in a new file.
    rm -f "lines-$i"
    touch "lines-$i"
    if (( $i
    head -n "$((i))" "$seed"         >> "lines-$i"
    tail -n "$((n - i - 1))" "$seed" >> "lines-$i"

    # Tell `preduce` about the potential reduction.
    echo "lines-$i"
done
