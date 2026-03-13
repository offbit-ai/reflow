# MicroSandbox Server for Reflow
# Provides hardware-level VM isolation for secure script execution

# Use Ubuntu as base for MicroSandbox compatibility
FROM ubuntu:22.04

# Install runtime dependencies and tools
RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    netcat \
    python3 \
    python3-pip \
    nodejs \
    npm \
    git \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

# Install MicroSandbox using the official installer
RUN curl -sSL https://get.microsandbox.dev | sh

# Add PATH for MicroSandbox binary (installer puts it in ~/.local/bin)
ENV PATH="/root/.local/bin:${PATH}"

# Verify MicroSandbox is installed
RUN which msb || (echo "ERROR: msb not found in PATH" && exit 1)

# Install Python and Node.js packages that actors might need
RUN pip3 install numpy pandas msgpack redis
RUN npm install -g ws redis msgpack

# Create directories for scripts and configuration
RUN mkdir -p /var/lib/microsandbox/scripts \
    /var/lib/microsandbox/config \
    /var/lib/microsandbox/cache

# Copy Reflow-specific configuration
COPY config/microsandbox.yaml /var/lib/microsandbox/config/

# Environment variables for MicroSandbox
ENV MSB_PORT=8080
ENV MSB_HOST=0.0.0.0
ENV RUST_LOG=info

# Health check - just check if the port is listening
# MicroSandbox server doesn't have a /health endpoint
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD nc -z localhost 8080 || exit 1

# Expose the MicroSandbox server port
EXPOSE 8080

# Volume for scripts and sandboxes
VOLUME ["/var/lib/microsandbox/scripts", "/root/.msb"]

# Start MicroSandbox server in dev mode
CMD ["msb", "server", "start", "--dev", "--host", "0.0.0.0", "--port", "8080"]