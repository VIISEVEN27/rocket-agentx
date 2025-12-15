ARG pkg=rocket-agentx

FROM rust:1.91.0-alpine3.22 AS builder

WORKDIR /build

COPY . .

RUN sed -i 's/dl-cdn.alpinelinux.org/mirrors.aliyun.com/g' /etc/apk/repositories \
    && apk add --update-cache --no-cache build-base openssl-dev cmake

ENV RUSTFLAGS="-C target-feature=-crt-static"

RUN echo -e "[source.crates-io]\nreplace-with = \"aliyun\"\n\n[source.aliyun]\nregistry = \"sparse+https://mirrors.aliyun.com/crates.io-index/\"" >> $CARGO_HOME/config.toml \
    && cargo build --release

FROM alpine:3.22

WORKDIR /app

RUN sed -i 's/dl-cdn.alpinelinux.org/mirrors.aliyun.com/g' /etc/apk/repositories \
    && apk update --quiet \
    && apk add --no-cache libgcc openssl tzdata

RUN cp /usr/share/zoneinfo/Asia/Shanghai /etc/localtime \
    && echo "Asia/Shanghai" > /etc/timezone

COPY --from=builder /build/target/release/$pkg ./

COPY --from=builder /build/Rocket.toml ./

ENTRYPOINT ["./main"]
