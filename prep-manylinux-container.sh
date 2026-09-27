#!/bin/bash

set -exo pipefail

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

# remoteprocess links liblzma for symbolication.
yum install -y xz-devel

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
# libunwind 1.6.2's aarch64 code passes ucontext_t* where unw_tdep_context_t* is expected.
# gcc 14 (in manylinux_2_28) makes that an error by default, so keep it a warning.
./configure CFLAGS="-fPIC -Wno-error=incompatible-pointer-types" --enable-static
make -j3
make install
