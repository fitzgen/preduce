#!/usr/bin/env python

import sys

if __name__ == "__main__":
    # Support globbing `$PREDUCE/reducers/*.py` to get a list of reducers.
    sys.exit(0)

import os
import subprocess
import tempfile

def num_lines_in_file(path):
    """Return the number of lines in the file at the given path."""
    num_lines = 0
    with open(path, "r") as f:
        for _ in f:
            num_lines += 1
    return num_lines

def get_executable(possibles):
    for p in possibles:
        if os.path.isfile(p) and os.access(p, os.X_OK):
            return p
    return None

def get_topformflat():
    """Try to find the `topformflat` program."""
    return get_executable([
        "/usr/local/libexec/topformflat",
        "/usr/libexec/topformflat",
        "/usr/lib/x86_64-linux-gnu/topformflat",
        "/usr/lib/creduce/topformflat",
        "/usr/local/Cellar/creduce/2.7.0/libexec/topformflat",
    ])

def get_clex():
    """Try to find the `clex` program."""
    return get_executable([
        "/usr/local/libexec/clex",
        "/usr/libexec/clex",
        "/usr/lib/x86_64-linux-gnu/clex",
        "/usr/lib/creduce/clex",
        "/usr/local/Cellar/creduce/2.7.0/libexec/clex",
    ])

def get_clang_delta():
    """Try to find the `clang_delta` program."""
    return get_executable([
        "/usr/local/libexec/clang_delta",
        "/usr/libexec/clang_delta",
        "/usr/lib/x86_64-linux-gnu/clang_delta",
        "/usr/lib/creduce/clang_delta",
        "/usr/local/Cellar/creduce/2.7.0/libexec/clang_delta",
    ])

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
            if out_file_path == "":
                return

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

    with tempfile.NamedTemporaryFile(mode="w+") as tmp_file:
        with open(seed, "r") as in_file:
            subprocess.check_call([topformflat, str(flatten)], stdin=in_file, stdout=tmp_file)
        chunking_reducer(tmp_file.name)

def clex_reducer(seed, clex_command):
    clex = get_clex()
    if clex is None:
        return

    index = 0
    while True:
        # Read the file path from stdin.
        out_file_path = sys.stdin.readline().strip()
        if out_file_path == "":
            return

        retcode = 0
        with open(out_file_path, "w") as out_file:
            retcode = subprocess.call([clex, clex_command, str(index), seed],
                                      stdout=out_file)

        # I don't know why clex is written with these bizarre exit codes...
        if retcode != 51:
            return;

        index += 1

        # Tell `preduce` we generated the reduction.
        sys.stdout.write("\n")
        sys.stdout.flush()

def clang_delta_reducer(seed, transformation):
    clang_delta = get_clang_delta()
    if clang_delta is None:
        return

    index = 1
    while True:
        # Read the file path from stdin.
        out_file_path = sys.stdin.readline().strip()
        if out_file_path == "":
            return

        retcode = None
        with open(out_file_path, "w") as out_file:
            retcode = subprocess.call([clang_delta,
                                       "--transformation={}".format(transformation),
                                       "--counter={}".format(str(index)),
                                       seed],
                                      stdout=out_file)

        if retcode == 0:
            # Tell `preduce` we generated the reduction.
            sys.stdout.write("\n")
            sys.stdout.flush()

            index += 1
            continue
        elif retcode == 255 or retcode == 1:
            return
        else:
            # Ideally, we would do this:
            #
            #     raise Exception("Unknown return code from clang_delta: {}".format(str(retcode)))
            #
            # Except that `clang_delta` SIGSEGVs frequently enough that the
            # backtraces from raising this exception would completely drown out
            # any useful information we are otherwise logging.
            return

def regexp_matching_reducer(seed, regexp):
    n = 0
    found_nth_match = True

    while found_nth_match:
        # Read the test case path from stdin.
        out_file_path = sys.stdin.readline().strip()
        if out_file_path == "":
            return

        # Write a copy of the file without the n^th matching line.
        i = -1
        found_nth_match = False
        with open(out_file_path, "w") as out_file:
            with open(seed, "r") as in_file:
                for line in in_file:
                    if regexp.match(line.strip()):
                        i += 1
                        if i == n:
                            found_nth_match = True
                            continue
                    out_file.write(line)

        n += 1
        if not found_nth_match:
            # No more matching lines to remove, time to return.
            return

        # Tell `preduce` we generated the reduction.
        sys.stdout.write("\n")
        sys.stdout.flush()

class BalancedBracketFinder(object):
    """Given source text and a bracket type tuple describing the opening
    bracket character and closing bracket character, finds all pairs of
    indices in the source text where, starting at the first index in the
    pair and proceeding to the second index in the pair, the contents of
    the source text is balanced with respect to the given bracket
    type. As implemented this is essentially an O(n^2) algorithm. If
    this ever becomes a problem, this could be reimplemented with a
    stack to be O(n).
    """
    angle = ("<", ">")
    curly = ("{", "}")
    paren = ("(", ")")
    square = ("[", "]")

    def _find_next_pair_from(self, start_index):
        """Given a start index (where there may or may not be an opening
        bracket), finds and returns the index of next opening bracket
        and its corresponding balanced closing bracket index, if such a
        pair exists (returns None otherwise).
        """
        depth = 0
        opening_index = None
        closing_index = None
        for index in xrange(start_index, len(self._source)):
            if self._source[index] == self._opening_bracket:
                if not opening_index:
                    opening_index = index
                depth = depth + 1
            elif depth > 0 and self._source[index] == self._closing_bracket:
                depth = depth - 1
                if depth == 0:
                    closing_index = index
                    return (opening_index, closing_index)
        return None

    def __init__(self, source, bracket_tuple):
        self._source = source
        self._index = 0
        (self._opening_bracket, self._closing_bracket) = bracket_tuple

    def find_next_pair(self):
        """Finds the next (opening bracket index, closing bracket index)
        pair. Returns None if no more exist.
        """
        if self._index >= len(self._source):
            return None
        next_pair = self._find_next_pair_from(self._index)
        if not next_pair:
            self._index = len(self._source)
            return None
        self._index = next_pair[0] + 1
        return next_pair


def balanced_reducer(seed, bracket_type):
    with open(seed, "r") as in_file:
        contents = in_file.read()
    bracket_finder = BalancedBracketFinder(contents, bracket_type)
    indices = bracket_finder.find_next_pair()
    while indices:
        out_file_path = sys.stdin.readline().strip()
        if out_file_path == "":
            return
        with open(out_file_path, "w") as out_file:
            # Generate a reduction by removing the entire range
            # (brackets included)
            out_file.write(contents[0:indices[0]] + contents[indices[1] + 1:])
        # Tell `preduce` we generated the reduction.
        sys.stdout.write("\n")
        sys.stdout.flush()

        # Generate a reduction by removing the contents of the range
        # (brackets not included). This doesn't make much sense if the
        # range is (n, n + 1), so filter out that case.
        if indices[0] + 1 < indices[1]:
            out_file_path = sys.stdin.readline().strip()
            if out_file_path == "":
                return
            with open(out_file_path, "w") as out_file:
                out_file.write(contents[0:indices[0] + 1] + contents[indices[1]:])
            # Tell `preduce` we generated the reduction.
            sys.stdout.write("\n")
            sys.stdout.flush()
        indices = bracket_finder.find_next_pair()
