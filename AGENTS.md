# DashScene -- Agent Instructions

## Project

DashScene is a semantic UI scene platform for adaptive cockpit and dashboard HMIs. It separates authored UI intent from runtime layout and rendering, targeting Android Compose, Unity, and Web hosts.

## Build Commands

- `just assemble` -- build all crates
- `just test` -- run all tests
- `just lint` -- clippy + fmt check
- `just check` -- test + lint
- `just build` -- assemble + check
- `just fmt` -- format code
- `just doc` -- generate and open docs
- `just publish` -- publish crates to crates.io in dependency order
- `just clean` -- clean build artifacts

## Architecture

Cargo workspace with crates under `crates/`:

| Crate | Role |
|---|---|
| `dashscene` | Umbrella crate, re-exports |
| `dashscore` | IDE, studio, authoring environment |
| `dashlang` | Declarative language / source form |
| `dashc` | Compiler, lowering, artifact generation |
| `dashcue` | Event, intent, state model |
| `dashpaint` | Theming, appearance, branding |
| `dashbuf` | Binary serialization, FlatBuffer transport |
| `dashscene-core` | Canonical semantic scene model |
| `dashscene-engine` | Adaptive layout engine, realization core |
| `dashscene-compose` | Android Jetpack Compose host backend |
| `dashscene-unity` | Unity host backend |
| `dashscene-web` | Web host backend |

### Design principles

- Library crates use `thiserror` for typed errors
- Binary crates use `anyhow` for error handling
- Semantic model is renderer-neutral
- Layout is host-owned, not pre-baked
- Each host backend owns its platform orchestration

## Conventions

- Edition 2024, resolver 3
- MIT license
- Conventional Commits (imperative mood)
- One concept per module, 300-line soft / 500-line hard limit
- Zero warnings policy (`-D warnings`)
- No `utils`, `helpers`, or `misc` modules
