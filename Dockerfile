
ARG RUST_VERSION=1.85.0
ARG NAME=rust-template

FROM rust:${RUST_VERSION}-bullseye AS build
ADD . /app
WORKDIR /app
ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
RUN apt-get install -y git \
    && cargo build -p ${NAME} --release

FROM debian:11
WORKDIR /root/
COPY --from=build  /app/target/release/${NAME} /bin
