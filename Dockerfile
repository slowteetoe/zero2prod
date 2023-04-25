FROM rust:1.69.0 AS builder
RUN update-ca-certificates
WORKDIR /app
RUN apt update && apt install -y lld clang
COPY . .
ENV SQLX_OFFLINE true
RUN cargo build --release

# Runtime stage
FROM gcr.io/distroless/cc
WORKDIR /app
COPY --from=builder /app/target/release/zero2prod zero2prod
# need the config at runtime
COPY configuration configuration
ENV APP_ENVIRONMENT production
ENTRYPOINT [ "./zero2prod" ]