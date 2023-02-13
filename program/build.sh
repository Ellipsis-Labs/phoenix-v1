#!/bin/bash -e
ROOT=$(git rev-parse --show-toplevel)
cargo build-sbf
(cd $ROOT/idl && yarn && node generateIdl.js)
