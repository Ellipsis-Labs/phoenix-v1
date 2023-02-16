# Get rust and solana toolchain
FROM ellipsislabs/solana:latest
# Download the phoenix-v1 source code
RUN git clone https://github.com/Ellipsis-Labs/phoenix-v1.git /build
# Checkout the commit that was used to build the binary
RUN git checkout d66d2cddc7bcfd9fb3d89e5242e97f6928cd7361
# Run the build script
RUN cargo build-sbf -- --locked --frozen
