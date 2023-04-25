FROM lukemathwalker/cargo-chef:latest-rust-1.69.0 AS chef
WORKDIR /app
RUN apt update && apt install -y lld clang

FROM chef AS planner
COPY . .
# Compute a lock file for our project
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# Build project deps, NOT our app
RUN cargo chef cook --release --recipe-path recipe.json
# Up to this point, if deps stay the same then the layers should all be cached
COPY . .
ENV SQLX_OFFLINE true
RUN cargo build --release --bin zero2prod

# Runtime stage
FROM gcr.io/distroless/cc
WORKDIR /app
COPY --from=builder /app/target/release/zero2prod zero2prod
# need the config at runtime
COPY configuration configuration
ENV APP_ENVIRONMENT production
ENTRYPOINT [ "./zero2prod" ]