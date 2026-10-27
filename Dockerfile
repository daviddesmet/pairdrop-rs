# Build against musl so the binary is fully static and the runtime image can be empty.
FROM rust:1-alpine AS build

RUN apk add --no-cache musl-dev

WORKDIR /build

# Dependencies first, so edits to src/ do not re-resolve the whole tree.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs && cargo build --release && rm -rf src

COPY src ./src
RUN touch src/main.rs && cargo build --release


# Nothing but the binary and the client: no shell, no package manager, no userland to patch.
# `--healthcheck` is the same binary making one request to the server beside it, which is why
# this image needs neither wget nor curl.
FROM scratch

WORKDIR /

COPY --from=build /build/target/release/pairdrop-rs /pairdrop-rs
COPY public /public

USER 1000:1000

EXPOSE 3000

HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
  CMD ["/pairdrop-rs", "--healthcheck"]

ENTRYPOINT ["/pairdrop-rs"]
