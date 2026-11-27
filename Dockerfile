FROM rust:1.90-bookworm AS build
WORKDIR /src
COPY .cargo .cargo
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --locked --release -p wbpc-server
FROM debian:bookworm-slim
RUN install -d -m 0700 -o 1000 -g 1000 /data /data/access_account
COPY --from=build /src/target/release/wbpc-server /usr/local/bin/wbpc-server
ENV WBP_ADDR=0.0.0.0:3040 WBP_DATA_DIR=/data WBP_ACCESS_DIR=/data/access_account WBP_CONTAINER=1
USER 1000:1000
EXPOSE 3040
ENTRYPOINT ["/usr/local/bin/wbpc-server"]
