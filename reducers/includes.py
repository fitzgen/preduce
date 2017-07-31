#!/usr/bin/env python

import os
import re
import sys

sys.path.append(os.path.dirname(os.path.realpath(__file__)))
from reducer_utils import regexp_matching_reducer

def main():
    regexp_matching_reducer(sys.argv[1], re.compile('^\s*#\s*include.*'))

if __name__ == "__main__":
    main()
