#!/bin/bash -e
cargo build-sbf
(cd ../sdk && yarn && yarn solita)
