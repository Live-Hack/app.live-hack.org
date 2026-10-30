FROM rust:1.89-bookworm AS build
WORKDIR /app

# cache dependencies
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && touch src/lib.rs && cargo build --release && rm -rf src

COPY src ./src
RUN touch src/main.rs src/lib.rs && cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates tzdata \
    && rm -rf /var/lib/apt/lists/*
RUN useradd --system --uid 10001 livehack
WORKDIR /app
RUN mkdir -p /app/data && chown livehack /app/data

COPY --from=build /app/target/release/app_live_hack ./app_live_hack
COPY assets ./assets
COPY html ./html
COPY utils ./utils
COPY migrations ./migrations

USER livehack
EXPOSE 8080
CMD ["./app_live_hack"]
