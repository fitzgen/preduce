#!/usr/bin/env bash

set -eux

cd $(dirname $0)

# Set for CI in the .travis.yml config. Default to empty strings, aka a debug
# build with no features.
: ${PROFILE:=""}
: ${FEATURES:=""}

function test_reduction {
    cargo run $PROFILE --features "$FEATURES" -- \
          "fixtures/$1" "$2" ../reducers/*

    # Diff exits 0 if they're the same, non-zero if there is any diff.
    diff -U8 "fixtures/$1" "expectations/$1"

    # Replace the unreduced fixture file.
    mv "fixtures/$1.orig" "fixtures/$1"
}

test_reduction lorem-ipsum.txt ./predicates/has-lorem.sh

echo "OK! All tests passed!"
