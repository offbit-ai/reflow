FROM rust:latest as builder

# Install required system dependencies
RUN apt-get update && apt-get install -y \
    python3 \
    python3-dev \
    python3-pip \
    python3-venv \
    pkg-config \
    libssl-dev \
    libpython3.11-dev \
    && rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /usr/src/app

# Copy the Cargo.toml and Cargo.lock files first (for better caching)
COPY Cargo.toml ./


# Create empty source files to trigger dependency compilation
RUN mkdir -p src && \
    echo "fn main() {}" > src/main.rs

# Build dependencies - this will be cached unless Cargo.toml changes
RUN cargo build --release

# Now copy the actual source code
COPY src ./src/
COPY .env ./

# Build the application
RUN cargo build --release

# Create final lightweight image
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    python3 \
    python3-pip \
    python3-venv \
    ca-certificates \
    libssl-dev \
    libpython3.11-dev \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user for security
RUN useradd -m pyexec

# Create application directories
WORKDIR /app
RUN mkdir -p /app/venvs && chown -R pyexec:pyexec /app

# Copy the executable from the builder stage
COPY --from=builder /usr/src/app/target/release/pyexec-service /app/
COPY --from=builder /usr/src/app/.env /app/.env

# Set user to non-root
USER pyexec

# Expose the WebSocket port
ENV SERVER_HOST=0.0.0.0
ENV SERVER_PORT=8080
EXPOSE 8080

# Set the entry point
CMD ["/app/pyexec-service"]