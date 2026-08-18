# 📘 DataGate — Secure AI-Ready Data Gateway

[![Build](https://github.com/afurlane/copilot-datagate/actions/workflows/build.yml/badge.svg)](https://github.com/afurlane/copilot-datagate/actions/workflows/build.yml)
[![Release](https://img.shields.io/github/v/release/afurlane/copilot-datagate?include_prereleases)](https://github.com/afurlane/copilot-datagate/releases)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

> **DataGate è un gateway headless, sicuro e policy-driven che espone basi dati a strumenti AI (come GitHub Copilot) in modo controllato, prevedibile e privo di SQL libero.**

DataGate funge da **intermediario** tra un LLM e una base dati reale: non permette accesso diretto al database, non espone query arbitrarie, non consente operazioni di scrittura.
È progettato per essere **read-only**, **schema-aware**, **multi-database**, e completamente **LLM-safe**.

## 🎯 Obiettivi principali

- **Accesso sicuro ai dati** — read-only garantito tramite ruoli DB, policy engine e query builder controllato.
- **Astrazione del database** — il modello non genera SQL: DataGate costruisce query sicure e validate.
- **Multi-database** — può collegarsi a diverse basi dati di diverse applicazioni, come un DBeaver headless.
- **Policy engine** — controllo granulare su tabelle, colonne, filtri, limiti e complessità delle operazioni.
- **Schema awareness** — carica automaticamente lo schema del database e applica regole di sicurezza.
- **Concorrenza reale** — architettura asincrona e sicura, ideale per richieste parallele da Copilot.
- **Metriche e audit** — tracciamento di richieste, tempi, errori e limiti per debugging e osservabilità.

## 🧩 Funzionalità principali

- **Query builder controllato** — operazioni semantiche come `select`, `search`, `aggregate`, con validazione automatica.
- **Rate limiting** — protezione contro loop del modello e query troppo pesanti.
- **Limiti di output** — risposte sempre contenute, sicure e strutturate.
- **Errori deterministici** — nessun leak di schema, nessun SQL esposto, nessun stack trace.
- **Configurazione esterna** — file di config per DB, policy, limiti e profili (dev/staging/prod).

## 🛡️ Perché DataGate?

**Gli LLM non devono parlare direttamente ai database.** Serve un layer sicuro, prevedibile, controllato, auditabile, estensibile e multi-applicazione. DataGate è questo layer.

## 🔧 Tecnologie

- Linguaggio: **Rust**
- Database: PostgreSQL (oggi), altri DB domani
- Protocollo: **MCP (Model Context Protocol)**
- Architettura: asincrona, policy-driven, schema-aware

## 🚀 Stato del progetto

DataGate è in fase di progettazione. Il repository contiene la struttura iniziale, la documentazione e la roadmap tecnica.

Documentazione completa:
- [Architecture.md](Architecture.md) — design principles e architettura a layer
- [ROADMAP.md](ROADMAP.md) — roadmap dettagliata verso la 1.0
- [CONTRIBUTING.md](CONTRIBUTING.md) — regole di contribuzione, commit e branch
- [SECURITY.md](SECURITY.md) — policy di sicurezza e segnalazione vulnerabilità

## 📍 Roadmap (sintesi)

- [ ] Definizione del policy engine
- [ ] Implementazione del query builder controllato
- [ ] Connessione read-only garantita
- [ ] Schema awareness automatica
- [ ] Metriche + audit log
- [ ] Tool MCP iniziali
- [ ] Supporto multi-database
- [ ] Versione 0.1.0

## 📄 Licenza

[Apache License 2.0](LICENSE)

