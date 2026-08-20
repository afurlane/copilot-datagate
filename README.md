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
- **MCP `select` tipizzato** — contratto JSON, esecuzione read-only con righe JSON,
    policy, audit ed errori sanitizzati; il transport MCP resta lo step successivo.
- **MCP `search` tipizzato** — ricerca semantica con `ILIKE` su colonne consentite,
  query parametrizzate, policy/rate-limit/metriche/audit e output limit server-side.
- **Catalogo schema controllato** — introspezione policy-filtered di tabelle, viste,
  indici, trigger, funzioni, procedure e sequenze; definizioni DDL solo quando sicure.
- **Rate limiting** — protezione contro loop del modello e query troppo pesanti.
- **Limiti di output** — risposte sempre contenute, sicure e strutturate.
- **Errori deterministici** — nessun leak di schema, nessun SQL esposto, nessun stack trace.
- **Errori strutturati** — envelope stabile con codice, messaggio generico e indicazione `retryable`.
- **Configurazione esterna** — file di config per DB, policy, limiti e profili (dev/staging/prod).

DataGate non espone una funzione MCP di query SQL libera: l'agente invia esclusivamente
parametri strutturati, validati contro schema e policy prima della costruzione della
query parametrizzata.

Gli errori pubblici usano i codici `invalid_request`, `policy_denied`,
`backend_unavailable` e `internal_error`. SQL, stack trace, password, valori dei filtri
e nomi di oggetti non autorizzati non attraversano il confine MCP.

### Configurazione PostgreSQL

Le credenziali PostgreSQL non vengono salvate nei file di configurazione o nel codice.
Il servizio legge la connessione dall'ambiente runtime:

- modalità compatta: `DB_URL`
- modalità a componenti: `DB_HOST`, `DB_PORT` (default `5432`), `DB_USER`,
  `DB_PASSWORD`, `DB_NAME`
- opzioni PostgreSQL: `DB_OPTIONS`, nel formato `key=value&key=value`
- pool: `DB_MAX_CONNECTIONS` e `DB_ACQUIRE_TIMEOUT_SECS`

Il rate limiting server-side opzionale usa `RATE_LIMIT_REQUESTS` e
`RATE_LIMIT_WINDOW_SECS`. Il limite è applicato per `request_id` prima di policy,
query builder e database; richieste oltre soglia ricevono `rate_limited` e non
generano SQL.

Il query builder applica anche un budget massimo configurabile nella policy tramite
`max_query_complexity`: ogni colonna costa 1 e ogni filtro costa 2. Le richieste oltre
budget vengono rifiutate prima di generare SQL.

La risposta `select` è soggetta anche a `max_output_bytes` nella policy: se il payload
JSON finale supera il limite, DataGate rifiuta la richiesta con errore di policy senza
esporre SQL o dettagli interni.

Il layer metriche registra anche contatori e latenza lato server per il tool `select`:
richieste totali/accettate/rifiutate, errori backend, p95 latenza in memoria e snapshot
del pool PostgreSQL (`size`, `idle`).

Se `DB_URL` è presente ha precedenza sui componenti. Il ruolo PostgreSQL deve avere
solo permessi di lettura; inoltre ogni connessione imposta
`default_transaction_read_only = on`.

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
- [docs/mcp-tools.md](docs/mcp-tools.md) — contratti interni e regole dei tool MCP
- [CONTRIBUTING.md](CONTRIBUTING.md) — regole di contribuzione, commit e branch
- [SECURITY.md](SECURITY.md) — policy di sicurezza e segnalazione vulnerabilità

## 📍 Roadmap (sintesi)

- [ ] Definizione del policy engine
- [x] Implementazione del query builder controllato (select parametrizzato)
- [ ] Connessione read-only garantita
- [x] Schema awareness automatica (catalogo interno PostgreSQL)
- [x] Audit log foundation (JSONL via `AUDIT_LOG_PATH`)
- [x] Tool MCP iniziali (`select`, `search`)
- [ ] Supporto multi-database
- [ ] Versione 0.1.0

## 📄 Licenza

[Apache License 2.0](LICENSE)

