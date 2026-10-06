# FreeKerf

Software livre de design e controle para máquinas laser (Grbl, grblHAL, Smoothie, Marlin…), escrito em Rust, multiplataforma, para mouse, teclado e toque. É a evolução do [LaserGRBL](https://github.com/arkypita/LaserGRBL).

> **Status: planejamento.** Ainda não há código. O que será construído, a arquitetura em camadas e os marcos estão em **[PLAN.md](PLAN.md)**.

| Repositório | Conteúdo |
|---|---|
| **freekerf** (este) | monorepo Rust: core, config, nativo, runtime, UI e apps |
| [conformance](https://github.com/freekerf/conformance) | suíte que especifica o comportamento do core atual do LaserGRBL |
| [design](https://github.com/freekerf/design) | UX, identidade visual, design system e protótipo navegável |
| [library](https://github.com/freekerf/library) | biblioteca de materiais e perfis de máquina |
| [website](https://github.com/freekerf/website) | site e documentação de usuário |

## Licença

[GPL-3.0-or-later](LICENSE). Derivado do LaserGRBL, Copyright (c) 2016 Diego Settimi (ver [NOTICE](NOTICE)). As dependências são verificadas com `cargo deny` ([deny.toml](deny.toml)).
