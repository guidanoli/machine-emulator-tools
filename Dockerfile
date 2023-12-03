# Copyright Cartesi and individual authors (see AUTHORS)
# SPDX-License-Identifier: Apache-2.0
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
# http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.
#

FROM --platform=linux/riscv64 riscv64/ubuntu:22.04 as tools-env
ARG IMAGE_KERNEL_VERSION=v0.19.1
ARG LINUX_VERSION=6.5.9-ctsi-1
ARG LINUX_HEADERS_URLPATH=https://github.com/cartesi/image-kernel/releases/download/${IMAGE_KERNEL_VERSION}/linux-libc-dev-${LINUX_VERSION}-${IMAGE_KERNEL_VERSION}.deb
ARG BUILD_BASE=/opt/cartesi

# Install dependencies
# ------------------------------------------------------------------------------
ENV LINUX_HEADERS_FILEPATH=/tmp/linux-libc-dev-${LINUX_VERSION}-${IMAGE_KERNEL_VERSION}.deb

RUN DEBIAN_FRONTEND=noninteractive apt-get update && \
    apt-get upgrade -y && \
    apt-get install -y --no-install-recommends \
      build-essential \
      ca-certificates \
      git \
      protobuf-compiler \
      rsync \
      wget \
      rust-all=1.58.1+dfsg1~ubuntu1-0ubuntu2 \
      && \
    wget -O ${LINUX_HEADERS_FILEPATH} ${LINUX_HEADERS_URLPATH} && \
    echo "aace918b830ee2d682c329b3361a3458ebc42ba5193ccb821fd0b2071ac821b3  ${LINUX_HEADERS_FILEPATH}" | sha256sum --check && \
    apt-get install -y --no-install-recommends ${LINUX_HEADERS_FILEPATH} && \
    mkdir -p ${BUILD_BASE} && \
    rm -rf /var/lib/apt/lists/* ${LINUX_HEADERS_FILEPATH}

FROM tools-env as builder
COPY sys-utils/ ${BUILD_BASE}/tools/sys-utils/

# build C/C++ tools
# ------------------------------------------------------------------------------
FROM builder as c-builder
RUN make -C ${BUILD_BASE}/tools/sys-utils/xhalt/ CROSS_COMPILE="" xhalt.toolchain
RUN make -C ${BUILD_BASE}/tools/sys-utils/yield/ CROSS_COMPILE="" yield.toolchain
RUN make -C ${BUILD_BASE}/tools/sys-utils/ioctl-echo-loop/ CROSS_COMPILE="" ioctl-echo-loop.toolchain
RUN make -C ${BUILD_BASE}/tools/sys-utils/rollup/ CROSS_COMPILE="" rollup.toolchain

# build rust tools
# ------------------------------------------------------------------------------
FROM builder as rust-builder

# NOTE: cargo update RAM usage keeps going up without this
RUN mkdir -p $HOME/.cargo && \
    echo "[net]" >> $HOME/.cargo/config && \
    echo "git-fetch-with-cli = true" >> $HOME/.cargo/config

COPY rollup-http/rollup-init ${BUILD_BASE}/tools/rollup-http/rollup-init
COPY rollup-http/rollup-http-client ${BUILD_BASE}/tools/rollup-http/rollup-http-client

# build rollup-http-server dependencies
FROM rust-builder as http-server-dep-builder
COPY rollup-http/rollup-http-server/Cargo.toml rollup-http/rollup-http-server/Cargo.lock ${BUILD_BASE}/tools/rollup-http/rollup-http-server/
RUN cd ${BUILD_BASE}/tools/rollup-http/rollup-http-server && \
    mkdir src/ && \
    echo "fn main() {}" > src/main.rs && \
    echo "pub fn dummy() {}" > src/lib.rs && \
    cargo build --release

# build rollup-http-server
FROM http-server-dep-builder as http-server-builder
COPY rollup-http/rollup-http-server/build.rs ${BUILD_BASE}/tools/rollup-http/rollup-http-server/
COPY rollup-http/rollup-http-server/src ${BUILD_BASE}/tools/rollup-http/rollup-http-server/src
RUN cd ${BUILD_BASE}/tools/rollup-http/rollup-http-server && \
    cargo build --release

# build echo-dapp dependencies
FROM rust-builder as echo-dapp-dep-builder
COPY rollup-http/echo-dapp/Cargo.toml rollup-http/echo-dapp/Cargo.lock ${BUILD_BASE}/tools/rollup-http/echo-dapp/
RUN cd ${BUILD_BASE}/tools/rollup-http/echo-dapp && \
    mkdir src/ && echo "fn main() {}" > src/main.rs && \
    cargo build --release

# build echo-dapp
FROM echo-dapp-dep-builder as echo-dapp-builder
COPY rollup-http/echo-dapp/src ${BUILD_BASE}/tools/rollup-http/echo-dapp/src
RUN cd ${BUILD_BASE}/tools/rollup-http/echo-dapp && \
    cargo build --release

# pack tools (deb)
# ------------------------------------------------------------------------------
FROM c-builder as packer
ARG TOOLS_DEB=machine-emulator-tools.deb
ARG STAGING_BASE=${BUILD_BASE}/_install
ARG STAGING_DEBIAN=${STAGING_BASE}/DEBIAN
ARG STAGING_SBIN=${STAGING_BASE}/usr/sbin
ARG STAGING_BIN=${STAGING_BASE}/usr/bin

RUN mkdir -p ${STAGING_DEBIAN} ${STAGING_SBIN} ${STAGING_BIN} ${STAGING_BASE}/etc && \
    echo "cartesi-machine" > ${staging_base}/etc/hostname && \
    cp ${BUILD_BASE}/tools/sys-utils/system-init/system-init ${STAGING_SBIN} && \
    cp ${BUILD_BASE}/tools/sys-utils/xhalt/xhalt ${STAGING_SBIN} && \
    cp ${BUILD_BASE}/tools/sys-utils/yield/yield ${STAGING_SBIN} && \
    cp ${BUILD_BASE}/tools/sys-utils/rollup/rollup ${STAGING_SBIN} && \
    cp ${BUILD_BASE}/tools/sys-utils/ioctl-echo-loop/ioctl-echo-loop ${STAGING_BIN} && \
    cp ${BUILD_BASE}/tools/sys-utils/misc/* ${STAGING_BIN}

COPY control ${STAGING_DEBIAN}/control

COPY --from=rust-builder ${BUILD_BASE}/tools/rollup-http/rollup-init/rollup-init ${STAGING_SBIN}
COPY --from=http-server-builder ${BUILD_BASE}/tools/rollup-http/rollup-http-server/target/release/rollup-http-server ${STAGING_BIN}
COPY --from=echo-dapp-builder ${BUILD_BASE}/tools/rollup-http/echo-dapp/target/release/echo-dapp ${STAGING_BIN}

RUN dpkg-deb -Zxz --root-owner-group --build ${STAGING_BASE} ${BUILD_BASE}/${TOOLS_DEB}
