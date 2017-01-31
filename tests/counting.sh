#!/usr/bin/env bash

n="$PREDUCE_COUNTING_ITERATIONS"

for (( i = 0; i < $n; i++ )); do
    read -r ignored
    echo $i > "counting-$i"
    echo "counting-$i"
done
