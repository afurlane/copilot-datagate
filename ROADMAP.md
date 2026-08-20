# 🗺️ Roadmap

## 🔵 Versione 0.1 — Foundation
- [x] Struttura del progetto Rust
- [x] Connessione PostgreSQL read-only
- [x] Schema loader
- [x] Policy engine (base)
- [x] Query builder controllato (select + filters + limit)
- [ ] MCP tool: `select`
- [ ] Audit log
- [ ] Errori strutturati
- [ ] README completo

## 🔵 Versione 0.2 — Safety & Observability
- [ ] Rate limiting
- [ ] Limiti di complessità
- [ ] Limiti di output
- [ ] Metriche (latency, p95, errori, pool)
- [ ] MCP tool: `search`
- [ ] Documentazione interna dei tool
- [ ] Configurazione esterna (TOML/YAML)

## 🔵 Versione 0.3 — Advanced Query Layer
- [ ] Aggregazioni (count, sum, avg, min, max)
- [ ] Filtri avanzati (range, pattern, full-text)
- [ ] Policy engine avanzato
- [ ] Schema awareness completa
- [ ] MCP tool: `aggregate`
- [ ] Supporto multi-schema

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
