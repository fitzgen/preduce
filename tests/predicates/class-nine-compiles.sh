#!/usr/bin/env bash

set -eux

# Ensure that it still has `class Nine`.
grep 'class Nine ' "$1"

# Ensure that it still compiles OK.
clang++ -c "$1" -x c++ -std=c++11
