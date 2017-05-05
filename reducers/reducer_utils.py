#!/usr/bin/env python

import os
import subprocess
import sys
import tempfile

if __name__ == "__main__":
    sys.exit(0)

def num_lines_in_file(path):
    """Return the number of lines in the file at the given path."""
    num_lines = 0
    with open(path, "r") as f:
        for _ in f:
            num_lines += 1
    return num_lines

def get_topformflat():
    """Try to find the `topformflat` program."""
    possibles = [
        "/usr/local/libexec/topformflat",
        "/usr/libexec/topformflat",
    ]
    for p in possibles:
        if os.path.isfile(p):
            return p
    return None

def chunk_sizes(min_chunk_size, max_chunk_size):
    """Generate chunk sizes from min_chunk_size to max_chunk_size."""
    chunk_size = max_chunk_size
    while chunk_size >= min_chunk_size:
        yield chunk_size
        chunk_size = chunk_size // 2

def copy_without_lines(from_path, to_path, start_skip_line, num_skip_lines):
    """Copy the file at `from_path` to `to_path`, without the lines in the range
    [start_skip_line, start_skip_line + num_skip_lines)

    """
    end_skip_lines = start_skip_line + num_skip_lines
    with open(to_path, "w") as out_file:
        with open(from_path, "r") as in_file:
            for j, line in enumerate(in_file):
                if start_skip_line <= j < end_skip_lines:
                    continue
                else:
                    out_file.write(line)

def chunking_reducer(seed, min_chunk_size = 1, max_chunk_size = None):
    """Implements a reducer that removes chunks of lines from the given seed file.

    """
    num_lines = num_lines_in_file(seed)

    if max_chunk_size is None:
        max_chunk_size = num_lines

    for chunk_size in chunk_sizes(min_chunk_size, max_chunk_size):
        for i in range(0, num_lines - (chunk_size - 1)):
            # Read the file path from stdin.
            out_file_path = sys.stdin.readline().strip()

            # Copy the file without the current chunk.
            copy_without_lines(seed, out_file_path, i, chunk_size)

            # Tell `preduce` we generated the reduction.
            sys.stdout.write("\n")
            sys.stdout.flush()

def topformflat_reducer(seed, flatten):
    """Run `topformflat` on the seed file, and then create a chunking reducer from
    it.

    """
    topformflat = get_topformflat()
    if topformflat is None:
        return

    with tempfile.NamedTemporaryFile(mode="w+", delete=False) as tmp_file:
        with open(seed, "r") as in_file:
            subprocess.check_call([topformflat, str(flatten)], stdin=in_file, stdout=tmp_file)

    chunking_reducer(tmp_file.name)