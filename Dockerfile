FROM rust:latest AS base
RUN cargo install sccache
RUN cargo install cargo-chef
ENV RUSTC_WRAPPER=sccache SCCACHE_DIR=/sccache

FROM base AS planner
WORKDIR /telegram-bot-api-proxy
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=$SCCACHE_DIR,sharing=locked \
    cargo chef prepare --recipe-path recipe.json

FROM base AS builder
WORKDIR /telegram-bot-api-proxy
COPY --from=planner /telegram-bot-api-proxy/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=$SCCACHE_DIR,sharing=locked \
    cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=$SCCACHE_DIR,sharing=locked \
    cargo build --release

FROM base AS runner
WORKDIR /telegram-bot-api-proxy
COPY --from=builder /telegram-bot-api-proxy/target/release/telegram-bot-api-proxy /bin/telegram-bot-api-proxy
CMD [ "telegram-bot-api-proxy" ]
