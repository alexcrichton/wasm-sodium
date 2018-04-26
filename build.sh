set -ex

# Build our docker container we'll compile wasm in.
#
# The primary need for this is a version of `clang` that supports wasm, see
# comments in the `Dockerfile` for more info
docker build \
  --rm \
  --tag wasm-sodium-test \
  .

# Use our docker container to compile to wasm. This is intended to share files
# with the host so the output will show up in this directory
mkdir -p target
docker run \
  --user $(id -u):$(id -g) \
  --volume `pwd`:/c:ro \
  --volume `pwd`/target:/c/target \
  --volume $HOME/.cargo:/cargo \
  --workdir /c \
  --volume `rustc +nightly --print sysroot`:/rust:ro \
  --interactive \
  --tty \
  --rm \
  wasm-sodium-test \
  cargo build --target wasm32-unknown-unknown --release

# Now that we've finished compiling, execute `wasm-bindgen` and then run it
# through `node.js` to get some examples.
wasm-bindgen --nodejs --out-dir . \
  target/wasm32-unknown-unknown/release/wasm_sodium.wasm

node run.js
