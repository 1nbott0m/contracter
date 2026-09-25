FROM rust:1.94-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release -p server

FROM debian:bookworm-slim
RUN useradd --system --uid 10001 contracter
COPY --from=build /src/target/release/contracter-server /usr/local/bin/contracter-server
USER contracter
EXPOSE 8080 9090
ENTRYPOINT ["/usr/local/bin/contracter-server"]
