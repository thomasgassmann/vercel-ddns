FROM node:24-alpine AS web
WORKDIR /web
COPY web/package.json web/pnpm-lock.yaml ./
RUN corepack enable && pnpm install --frozen-lockfile
COPY web ./
RUN pnpm build

FROM rust:1-alpine AS planner
RUN apk add --no-cache musl-dev && cargo install cargo-chef
WORKDIR /build
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM rust:1-alpine AS builder
RUN apk add --no-cache musl-dev && cargo install cargo-chef
WORKDIR /build
COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
COPY --from=web /web/dist web/dist
RUN cargo build --release

FROM scratch
# reqwest 0.13's rustls uses the platform verifier, so ship a CA bundle.
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=builder /build/target/release/ddnser /ddnser
EXPOSE 8080
ENTRYPOINT ["/ddnser"]
