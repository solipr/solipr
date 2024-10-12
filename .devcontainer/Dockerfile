FROM mcr.microsoft.com/devcontainers/rust:1-1-bullseye

COPY --chown=vscode:vscode config.toml /home/vscode/.cargo/config.toml
RUN cargo install bacon sccache cargo-expand
