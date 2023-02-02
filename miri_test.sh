#!/usr/bin/env bash

set -ex

cargo clean
cargo +nightly miri test
