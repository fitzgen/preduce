#!/usr/bin/env python

import os
import re
import sys

sys.path.append(os.path.dirname(os.path.realpath(__file__)))
from reducer_utils import BalancedBracketFinder
from reducer_utils import balanced_reducer

def main():
    balanced_reducer(sys.argv[1], BalancedBracketFinder.angle)

if __name__ == "__main__":
    main()
