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

FROM ubuntu:22.04 AS tools-env
ARG IMAGE_KERNEL_VERSION=v0.20.0
ARG LINUX_VERSION=6.5.13-ctsi-1
ARG LINUX_HEADERS_URLPATH=https://github.com/cartesi/image-kernel/releases/download/${IMAGE_KERNEL_VERSION}/linux-libc-dev-riscv64-cross-${LINUX_VERSION}-${IMAGE_KERNEL_VERSION}.deb
ARG BUILD_BASE=/opt/cartesi

# Install dependencies
# ------------------------------------------------------------------------------
ENV LINUX_HEADERS_FILEPATH=/tmp/linux-libc-dev-riscv64-cross-${LINUX_VERSION}-${IMAGE_KERNEL_VERSION}.deb

RUN <<EOF
set -e

export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get upgrade -y
apt-get install -y --no-install-recommends \
        dpkg-dev \
        g++-12 \
        gcc-12 \
        make \
        ca-certificates \
        git \
        wget \
        libclang-dev \
        pkg-config \
        dpkg-cross \
        gcc-12-riscv64-linux-gnu \
        g++-12-riscv64-linux-gnu

for tool in cpp g++ gcc gcc-ar gcc-nm gcc-ranlib gcov gcov-dump gcov-tool; do
    update-alternatives --install /usr/bin/riscv64-linux-gnu-$tool riscv64-linux-gnu-$tool /usr/bin/riscv64-linux-gnu-$tool-12 12
    update-alternatives --install /usr/bin/$tool $tool /usr/bin/$tool-12 12
done
update-alternatives --install /usr/bin/cc cc /usr/bin/gcc-12 12

wget -O ${LINUX_HEADERS_FILEPATH} ${LINUX_HEADERS_URLPATH}
echo "2723435e8b45d8fb7a79e9344f6dc517b3dbc08e03ac17baab311300ec475c08  ${LINUX_HEADERS_FILEPATH}" | sha256sum --check
apt-get install -y --no-install-recommends ${LINUX_HEADERS_FILEPATH}

rm -rf /var/lib/apt/lists/* ${LINUX_HEADERS_FILEPATH}
EOF

# build rust tools
# ------------------------------------------------------------------------------
FROM tools-env AS rust-env
ENV RUSTUP_VERSION=1.27.1
ENV RUST_TOOLCHAIN_VERSION=1.80.1
ENV CARGO_HOME=${HOME}/.cargo
ENV PATH=${CARGO_HOME}/bin:${PATH}

RUN cd  && \
    wget https://github.com/rust-lang/rustup/archive/refs/tags/${RUSTUP_VERSION}.tar.gz && \
    echo "f5ba37f2ba68efec101198dca1585e6e7dd7640ca9c526441b729a79062d3b77  ${RUSTUP_VERSION}.tar.gz" | sha256sum --check && \
    tar -xzf ${RUSTUP_VERSION}.tar.gz && \
    bash rustup-${RUSTUP_VERSION}/rustup-init.sh \
        -y \
        --default-toolchain ${RUST_TOOLCHAIN_VERSION} \
        --profile minimal \
        --target riscv64gc-unknown-linux-gnu && \
    printf '[target.riscv64gc-unknown-linux-gnu]\nlinker = "riscv64-linux-gnu-gcc"\n' >> ${CARGO_HOME}/config.toml \
    rm -rf rustup-${RUSTUP_VERSION} ${RUSTUP_VERSION}.tar.gz
