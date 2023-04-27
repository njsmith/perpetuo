#!/bin/bash

set -exo pipefail

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

# We could get libunwind by doing
#
#   yum install -y libunwind-devel
#
# but that failed b/c the static library wasn't built with -fPIC, and that's required
# because our final binary is a PIE, so everything in it has to be PIC. So build our own
# libunwind that we can force to use -fPIC.
cd /
curl -L https://github.com/libunwind/libunwind/archive/refs/tags/v1.6.2.tar.gz  -o libunwind.tar.gz
tar xvf libunwind.tar.gz
cd libunwind-*/
autoreconf -i
./configure CFLAGS="-fPIC" --enable-static
make -j3
make install
