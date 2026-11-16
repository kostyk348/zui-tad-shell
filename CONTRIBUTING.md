# Contributing to ZUI-TAD Shell

Thanks for helping out. This file says how to build and test the project, how to
report bugs and security problems, and how to write commit messages.

## Build and test

These are the same commands CI runs (`.github/workflows/ci.yml`).

### Core and shell (CPU only, no system dependencies)

```bash
cargo fmt --all -- --check
cargo test -p canvas-engine -p phosphor
cargo clippy -p canvas-engine -p phosphor --all-targets
cargo build --release -p phosphor --bins
cargo run --release -q -p phosphor --bin gen_bg -- 960 540
```

### Compositor (smithay, needs system libraries)

On Debian/Ubuntu, install the dependencies first:

```bash
sudo apt-get update
sudo apt-get install -y --no-install-recommends \
  pkg-config libwayland-dev libxkbcommon-dev libudev-dev libseat-dev \
  libinput-dev libdisplay-info-dev libgbm-dev libdrm-dev libegl-dev libgles-dev
```

Then:

```bash
cargo check -p compositor --features smithay --bins
cargo test -p compositor --features smithay
cargo clippy -p compositor --features smithay --bins
```

Please make sure these pass before you open a pull request.

## Reporting a bug

Open an issue at <https://github.com/kostyk348/zui-tad-shell/issues> and include:

- the version: a release tag, or the commit you built (`git rev-parse HEAD`);
- your platform: distribution and kernel, GPU and driver, and how you ran it
  (nested in another session and which one, from a TTY, or `zui-preview`);
- the output of `zui-compositor --check`, if the compositor is involved;
- steps to reproduce, what you expected and what happened, plus any logs or
  screenshots.

## Reporting a security problem

Do not open a public issue. Report it privately through a GitHub security
advisory: <https://github.com/kostyk348/zui-tad-shell/security/advisories/new>.

## Commit messages

Write a short imperative summary line (e.g. `compositor: fix resize from top edge`),
and put details in the body if you need them.
