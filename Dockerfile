FROM rust:latest

# Docker go to `app` directory (equivalent to `cd app`)
# Docker will create one if it doesn't exist
WORKDIR /app
# Install system dependencies to build rust project
RUN apt update && apt install lld clang -y
# Copy all files from current project directory to Docker image directory
COPY . .
ENV SQLX_OFFLINE true
ENV APP_ENVIRONMENT production
# Build project
RUN cargo build --release
# Run builded binary when `docker run` command is executed
CMD ["./target/release/zero2prod"]