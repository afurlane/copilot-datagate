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
- [ ] Backend astratto
- [ ] Supporto SQLite
- [ ] Supporto MySQL/MariaDB
- [ ] Configurazione multi-database
- [ ] Test di integrazione multi-backend

## 🔵 Versione 0.5 — Enterprise Layer
- [ ] Logging strutturato
- [ ] Profilazione performance
- [ ] Hardening sicurezza
- [ ] Policy engine dinamico
- [ ] Documentazione completa
- [ ] Versione 1.0 candidate

## 🔵 Versione 1.0 — Release
- [ ] Stabilità API MCP
- [ ] Test end-to-end con Copilot
- [ ] Benchmark
- [ ] Hardening finale
- [ ] Pubblicazione ufficiale

> Ogni feature nuova deve essere accompagnata da test (unit e, dove applicabile,
> integrazione). Nessuna PR viene considerata completa senza copertura di test.
