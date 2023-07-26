# region Set up Rust dependencies with cargo-chef
FROM lukemathwalker/cargo-chef:latest AS chef
WORKDIR /app
# Install system dependencies to build rust project
RUN apt update -y && apt install lld clang -y

FROM chef AS planner
COPY . .
# Prepare dependencies recipe and cache for later docker build
RUN cargo chef prepare --recipe-path recipe.json
# region Set up Rust dependencies with cargo-chef

# region Builder state
FROM chef AS builder
# Copy cached recipe.json, if recipe.json already exists and doesn't change, then skip building (cache hit)
COPY --from=planner /app/recipe.json recipe.json
# Build project dependencies, not the application
RUN cargo chef cook --release --recipe-path recipe.json
# If dependencies trees stay the same, all layers should be cached and COPY just reuse previous layers
COPY . .
ENV SQLX_OFFLINE true
# Build project
RUN cargo build --release --bin zero2prod
# endregion Builder state

# region Runtime state
FROM debian:bullseye-slim AS runtime

WORKDIR /app
# Install OpenSSL - dynamically linked library that binary depends on
# Install ca-certificates - TLS certificates for HTTPS
RUN apt update -y \
    && apt install -y --no-install-recommends openssl ca-certificates \
    && apt autoremove -y \
    && apt clean -y \
    && rm -rf /var/lib/apt/lists/*
# Copy binary from builder to runtime
COPY --from=builder /app/target/release/zero2prod zero2prod
# Configuration files are needed at runtime
COPY configuration configuration
ENV APP_ENV_STATE production
ENTRYPOINT ["./zero2prod"]
# endregion Runtime state