#!/bin/sh

case "$TARGETPLATFORM" in
"linux/arm64")
    if [ "$BUILDPLATFORM" != "$TARGETPLATFORM" ]; then
        rustup target add aarch64-unknown-linux-gnu
        apt-get update
        apt-get install -y  gcc-aarch64-linux-gnu
    fi

    echo "aarch64-unknown-linux-gnu" > "/.rust-target.temp"
;;
"linux/amd64")
    if [ "$BUILDPLATFORM" != "$TARGETPLATFORM" ]; then
        rustup target add x86_64-unknown-linux-gnu
        apt-get update
        apt-get install -y  gcc-x86-64-linux-gnu
    fi

    echo "x86_64-unknown-linux-gnu" > "/.rust-target.temp" 
;;
*)
    echo "unknown" > "/.rust-target.temp"
;;
esac
