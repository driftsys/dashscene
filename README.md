# DashScene

[![CI](https://github.com/driftsys/dashscene/actions/workflows/ci.yml/badge.svg)](https://github.com/driftsys/dashscene/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

A semantic UI scene platform for adaptive cockpit and dashboard HMIs.

DashScene is designed for systems where UI must be renderer-neutral, adaptive, efficient to transport, and host-performed rather than fully pre-rendered.

## Workspace Crates

| Crate                                           | Description                                 |
| ----------------------------------------------- | ------------------------------------------- |
| [`dashscene`](crates/dashscene)                 | Umbrella crate for the DashScene ecosystem  |
| [`dashscore`](crates/dashscore)                 | IDE, studio, and authoring environment      |
| [`dashlang`](crates/dashlang)                   | Declarative language for expressing scenes  |
| [`dashc`](crates/dashc)                         | Compiler, lowering, and artifact generation |
| [`dashcue`](crates/dashcue)                     | Event, intent, and state model              |
| [`dashpaint`](crates/dashpaint)                 | Theming, appearance, and branding layer     |
| [`dashbuf`](crates/dashbuf)                     | Binary serialization and transport layer    |
| [`dashscene-core`](crates/dashscene-core)       | Canonical semantic scene model              |
| [`dashscene-engine`](crates/dashscene-engine)   | Adaptive layout engine and realization core |
| [`dashscene-compose`](crates/dashscene-compose) | Android Jetpack Compose host backend        |
| [`dashscene-unity`](crates/dashscene-unity)     | Unity host backend                          |
| [`dashscene-web`](crates/dashscene-web)         | Web host backend                            |

## Architecture

```text
DashScore (IDE / studio)
        |
DashLang (declarative language)
        |
DashC (compiler / lowering)
        |
DashScene Core (canonical model)
        |
DashScene Engine (adaptive layout)
        |
Host Backend (Compose / Unity / Web)
```

## License

MIT
