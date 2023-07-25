# region Buildter state
FROM rust:latest AS builder

# Docker go to `app` directory (equivalent to `cd app`)
# Docker will create one if it doesn't exist
WORKDIR /app
# Install system dependencies to build rust project
RUN apt update && apt install lld clang -y
# Copy all files from current project directory to Docker image directory
COPY . .
ENV SQLX_OFFLINE true
# Build project
RUN cargo build --release
# endregion Buildter state

# region Runtime state
FROM debian:bullseye-slim AS runtime

WORKDIR /app
# Install OpenSSL - dynamically linked library that binary depends on
# Install ca-certificates - TLS certificates for HTTPS
RUN apt update -y \
    && apt install -y --no-install-recommends openssl ca-certificates \
    # Clean up
    && apt autoremove -y \
    && apt clean -y \
    && rm -rf /var/lib/apt/lists/*
# Copy binary from builder to runtime
COPY --from=builder /app/target/release/zero2prod zero2prod
# Configuration files are needed at runtime
COPY configuration configuration
ENV APP_ENVIRONMENT production
ENTRYPOINT ["./zero2prod"]
# endregion Runtime state