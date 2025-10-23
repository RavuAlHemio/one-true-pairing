#!/bin/bash

# bail out at first sign of trouble
set -e -o pipefail

# build the protocol generator
cargo build --bin wlproto

# run it for each protocol we are interested in
build_proto() {
    src="$1"
    dst="$2"

    ./target/debug/wlproto \
        --async \
        "$src" \
        | \
        rustfmt --edition=2018 \
        > "$dst"
}

build_proto "ext/wayland/protocol/wayland.xml" "one-true-pairing/src/wayland/protocol/wayland.rs"
