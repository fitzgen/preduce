#!/usr/bin/env python

import os
import sys

sys.path.append(os.path.dirname(os.path.realpath(__file__)))
from reducer_utils import clang_delta_reducer

def main():
    clang_delta_reducer(sys.argv[1], "rename-class")

if __name__ == "__main__":
    main()
