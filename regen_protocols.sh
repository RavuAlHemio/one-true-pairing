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
	--in-crate \
        "$src" \
        | \
        rustfmt --edition=2018 \
        > "$dst"
}

build_proto "ext/wayland/protocol/wayland.xml" "whale-land/src/protocol/wayland.rs"
build_proto "ext/wayland-protocols/staging/ext-data-control/ext-data-control-v1.xml" "whale-land/src/protocol/ext_data_control_v1.rs"
