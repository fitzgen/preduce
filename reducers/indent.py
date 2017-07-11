#!/usr/bin/env python

import os
import random
import sys

# The initial seed test case is the first and only argument.
seed = sys.argv[1]

def indent_of_line(line):
    indent = 0
    for ch in line:
        if ch != " ":
            break
        indent += 1
    return indent

# Find the maximum indent in the file.
max_indent = 0
with open(seed, "r") as f:
    for line in f:
        this_indent = indent_of_line(line)
        if this_indent > max_indent:
            max_indent = this_indent

for indent_level in range(2, max_indent, 2):
    # Read the file path from stdin.
    out_file_path = sys.stdin.readline().strip()
    if out_file_path == "":
        sys.exit(0)

    # Generate the potential reduction without any line that is indented more
    # than the current indent level.
    with open(out_file_path, "w") as out_file:
        with open(seed, "r") as in_file:
            for line in in_file:
                if indent_of_line(line) < indent_level:
                    out_file.write(line)

    # Tell `preduce` we generated the reduction.
    sys.stdout.write("\n")
    sys.stdout.flush()
