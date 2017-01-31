#!/usr/bin/env bash

for (( i = 0; i < 5; i++ )); do
    read -r ignored
    echo $i > "counting-$i"
    echo "counting-$i"
done
