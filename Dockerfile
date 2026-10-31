# Setup downloads dependencies. Verification itself runs with --network none.
FROM rust:1.95.0-slim-bookworm@sha256:d7482085ff5b415f84dba5647ae71606650bdef00db7aeb69f4b3d170c3e4082
RUN apt-get update \
    && apt-get install -y --no-install-recommends git nodejs npm ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && rustup component add rustfmt clippy
COPY tools/jsonc-oracle/package*.json /opt/jsonc-oracle/
RUN npm ci --prefix /opt/jsonc-oracle --ignore-scripts --no-audit --no-fund \
    && npm cache clean --force
WORKDIR /app
COPY . .
RUN cargo vendor --locked /opt/vendor > /opt/vendor-config.toml \
    && chmod -R a+rX /app /opt/vendor /opt/jsonc-oracle /usr/local/rustup
ENV CARGO_HOME=/tmp/cargo \
    CARGO_TARGET_DIR=/tmp/target \
    CARGO_NET_OFFLINE=true \
    CARGO_VENDOR_CONFIG=/opt/vendor-config.toml \
    XDG_CONFIG_HOME=/tmp/xdg \
    NODE_PATH=/opt/jsonc-oracle/node_modules
USER 65534:65534
CMD ["bash", "scripts/verify.sh"]
