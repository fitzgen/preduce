#!/usr/bin/env bash

n="$PREDUCE_COUNTING_ITERATIONS"

for (( i = 0; i < $n; i++ )); do
    read -r path
    echo $i > "$path"
    echo
done
