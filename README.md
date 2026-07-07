# Thesis — Semantic Code Search for C

A semantic code-search tool for C, built on slotted e-graphs. Rust core with a CLI for metrics work and PyO3 bindings as the eventual primary distribution.

## Repository Structure

```
csearch/                              ← repo root, Cargo workspace
├── Cargo.toml                        ← [workspace] members = [...]
├── README.md
├── crates/
│   ├── slotted-egraphs/              ← vendored fork
│   ├── csearch-core/                 ← Rust library: language, analyses, search API
│   ├── csearch-cli/                  ← thin Rust binary for metrics / ad-hoc use
│   └── csearch-py/                   ← PyO3 bindings (cdylib + maturin)
└── examples/
    └── c-samples/                    ← small .c files for tests + benches
```

## Building

```bash
# Build Rust library
cargo build -p csearch-core

# Build CLI
cargo build -p csearch-cli

# Build Python bindings
cd crates/csearch-py
maturin develop
```

## Milestones

- **M1** — Library upstreams (MultiBind, semantic_search, spans)
- **M2** — csearch-core skeleton
- **M3** — C lowering pipeline
- **M4** — CodeBase API + CLI
- **M5** — PyO3 bindings
