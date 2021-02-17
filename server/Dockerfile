FROM rustlang/rust:nightly as builder

# Agones SDK building part taken from
# https://github.com/googleforgames/agones/blob/master/examples/rust-simple/Dockerfile
# Copyright 2018 Google LLC All Rights Reserved.
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.
RUN useradd -m build
# Rust SDK depends on https://github.com/pingcap/grpc-rs and it requires CMake and Go

## Cmake
ENV CMAKE_MINOR_VERSION=v3.10 \
    CMAKE_FULL_VERSION=3.10.3
RUN mkdir -p /usr/src/cmake \
  && curl -fSLO https://cmake.org/files/${CMAKE_MINOR_VERSION}/cmake-${CMAKE_FULL_VERSION}.tar.gz \
  && curl -fSLO https://cmake.org/files/${CMAKE_MINOR_VERSION}/cmake-${CMAKE_FULL_VERSION}-SHA-256.txt \
  && sha256sum -c cmake-${CMAKE_FULL_VERSION}-SHA-256.txt 2>&1 | grep OK \
  && tar xf cmake-${CMAKE_FULL_VERSION}.tar.gz -C /usr/src/cmake --strip-components=1 \
  && rm -f cmake-${CMAKE_FULL_VERSION}.* \
  && cd /usr/src/cmake \
  && ./configure && make -j$(nproc) && make install

## Go
ENV GO_VERSION=1.10.2 \
    GO_CHECKSUM=4b677d698c65370afa33757b6954ade60347aaca310ea92a63ed717d7cb0c2ff
RUN mkdir -p /usr/local/go \
  && curl -fSO https://dl.google.com/go/go${GO_VERSION}.linux-amd64.tar.gz \
  && shasum -a 256 go${GO_VERSION}.linux-amd64.tar.gz | grep ${GO_CHECKSUM} \
  && tar xf go${GO_VERSION}.linux-amd64.tar.gz -C /usr/local/go --strip-components=1 \
  && rm -f go${GO_VERSION}.linux-amd64.tar.gz
ENV PATH $PATH:/usr/local/go/bin

# SDK
# MODIFIED TO FIT THIS REPO STRUCTURE
COPY agones/sdks/rust/src /home/builder/agones/sdks/rust/src
COPY agones/sdks/rust/Cargo.toml /home/builder/agones/sdks/rust/

# END OF COPY

WORKDIR /home/builder/server
# Rust build cache hack https://stackoverflow.com/questions/58473606/cache-rust-dependencies-with-docker-build
COPY server/Cargo.toml .
COPY common/ common/
RUN echo "fn main() {}" > dummy.rs
RUN sed -i 's#src/main.rs#dummy.rs#' Cargo.toml
RUN cargo build --release

COPY server/ .
RUN cargo install --features clustered --path .

FROM debian:buster-slim
RUN apt-get update && apt-get install -y libssl-dev && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/cargo/bin/server /usr/local/bin/server
CMD ["server"]
