#!/usr/bin/env python

import subprocess
import sys

def main():
    clang_format = None
    try:
        clang_format = subprocess.check_output(["which", "clang-format"]).strip()
    except:
        return

    seed = sys.argv[1]

    # Read the out file path from stdin.
    out_file_path = sys.stdin.readline().strip()

    with open(out_file_path, "w") as out_file:
        try:
            subprocess.check_call([clang_format,
                                   "-style",
                                   "{SpacesInAngles: true, IndentWidth: 0}",
                                   seed], stdout=out_file)
        except:
            return

    # Tell `preduce` we generated the reduction.
    sys.stdout.write("\n")
    sys.stdout.flush()

if __name__ == "__main__":
    main()
