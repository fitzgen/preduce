#!/usr/bin/env bash

function gen {
    read -r ignored
    echo $1 > "alphabet-$1"
    echo "alphabet-$1"
}

gen "a"
gen "b"
gen "c"
gen "d"
gen "e"
gen "f"
gen "g"
gen "h"
gen "i"
gen "j"
gen "k"
gen "l"
gen "m"
gen "n"
gen "o"
gen "p"
gen "q"
gen "r"
gen "s"
gen "t"
gen "u"
gen "v"
gen "w"
gen "x"
gen "y"
gen "z"
