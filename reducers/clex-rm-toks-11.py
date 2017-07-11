#!/usr/bin/env python

import os
import sys

sys.path.append(os.path.dirname(os.path.realpath(__file__)))
from reducer_utils import clex_reducer

def main():
    clex_reducer(sys.argv[1], "rm-toks-11")

if __name__ == "__main__":
    main()
