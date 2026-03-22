FROM rust:1-slim-trixie as builder

WORKDIR /build
RUN cargo install cargo-auditable
COPY Cargo.lock Cargo.toml ./
COPY src/ src/
RUN cargo auditable build --release

# Runtime image
FROM gcr.io/distroless/cc-debian13:nonroot

COPY --from=builder /build/target/release/ai-operator /bin/

WORKDIR /app
EXPOSE 3000/tcp
CMD ["/bin/ai-operator"]
