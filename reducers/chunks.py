#!/usr/bin/env python

import os
import sys

# The initial seed test case is the first and only argument.
seed = sys.argv[1]

n = 0
with open(seed, "r") as f:
    for line in f:
        n += 1

def chunk_sizes():
    # The initial chunk_size will be how many lines are in the seed test case.
    chunk_size = n

    # Then we'll just keep halving it. Skip chunk_size=1, as that is just the
    # lines.py reducer.
    while chunk_size > 1:
        yield chunk_size
        chunk_size = chunk_size // 2

for chunk_size in chunk_sizes():
    for i in range(0, n - (chunk_size - 1)):
        # Read the file path from stdin.
        out_file_path = sys.stdin.readline().strip()

        with open(out_file_path, "w") as out_file:
            with open(seed, "r") as in_file:
                for j, line in enumerate(in_file):
                    if i <= j < (i + chunk_size):
                        # Skip lines `i` through `i + chunk_size` from the in
                        # file.
                        continue
                    else:
                        # Copy the rest of the lines to the out file.
                        out_file.write(line)

        # Tell `preduce` we generated the reduction.
        sys.stdout.write("\n")
        sys.stdout.flush()
