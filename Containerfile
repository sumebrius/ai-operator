FROM rust:1-alpine AS builder

WORKDIR /build
RUN apk add --no-cache musl-dev upx
ENV RUSTFLAGS="-C target-feature=+crt-static"
COPY Cargo.lock Cargo.toml ./
COPY src/ src/
RUN cargo build --locked --release --target x86_64-unknown-linux-musl
RUN upx --best --lzma /build/target/x86_64-unknown-linux-musl/release/ai-operator

# Runtime image
FROM scratch

COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/ai-operator /bin/
WORKDIR /app
EXPOSE 3000/tcp
CMD ["/bin/ai-operator"]
