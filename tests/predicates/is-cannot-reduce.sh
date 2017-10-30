#!/usr/bin/env bash

set -xeu

LOREM="$(dirname $0)/../fixtures/cannot-reduce.txt"

diff "$LOREM" "$1"
