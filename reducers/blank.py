#!/usr/bin/env python

import os
import sys

seed = sys.argv[1]

n = 0
found_nth_blank = True

while found_nth_blank:
    # Read the test case path from stdin.
    out_file_path = sys.stdin.readline().strip()

    # Write a copy of the file without the n^th blank line.
    i = 0
    found_nth_blank = False
    with open(out_file_path, "w") as out_file:
        with open(seed, "r") as in_file:
            for line in in_file:
                if line.strip() == "":
                    if i == n:
                        found_nth_blank = True
                        continue
                    i += 1
                out_file.write(line)

    n += 1
    if not found_nth_blank:
        # No more blank lines to remove, time to exit.
        sys.exit(0)

    # Tell `preduce` we generated the reduction.
    sys.stdout.write("\n")
    sys.stdout.flush()
