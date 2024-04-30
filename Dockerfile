FROM --platform=$BUILDPLATFORM docker.io/rust:1.75 as builder

ARG TARGETPLATFORM
ARG BUILDPLATFORM

RUN USER=root cargo new --bin hallway
WORKDIR ./hallway

COPY ["Cargo.toml", "Cargo.lock", "./"]
COPY "./distr" "./distr"
COPY "./.cargo" "./.cargo"

RUN sh ./distr/cross-build.sh && \
    cargo build --target $(cat "/.rust-target.temp") --release && \
    rm src/*.rs

ADD . ./

# Compile the project and copy the binary to an static place
RUN rm ./target/$(cat "/.rust-target.temp")/release/deps/hallway* && \
    cargo build --target $(cat "/.rust-target.temp") --release --features container && \
    cp ./target/$(cat "/.rust-target.temp")/release/hallway ./target/hallway-app


FROM --platform=$TARGETPLATFORM debian:bookworm-slim
ARG APP=/usr/src/app


RUN apt-get update \
    && apt-get install -y ca-certificates tzdata libssl3 \
    && rm -rf /var/lib/apt/lists/*

ENV TZ=Etc/UTC \
    APP_USER=appuser
    
RUN groupadd $APP_USER \
    && useradd -g $APP_USER $APP_USER \
    && mkdir -p ${APP} \
    && mkdir /config

EXPOSE 8080

ADD html_files /html_files
COPY --from=builder /hallway/target/hallway-app ${APP}/hallway
RUN chown -R $APP_USER:$APP_USER ${APP};  chown -R $APP_USER:$APP_USER /html_files
USER $APP_USER
WORKDIR ${APP}
CMD ["./hallway"]
