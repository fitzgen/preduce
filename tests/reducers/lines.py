#!/usr/bin/env python3

import os
import random
import sys

# The initial seed test case is the first and only argument.
seed = sys.argv[1]

# Count how many lines are in the seed test case.
n = 0
with open(seed, "r") as f:
    for line in f:
        n += 1

for i in range(0, n):
    # Read the '\n' from stdin and ignore it.
    sys.stdin.readline()

    # Generate the potential reduction without the seed's i^th line in a new
    # file.

    out_file_path = "lines-{}-{}".format(str(i), str(random.random()))

    try:
        os.remove(out_file_path)
    except FileNotFoundError:
        pass

    with open(out_file_path, "w") as out_file:
        with open(seed, "r") as in_file:
            for j, line in enumerate(in_file):
                if i != j:
                    out_file.write(line)

    # Tell `preduce` about the potential reduction.
    sys.stdout.write(out_file_path)
    sys.stdout.write("\n")
    sys.stdout.flush()
