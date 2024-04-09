# Use the official Rust image as the base image
FROM rust:latest

# Set the working directory
WORKDIR .

# Copy the Cargo.toml files
COPY Cargo.toml config.toml ./

# Copy the source code
COPY src ./src

# Accept a build argument for the build mode
ARG BUILD_MODE=release

# Build the application based on the build mode
RUN if [ "$BUILD_MODE" = "debug" ]; then \
    cargo build; \
else \
    cargo build --release; \
fi

# Expose the port your application uses
EXPOSE 8080

# Set the unbuffered environment variable
ENV RUST_BACKTRACE "1"

# Run the binary conditionally based on the build mode
CMD if [ "$BUILD_MODE" = "debug" ]; then ./target/debug/starknetid_server; else ./target/release/starknetid_server; fi
