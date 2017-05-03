#!/usr/bin/env bash

set -eux

case "$JOB" in
    "build")
        cargo build $PROFILE --verbose --features "$FEATURES"
        ;;
    "test")
        cargo test $PROFILE --verbose --features "$FEATURES"
        ./tests/tests.sh
        ;;
    "bench")
        if [[ "$PROFILE" != "--release" ]]; then
            echo Benching a non-release build??
            exit 1
        fi
        cargo bench --verbose --features "$FEATURES"
        ;;
    *)
        echo Unknown job: "$JOB"
        exit 1
        ;;
esac
