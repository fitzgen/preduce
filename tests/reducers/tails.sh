#!/usr/bin/env bash

set -eu

# The initial seed test case is the first and only argument.
seed="$1"

# Count how many lines are in the test case.
n=$(wc -l "$seed" | cut -d ' ' -f 1)

# Generate a potential reduction of the seed's last line, then its last 2
# lines, then its last 3 lines, etc...
for (( i=1 ; i < n; i++ )); do
    # Read the '\n' from stdin and ignore it.
    read -r ignored

    # Generate the potential reduction in a new file.
    tail -n "$i" "$seed" > "tail-$i"

    # Tell `preduce` about the potential reduction.
    echo "tail-$i"
done
