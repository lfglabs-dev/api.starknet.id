FROM rust:1.75

WORKDIR /app

COPY Cargo.toml config.toml ./
COPY src ./src

ARG BUILD_MODE=release

RUN if [ "$BUILD_MODE" = "debug" ]; then \
    cargo build; \
else \
    cargo build --release; \
fi

EXPOSE 8080

ENV RUST_BACKTRACE "1"

CMD if [ "$BUILD_MODE" = "debug" ]; then ./target/debug/starknetid_server; else ./target/release/starknetid_server; fi
