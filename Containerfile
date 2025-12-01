FROM rust:1-slim-trixie as builder

WORKDIR /usr/src/ai-operator
COPY Cargo.lock Cargo.toml ./
COPY src/ src/
RUN cargo install --path .

# Runtime image
FROM debian:trixie-slim

COPY --from=builder /usr/local/cargo/bin/ai-operator /usr/local/bin/ai-operator
COPY prompt.txt .

WORKDIR /app
EXPOSE 3000/tcp
CMD ["ai-operator"]
