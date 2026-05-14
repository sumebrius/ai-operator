FROM rust:1-alpine as builder

WORKDIR /build
RUN apk add --no-cache musl-dev
RUN cargo install cargo-auditable
ENV RUSTFLAGS="-C target-feature=+crt-static"
COPY Cargo.lock Cargo.toml ./
COPY src/ src/
RUN cargo auditable build --release --target x86_64-unknown-linux-musl

# Runtime image
FROM scratch

COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/ai-operator /bin/
WORKDIR /app
EXPOSE 3000/tcp
CMD ["/bin/ai-operator"]
