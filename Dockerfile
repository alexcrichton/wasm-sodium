FROM ubuntu:16.04

RUN apt-get update -y
RUN apt-get install -y \
  g++ \
  make \
  cmake \
  curl \
  xz-utils \
  python

WORKDIR /llvm/build
RUN curl http://releases.llvm.org/6.0.0/llvm-6.0.0.src.tar.xz | \
  tar xJf - -C /llvm --strip-components 1
RUN mkdir /llvm/tools/clang
RUN curl http://releases.llvm.org/6.0.0/cfe-6.0.0.src.tar.xz | \
  tar xJf - -C /llvm/tools/clang --strip-components 1
RUN cmake .. \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_INSTALL_PREFIX=/clang \
  -DLLVM_TARGETS_TO_BUILD=X86 \
  -DLLVM_EXPERIMENTAL_TARGETS_TO_BUILD=WebAssembly
RUN make -j $(nproc)
RUN make install

# Install Rust as we'll use it later. We'll also be cribbing `lld` out of Rust's
# sysroot to use when compiling libsodium
ENV CARGO_HOME /cargo
ENV RUSTUP_HOME /rustup
RUN curl https://sh.rustup.rs | sh -s -- -y --default-toolchain nightly
ENV PATH $PATH:/cargo/bin:/rustup/toolchains/nightly-x86_64-unknown-linux-gnu/lib/rustlib/x86_64-unknown-linux-gnu/bin
RUN rustup target add wasm32-unknown-unknown

ENV CC /clang/bin/clang

WORKDIR /
RUN apt-get install -y git
RUN git clone https://github.com/jfbastien/musl
WORKDIR /musl
RUN git reset --hard d312ecae
ENV CFLAGS -O3 --target=wasm32-unknown-unknown-wasm -nostdlib -Wl,--no-entry
RUN CFLAGS="$CFLAGS -Wno-error=pointer-sign" ./configure --prefix=/musl-sysroot wasm32
RUN make -j$(nproc) install

WORKDIR /
RUN curl https://download.libsodium.org/libsodium/releases/libsodium-1.0.16.tar.gz | tar xzf -
WORKDIR /libsodium-1.0.16
RUN CFLAGS="$CFLAGS --sysroot=/musl-sysroot -DSODIUM_STATIC"\
  ./configure \
  --host=asmjs \
  --prefix=/musl-sysroot \
  --without-pthreads \
  --enable-minimal \
  --disable-asm \
  --disable-ssp
RUN make -j$(nproc) install
ENV SODIUM_LIB_DIR /musl-sysroot/lib
ENV SODIUM_STATIC 1
RUN rustup self uninstall -y
ENV PATH /rust/bin:$PATH
