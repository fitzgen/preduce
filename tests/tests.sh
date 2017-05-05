#!/usr/bin/env bash

set -eux

cd $(dirname $0)

# Set for CI in the .travis.yml config. Default to empty strings, aka a debug
# build with no features.
: ${PROFILE:=""}
: ${FEATURES:=""}

# Travis CI is a bit over-eager about cleaning up the tempdir.
if [[ "${CI:=''}" == "true" ]]; then
    export TMPDIR=$(pwd)
fi

# Do a full preduce reduction run.
#
# Usage: test_preduce_run <fixture-name> <predicate>
function test_preduce_run {
    fixture=$1
    predicate=$2

    cargo run $PROFILE --features "$FEATURES" -- \
          "fixtures/$fixture" "$predicate" ../reducers/*.py

    # Ensure that the reduced file is still interesting.
    "$predicate" "fixtures/$fixture"

    # Diff exits 0 if they're the same, non-zero if there is any diff.
    diff -U8 "expectations/$fixture" "fixtures/$fixture"

    # Replace the unreduced fixture file.
    mv "fixtures/$fixture.orig" "fixtures/$fixture"
}

# Test a reducer's generated reductions.
#
# Usage: test_reducer <reducer> <fixture-name> <expected-0> <expected-1> ...
function test_reducer {
    reducer=$1
    seed=$2

    # Make a couple named pipes for the reducer's stdin and stdout.
    child_stdin=$(mktemp -u)
    mkfifo "$child_stdin"
    child_stdout=$(mktemp -u)
    mkfifo "$child_stdout"

    # Ensure that the stdin pipe doesn't get closed after the first `echo` into
    # it.
    (sleep 999999999999 > "$child_stdin")&
    sleep_pid=$!
    (sleep 999999999999 > "$child_stdout")&
    sleep_pid2=$!

    # Spawn the reducer in the background with its stdin and stdout connected to
    # named pipes.
    ("$reducer" "$seed" < "$child_stdin" > "$child_stdout") &
    pid=$!

    shift 2
    for expected in $@; do
        # Tell it to generate the reduction in a temp file.
        tmp=$(mktemp)
        echo "$tmp" > "$child_stdin"

        # Wait for it to finish generating its reduction.
        read empty < "$child_stdout"
        if [[ "$empty" != "" ]]; then
            echo "Reducer should have written a '\n', got: '$empty'"
            exit 1
        fi

        # There shouldn't be any diff with the expected file.
        diff -U8 "$expected" "$tmp"
    done

    # Clean up the children.
    kill "$pid" "$sleep_pid" "$sleep_pid2"
}

test_preduce_run lorem-ipsum.txt ./predicates/has-lorem.sh
test_preduce_run nested-classes.cpp ./predicates/class-nine-compiles.sh

test_reducer ../reducers/chunks.py fixtures/lorem-ipsum.txt expectations/chunks-*
test_reducer ../reducers/lines.py fixtures/lorem-ipsum.txt expectations/lines-*

echo "OK! All tests passed!"
