# Official Microsoft Rust image
FROM mcr.microsoft.com/devcontainers/rust:1-bookworm

# Install Tesseract and Leptonica C libraries
RUN apt-get update && export DEBIAN_FRONTEND=noninteractive \
    && apt-get -y install --no-install-recommends \
    libtesseract-dev \
    libleptonica-dev \
    tesseract-ocr \
    pkg-config \
    cmake \
    clang \
    libclang-dev

# Additional Rust tools
USER vscode
RUN rustup component add clippy rustfmt