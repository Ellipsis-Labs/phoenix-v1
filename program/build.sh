#!/bin/bash -e
ROOT=$(git rev-parse --show-toplevel)
cargo build-sbf
(cd $ROOT/sdk && yarn && node generateClient.js)
