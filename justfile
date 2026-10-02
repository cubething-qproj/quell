# ------------------------------------------
# SPDX-License-Identifier: MIT OR Apache-2.0
# -------------------------------- 𝒒𝒑𝒓𝒐𝒋 --

qproj := "qproj-scripts"

_default:
    just --list

# Build the workspace.
[working-directory('.')]
build *args:
    {{ qproj }} build {{ args }}

# Run the application.
[working-directory('.')]
play *args:
    {{ qproj }} play {{ args }}

# Run with dynamic linking + hot-patched systems (Dioxus CLI).
[working-directory('.')]
dev *args:
    RUST_LOG=vn-june-26=debug,bevy=info,wgpu=off,wgpu_hal=off,naga=warn
    dx serve --hot-patch --features dev {{ args }}

# Lint with Clippy.
[working-directory('.')]
check *args:
    cargo clippy {{ args }}

# Run clippy.
[working-directory('.')]
clippy *args:
    cargo clippy {{ args }}

# Check dependencies with cargo-deny.
[working-directory('.')]
deny:
    {{ qproj }} deny

# Run tests via cargo-nextest.
[working-directory('.')]
test *args:
    {{ qproj }} test {{ args }}

# Generate test coverage report.
[working-directory('.')]
coverage *args:
    {{ qproj }} coverage {{ args }}

# Fix all fixable issues.
[working-directory('.')]
fix *args:
    cargo clippy --fix {{ args }}

# Test CI locally with act.
[working-directory('.')]
ci *args:
    {{ qproj }} ci {{ args }}
