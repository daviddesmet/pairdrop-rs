# Build the signaling server against musl so the runtime image needs nothing but the binary.
FROM rust:1-alpine AS build

RUN apk add --no-cache musl-dev

WORKDIR /build

# Dependencies first, so edits to src/ do not re-resolve the whole tree.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs && cargo build --release && rm -rf src

COPY src ./src
RUN touch src/main.rs && cargo build --release


FROM alpine:3.24

RUN apk add --no-cache wget \
    && adduser -D -u 1000 pairdrop

WORKDIR /app

COPY --from=build /build/target/release/pairdrop-rs /usr/local/bin/pairdrop-rs
COPY public ./public

USER pairdrop

EXPOSE 3000

HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
  CMD wget --quiet --tries=1 --spider http://localhost:3000/config || exit 1

ENTRYPOINT ["pairdrop-rs"]
