# 🗺️ Roadmap

## 🔵 Versione 0.1 — Foundation
- [x] Struttura del progetto Rust
- [x] Connessione PostgreSQL read-only
- [x] Schema loader
- [x] Policy engine (base)
- [x] Query builder controllato (select + filters + limit)
- [x] MCP tool: `select` (typed contract + read-only execution; transport follows)
- [x] Audit log (foundation JSONL; tool-level coverage follows MCP layer)
- [x] Errori strutturati (public sanitized error envelope)
- [ ] README completo

## 🔵 Versione 0.2 — Safety & Observability
- [x] Rate limiting (fixed-window server-side per client)
- [x] Limiti di complessità (budget server-side nel query builder)
- [x] Limiti di output
- [x] Metriche (latency, p95, errori, pool)
- [x] MCP tool: `search`
- [x] Documentazione interna dei tool
- [x] Configurazione esterna TOML con profili nominati

## 🔵 Versione 0.3 — Advanced Query Layer
- [x] Aggregazioni (count, sum, avg, min, max; senza GROUP BY nella prima versione)
- [x] Filtri avanzati (range, pattern, full-text)
- [x] Policy engine avanzato
- [x] Schema awareness completa
- [x] MCP tool: `aggregate`
- [x] Supporto multi-schema

## 🔵 Versione 0.4 — Multi-Database
- [x] Backend astratto
- [x] Supporto SQLite
- [x] Supporto MySQL/MariaDB
- [x] Configurazione multi-database
- [x] Test di integrazione multi-backend

## 🔵 Versione 0.5 — Enterprise Layer
- [x] Logging strutturato
- [x] Profilazione performance
- [x] Hardening sicurezza
- [x] Policy engine dinamico
- [x] Documentazione completa
- [x] Versione 1.0 candidate

## 🔵 Versione 1.0 — Release
- [x] Stabilità API MCP
- [x] Transport MCP stdio
- [x] Transport stdio come default operativo per avvio locale/VS Code
- [x] Test end-to-end MCP (client rmcp + SQLite)
- [x] Benchmark
- [x] Hardening finale
- [x] Decision record: valutare se supportare un transport MCP HTTP remoto
- [x] Resolver configurazione project-aware (`DATAGATE_CONFIG` -> `.datagate/datagate.toml` -> `datagate.toml` -> config utente)
- [x] Connessione nominata singola per workspace (es. `name = "pippo"`) come base compatibile con futuro multi-connessione
- [x] CLI auto-configurante (`init`, `doctor`, `mcp stdio`) senza segreti nei file di progetto
- [x] Preparazione metadata e template stdio locale per MCP Registry / VS Code MCP Server Gallery
- [x] Valutazione requisiti MCP Registry, VS Code, Visual Studio e Rider / IntelliJ per server MCP locali installabili
- [ ] Packaging GitHub Release come sorgente canonica per installer e registry metadata
- [ ] Packaging Homebrew per macOS/Linux
- [ ] Packaging Scoop per Windows
- [ ] Packaging npm wrapper opzionale per client MCP che usano `npx`
- [ ] Valutazione WinGet
- [ ] Valutazione crates.io / `cargo install`
- [ ] Valutazione OCI/Docker per scenari non-IDE
- [ ] Pubblicazione MCP Registry, se compatibile con transport stdio locale e installazione cross-editor
- [ ] Pubblicazione ufficiale

> Ogni feature nuova deve essere accompagnata da test (unit e, dove applicabile,
> integrazione). Nessuna PR viene considerata completa senza copertura di test.
