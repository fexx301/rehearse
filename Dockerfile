# Clean-machine reproduction of the Rehearse demo.
#
#   docker build -t rehearse .
#   docker run --rm --network none rehearse
#
# The build stage fetches crates (needs network). The run stage replays the
# committed capture with networking disabled and checks that report.json and
# report.html come out byte-identical to the committed copies.
FROM rust:1.95-slim-bookworm

WORKDIR /rehearse
# Dependencies first so they cache separately from source edits.
COPY cli/Cargo.toml cli/Cargo.lock cli/
RUN mkdir -p cli/src && echo 'fn main() {}' > cli/src/main.rs \
 && cargo build --release --locked --manifest-path cli/Cargo.toml \
 && rm -rf cli/src cli/target/release/rehearse cli/target/release/deps/rehearse-*

COPY cli/src cli/src
RUN cargo build --release --locked --manifest-path cli/Cargo.toml

COPY demo/manifest.json demo/report.json demo/report.html demo/
COPY demo/capture demo/capture
COPY demo/wasm demo/wasm
COPY examples examples
COPY reproduce.sh .

ENV OFFLINE=1
CMD ["./reproduce.sh"]
