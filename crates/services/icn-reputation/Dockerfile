FROM rust:1.77 as builder

WORKDIR /src
COPY . .

# Build the reputation service - note this builds just the icn-reputation package
RUN cargo build -p icn-reputation --release

# ── Runtime stage ───────────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update -qq && apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/* && \
    update-ca-certificates

WORKDIR /app

COPY --from=builder /src/target/release/icn-reputation /app/icn-reputation

# Create a data directory for the reputation service
RUN mkdir -p /data/reputation && chmod 777 /data/reputation

# Set environment variables
ENV RUST_LOG=info
ENV STORAGE_PATH=/data/reputation

# Expose the API port
EXPOSE 8080

# Run the reputation service
CMD ["/app/icn-reputation"] 