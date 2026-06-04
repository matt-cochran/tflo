# Rust development container for tflo
FROM mcr.microsoft.com/devcontainers/rust:latest

# Install additional tools and fix cargo directory ownership for vscode user
RUN rustup component add rustfmt clippy rust-src \
    && cargo install cargo-watch cargo-expand cargo-audit cargo-outdated \
    && chown -R vscode:vscode /usr/local/cargo

# Set up working directory
WORKDIR /workspaces/tflo

# Default command
CMD ["bash"]

