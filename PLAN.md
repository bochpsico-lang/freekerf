# FreeKerf: plano do monorepo Rust

O FreeKerf é software livre (**GPL-3.0-or-later**) de design e controle para máquinas laser (Grbl, grblHAL, Smoothie, Marlin…). É a evolução do LaserGRBL (© 2016 Diego Settimi), reescrito em Rust, multiplataforma (Linux, Windows, macOS; tablets depois), com uso fluido por mouse/teclado **e** toque.

Este repositório ainda não tem código. Este documento descreve **o que precisa ser construído, em que ordem e com quais regras**. Ele é a referência para quem (pessoa ou agente) for implementar.

Repositórios relacionados na organização [`freekerf`](https://github.com/freekerf):

| Repositório | Papel para este monorepo |
|---|---|
| [`conformance`](https://github.com/freekerf/conformance) | especificação executável do core atual do LaserGRBL (Python). As camadas *protocol* e *golden* rodam contra o build Rust com `LASERGRBL_HOST=rust` |
| [`design`](https://github.com/freekerf/design) | UX, identidade, design system (tokens `fk-`), protótipo navegável e a análise de toolkit (`tech-notes.md`) |
| [`library`](https://github.com/freekerf/library) | biblioteca de materiais e perfis de máquina consumida por `freekerf-materials` |
| [`website`](https://github.com/freekerf/website) | site e documentação de usuário |

---

## 1. Princípios

1. **Camadas separadas e verificadas por máquina.** A lógica de negócio não conhece I/O, sistema operacional nem UI (ver §3).
2. **Protocolo "sans-IO".** A máquina de estados Grbl recebe bytes, linhas e tempo, e devolve bytes a enviar e eventos. Assim ela é testável sem threads, sem relógio real e sem serial.
3. **Uma API de serviço para todas as interfaces.** CLI, app desktop e cliente remoto (tablet) usam a mesma API de comandos e eventos (`freekerf-service`).
4. **Comportamento observável validado pela suíte `conformance`.** Não é obrigatório reproduzir os bugs do LaserGRBL (ver §6).
5. **Segurança do operador acima de conveniência.** PARAR sempre disponível, nada dispara o laser sem confirmação, e os estados da máquina são explícitos.

## 2. Estrutura do repositório

```
freekerf/
├── Cargo.toml                 workspace virtual (members por camada)
├── deny.toml                  política de licenças e fontes (já configurado)
├── rust-toolchain.toml        toolchain fixa + MSRV
├── crates/
│   ├── core/                  ── CAMADA CORE: lógica pura, sem I/O
│   │   ├── freekerf-gcode       parser/modelo G-code, estado modal, arcos, análise (ranges, quadrante), estimativa de tempo
│   │   ├── freekerf-geometry    caminhos, curvas, flatten, booleanas, offset, transformações
│   │   ├── freekerf-raster      operações de imagem, 12+ dithers, line-to-line, preparação para vetorização
│   │   ├── freekerf-document    modelo do projeto: objetos, camadas por cor, parâmetros de corte, formato .fkproj
│   │   ├── freekerf-import      SVG / DXF / imagem / G-code → documento (lê bytes, não arquivos)
│   │   ├── freekerf-planner     documento → trabalho: ordem, otimização de percurso, geradores (teste de material, Hershey), G-code
│   │   ├── freekerf-protocol    máquina de estados sans-IO: Grbl/grblHAL, Smoothie, Marlin, VigoWork
│   │   └── freekerf-emulator    modelo de um dispositivo Grbl 1.1 (para testes e modo simulador)
│   ├── config/                ── CAMADA CONFIG: dados persistentes e esquemas
│   │   ├── freekerf-settings    configurações do app e das máquinas (serde, schema versionado, migrações)
│   │   ├── freekerf-materials   formato e consulta da biblioteca de materiais (repo `library`)
│   │   └── freekerf-migrate     importação do LaserGRBL: Settings.bin, botões .zbn, materiais, contadores de uso
│   ├── native/                ── CAMADA NATIVA: tudo que depende do SO ou de hardware
│   │   ├── freekerf-transport   trait Transport + serial, Telnet, WebSocket (ESP8266), emulador; Android USB depois
│   │   ├── freekerf-discovery   descoberta de máquinas na rede (mDNS/varredura)
│   │   └── freekerf-platform    diretórios, gravação atômica, sons, notificações, info do SO
│   └── runtime/               ── CAMADA RUNTIME: orquestração
│       ├── freekerf-machine     driver: protocolo + transporte + relógio numa thread de I/O; ciclo de vida do job
│       ├── freekerf-service     fachada da aplicação: comandos → eventos/snapshots; sessões; API usada por todas as UIs
│       └── freekerf-remote      servidor WebSocket para o cliente remoto (tablet) — fase M6
├── ui/                        ── CAMADA UI (não-Rust)
│   ├── web/                     frontend (TypeScript) do app desktop/tablet; parte do protótipo do repo `design`
│   ├── tokens/                  tokens `fk-` gerados a partir de `design/tokens.json`
│   └── i18n/                    traduções Fluent (.ftl) compartilhadas entre Rust e web
├── apps/                      ── CAMADA APPS: binários finais (casca fina)
│   ├── cli/                     binário `freekerf` (headless)
│   └── desktop/                 app Tauri 2 (`src-tauri` = crate `freekerf-desktop`)
├── tools/xtask/               automações: check-layers, resx→ftl, sync-tokens, release
└── docs/
    ├── ARCHITECTURE.md
    ├── DIVERGENCES.md           diferenças intencionais em relação ao LaserGRBL
    ├── host-protocol.md         protocolo JSON Lines do `freekerf host --stdio`
    └── adr/                     registros de decisão (toolkit de UI, numérica, threads…)
```

Nomes de crate: sempre `freekerf-<nome>`. Todos com `license = "GPL-3.0-or-later"` herdado do `[workspace.package]`.

## 3. Regras de dependência entre camadas

| Camada | Pode depender de | Não pode |
|---|---|---|
| **core** | só outros crates core e bibliotecas puras | `std::fs`, `std::net`, threads, relógio do sistema, crates de SO/serial/UI. Deve compilar para `wasm32-unknown-unknown` |
| **config** | core | native, runtime, apps (o I/O de arquivo chega por trait/`platform`) |
| **native** | core | config, runtime, apps |
| **runtime** | core, config, native | apps, UI |
| **apps** | runtime (via `freekerf-service`), config, ui | acessar `protocol`/`transport` diretamente |
| **ui/web** | apenas a API exposta pelo `freekerf-service` (IPC do Tauri ou WebSocket remoto) | lógica de máquina ou de G-code própria |

Verificação automática:
- `cargo xtask check-layers` lê o `cargo metadata` e falha se uma aresta violar a tabela. Roda no CI.
- `cargo build -p <crate core> --target wasm32-unknown-unknown` no CI garante que o core não usa I/O.
- `#![forbid(unsafe_code)]` em todo crate, exceto `freekerf-transport` e `freekerf-platform` (onde houver justificativa documentada).

## 4. Licenças (já configurado em `deny.toml`)

- Projeto: **GPL-3.0-or-later**; crédito em `NOTICE` ("Derivado do LaserGRBL, Copyright (c) 2016 Diego Settimi"). Portar o C# diretamente é permitido.
- `cargo deny check` no CI. A lista em `deny.toml` é uma **allow-list**: GPL-2.0-only, AGPL, SSPL, BUSL, EPL, CDDL, licenças NC/proprietárias e crates sem licença são rejeitados. Crates de terceiros só do crates.io (sem git/registries desconhecidos).
- Potrace (GPL-2.0-or-later) é aceito: a obra combinada fica GPL-3.0.
- Evite dependências `GPL-3.0-only` (prenderiam o projeto à v3).
- Assets (fontes, ícones) seguem `design/CREDITS.md`.

## 5. Marcos

Cada marco tem critério de aceite objetivo. Não comece um marco sem o anterior verde no CI.

### M0 — Bootstrap (1–2 semanas)
- Workspace virtual, `rust-toolchain.toml` (stable atual + MSRV fixado), `rustfmt.toml`, lints de workspace (`clippy::pedantic` seletivo, `unsafe_code = "forbid"`).
- `tools/xtask`: `check-layers`.
- CI no GitHub Actions (o esqueleto já está em `.github/workflows/ci.yml`): fmt, clippy `-D warnings`, test, `cargo deny check`, `check-layers`, build wasm do core, `cargo check` para Windows (`x86_64-pc-windows-gnu`) e macOS (`aarch64-apple-darwin`). Testes rodam em Linux.
- `docs/ARCHITECTURE.md` e os ADRs iniciais: threads + canais (`crossbeam-channel`) vs `tokio`; representação numérica (o C# usa `decimal` → `rust_decimal` onde a formatação do *wire* exigir); `Clock` injetável.

**Aceite:** CI verde com um crate mínimo em cada camada; `check-layers` falha num PR de teste que viola a regra.

### M1 — Core de máquina, headless (6–10 semanas)
Crates: `gcode`, `protocol`, `emulator`, `transport` (serial + emulador), `machine`, `settings`, `service`, `apps/cli`.

- Paridade funcional com o escopo "core" do `conformance/SCOPE.md`: modos de streaming, contagem do buffer RX, `ok`/`error:N`/`ALARM`/`<status>`/`[MSG]`, `$$`/`$I`, comandos em tempo real, overrides, jog e jog contínuo, ciclo de vida do job, retomar, código custom com `[expr]`, detecção de firmware, contadores de vida útil do laser, emulador.
- CLI `freekerf`:
  - `analyze <arquivo.nc> --json`: o mesmo JSON de `conformance/src/lasergrbl_harness/golden.py`;
  - `send`, `console`;
  - `emulate --pty`: expõe o emulador numa PTY, útil inclusive para testar o LaserGRBL antigo;
  - `host --stdio`: protocolo JSON Lines (`docs/host-protocol.md`) que implementa o `HostAdapter` da suíte.
- No repo `conformance`: PR adicionando `RustHost` e o analisador Rust, selecionados por `LASERGRBL_HOST=rust`.
- Arquivos grandes: carga em streaming e análise paralela (`rayon`); benchmark com um raster sintético de ~1M linhas.

**Aceite:**
- camadas *protocol* e *golden (G-code)* do `conformance` verdes com `LASERGRBL_HOST=rust`, rodadas 3 vezes sem flakiness (ou com a divergência registrada, ver §6);
- cobertura (`cargo llvm-cov`) ≥ 95% em `gcode`/`protocol` e ≥ 85% nos demais;
- `proptest`: o parser não entra em pânico com nenhuma entrada, parse→format→parse é estável e o buffer nunca passa do limite;
- `BENCHMARKS.md` comparando com o C#.

### M2 — Importadores e planner (6–10 semanas)
Crates: `geometry`, `raster`, `document`, `import`, `planner`, `materials`, `migrate`.

- SVG (camadas por cor, preenchimento/corte, parâmetros por camada), DXF (arcos verdadeiros, splines → arcos com tolerância), imagem (ajustes, dithers, line-to-line), vetorização (Potrace e centerline), geradores (grade potência × velocidade, teste de tons) e fontes Hershey.
- `freekerf-migrate`: Settings.bin do LaserGRBL (BinaryFormatter, só leitura), botões `.zbn`, materiais e contadores.
- Formato de projeto `.fkproj` versionado.

**Aceite:** camada *golden (importers)* do `conformance` verde com `LASERGRBL_HOST=rust`. Os casos `platform_dependent` (dependentes do GDI+) são comparados com tolerância documentada.

### M3 — Spike de UI (2–3 semanas)
Execute a tabela "O que o spike precisa validar" de `design/tech-notes.md` (canvas com 20 000 caminhos + imagem de 8000×6000, gestos, IPC de geometria a 60 Hz, serial, cliente remoto, acessibilidade, empacotamento) para **Tauri 2** e, nos testes 1, 2 e 5, para **Flutter + flutter_rust_bridge**.

**Aceite:** `docs/adr/000X-ui-toolkit.md` com a decisão e os números medidos. A recomendação atual é Tauri 2; ela só muda se o WebKitGTK no Linux reprovar os testes 1 ou 2.

### M4 — App desktop MVP (10–16 semanas)
- `ui/web` a partir do protótipo de `design/prototypes/app`: modos Projetar/Produzir, canvas (pan, zoom, pinça, seleção, alças), camadas por cor, inspetor, painel da máquina, console, confirmação antes de disparar, recuperação de trabalho, boas-vindas e migração do LaserGRBL.
- Todos os itens **MVP** da matriz em `design/research.md` §6.
- i18n: `tools/xtask resx2ftl` converte as traduções `.resx` do LaserGRBL que ainda se aplicam.
- Densidade automática por tipo de entrada; temas claro, escuro e alto contraste com os tokens `fk-`.

**Aceite:** os 8 fluxos de `design/ux.md` executáveis ponta a ponta contra o emulador; testes E2E (Playwright via WebDriver do Tauri) desses fluxos no CI Linux.

### M5 — v1 e distribuição (8–12 semanas)
- Itens **v1** da matriz: edição de nós, booleanas, offset, array, trace, biblioteca de materiais, teste de material, editor `$$`, Smoothie/Marlin na UI, Telnet/WebSocket + descoberta, notificações.
- Empacotamento:
  - Linux: `.deb`, AppImage, Flatpak;
  - Windows: MSI assinado;
  - macOS: DMG assinado e notarizado;
  - atualização automática opcional e desativável.
- CI em matriz real (Linux, Windows, macOS).

### M6 — Tablet e controle remoto (depois)
`freekerf-remote` (WebSocket na LAN, *heartbeat* do jog contínuo, PARAR < 50 ms) e o cliente em tablet (Tauri mobile ou web servida pelo core). Android com USB host direto; o iPad funciona só como cliente remoto.

## 6. Divergências do LaserGRBL

- **Não é preciso reproduzir os bugs** listados em `conformance/FINDINGS.md`. O comportamento do FreeKerf pode ser diferente, desde que **documentado**.
- Todo item de FINDINGS recebe uma linha em `docs/DIVERGENCES.md`: *mantido* / *corrigido* / *não se aplica*, com justificativa e o teste afetado.
- Quando um teste do `conformance` falhar por uma divergência intencional, marque-o **apenas para o host Rust** (marker `rust_divergence("DIV-NNN")` no repo `conformance`) apontando para a entrada do DIVERGENCES.md. Não apague o teste nem afrouxe a asserção para o host C#.

## 7. Qualidade

- `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo deny check`, `cargo xtask check-layers`.
- Bibliotecas sem `unwrap`/`expect`/`panic!` em caminhos de produção; erros com `thiserror` nas libs e `anyhow` só em apps; logs com `tracing`.
- Testes: unitários por crate; testes de protocolo determinísticos com `Clock` simulado; `proptest` nos parsers; fuzzing (`cargo-fuzz`) para G-code, SVG e DXF a partir do M2.
- Documentação: rustdoc em toda API pública dos crates `core` e `service`.

## 8. Segurança

- Nenhum teste automatizado abre dispositivos seriais reais (`/dev/ttyUSB*`, `/dev/ttyACM*`, `COMx`). Use PTY, o emulador ou o fake Grbl do `conformance`. Testes em máquina real são manuais e documentados.
- PARAR (soft reset + desliga o laser) tem prioridade sobre qualquer fila e funciona em qualquer estado.
- Nenhum comando que liga o laser é enviado sem uma ação explícita do usuário. O jog contínuo por toque exige *heartbeat* (homem-morto).

## 9. Como começar

1. Leia `conformance/README.md`, `SCOPE.md` e `FINDINGS.md`, e `design/README.md`, `ux.md` e `tech-notes.md`.
2. Execute o M0 e abra um PR com o esqueleto.
3. M1 em PRs pequenos, um crate por vez: `gcode` → `analyze --json` + golden → `protocol` → `transport`/`machine` → `host --stdio` + `RustHost` → variantes, emulador, settings.

## 10. Perguntas em aberto

- Formato de configuração: TOML (legível) ou JSON (mais simples para a UI web)?
- Telemetria opcional (o LaserGRBL tem `UsageStats`): manter, com opt-in explícito, ou remover?
- Atualização automática: canal próprio ou só gerenciadores de pacote?
