#!/usr/bin/env python

import os
import sys

sys.path.append(os.path.dirname(os.path.realpath(__file__)))
from reducer_utils import topformflat_reducer

def main():
    topformflat_reducer(sys.argv[1], 8)

if __name__ == "__main__":
    main()
