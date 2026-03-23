FROM rust:1-alpine as builder

ENV RUSTFLAGS="-C target-feature=+crt-static"
ENV CARGO_BUILD_TARGET="x86_64-unknown-linux-musl"
WORKDIR /build
RUN apk add --no-cache musl-dev
RUN cargo install cargo-auditable
COPY Cargo.lock Cargo.toml ./
COPY src/ src/
RUN cargo auditable build --release

# Runtime image
FROM gcr.io/distroless/static-debian13:nonroot

COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/ai-operator /bin/

WORKDIR /app
EXPOSE 3000/tcp
CMD ["/bin/ai-operator"]
