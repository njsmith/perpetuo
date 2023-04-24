#!/bin/bash

set -exo pipefail

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

cd /
git clone https://github.com/libunwind/libunwind
cd libunwind
autoreconf -i
./configure CFLAGS="-fPIC"
make -j3
make install
