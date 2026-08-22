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
- **Profilazione performance** — profilo latenza in-process (min/media/p50/p95/p99/max) per monitoraggio evolutivo delle performance.
- **Backend astratto** — trait read-only comune per backend database, con implementazioni concrete per PostgreSQL, MySQL/MariaDB e SQLite.

## 🧩 Funzionalità principali

- **Query builder controllato** — operazioni semantiche come `select`, `search`, `aggregate`, con validazione automatica.
- **MCP `select` tipizzato** — contratto JSON, esecuzione read-only con righe JSON,
    policy, audit ed errori sanitizzati; il transport MCP resta lo step successivo.
- **MCP `search` tipizzato** — ricerca semantica con `ILIKE` su colonne consentite,
  query parametrizzate, policy/rate-limit/metriche/audit e output limit server-side.
- **MCP `aggregate` tipizzato** — `count`, `sum`, `avg`, `min` e `max` su colonne
    consentite, con filtri parametrizzati e senza supporto a SQL libero o `GROUP BY`.
- **API MCP versionata** — descriptor stabile per versione API e tool disponibili.
- **Transport MCP stdio** — handshake, discovery e chiamate `select`, `search` e
    `aggregate` sopra il percorso policy-safe.
- **Filtri avanzati** — supporto a pattern (`LIKE`/`ILIKE`), range (`BETWEEN`) e
    full-text (`to_tsvector` + `plainto_tsquery`) sempre parametrizzati e validati da policy.
- **Catalogo schema controllato** — introspezione policy-filtered di tabelle, viste,
    indici, vincoli (PK/FK/UNIQUE/CHECK/EXCLUSION), trigger, funzioni, procedure e sequenze;
    definizioni DDL solo quando sicure.
- **Rate limiting** — protezione contro loop del modello e query troppo pesanti.
- **Limiti di output** — risposte sempre contenute, sicure e strutturate.
- **Errori deterministici** — nessun leak di schema, nessun SQL esposto, nessun stack trace.
- **Errori strutturati** — envelope stabile con codice, messaggio generico e indicazione `retryable`.
- **Configurazione esterna** — file TOML per policy, limiti e profili nominati (dev/staging/prod).
- **Hardening input MCP** — validazione `request_id`, limiti su payload testuali e cardinalità per ridurre superfici DoS/input abuse.
- **Policy dinamica** — la policy attiva può essere sostituita atomicamente, anche ricaricandola da TOML, senza ricreare i tool MCP.

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

### Configurazione SQLite

Per usare SQLite in modalità read-only, configura una delle due varianti:

- URL completo: `SQLITE_URL`
- path file: `SQLITE_PATH`

Opzioni pool SQLite:

- `SQLITE_MAX_CONNECTIONS`
- `SQLITE_ACQUIRE_TIMEOUT_SECS`

### Configurazione MySQL/MariaDB

Per usare MySQL o MariaDB in modalità read-only, configura una delle due varianti:

- URL completo: `MYSQL_URL`
- modalità a componenti: `MYSQL_HOST`, `MYSQL_PORT` (default `3306`), `MYSQL_USER`,
  `MYSQL_PASSWORD`, `MYSQL_DATABASE`

Opzioni pool MySQL/MariaDB:

- `MYSQL_MAX_CONNECTIONS`
- `MYSQL_ACQUIRE_TIMEOUT_SECS`

### Selezione backend multi-database

Per scegliere esplicitamente quale backend attivare, imposta `DATAGATE_BACKEND`:

- `auto` (default): precedenza `postgres` -> `mysql` -> `sqlite`
- `postgres`
- `mysql` (o `mariadb`)
- `sqlite`

Quando `DATAGATE_BACKEND` è impostato a un backend specifico, DataGate richiede
la relativa configurazione ambiente; in caso contrario termina con errore esplicito.

Precedenza backend in bootstrap:

1. PostgreSQL
2. MySQL/MariaDB
3. SQLite

Il primo backend configurato nella lista viene attivato.

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

Il logging applicativo supporta due formati:

- `LOG_FORMAT=pretty` (default)
- `LOG_FORMAT=json` (strutturato, adatto a log collector)

Il livello minimo dei log può essere configurato con `LOG_LEVEL`
(`trace|debug|info|warn|error`) oppure tramite `RUST_LOG`.

Se `DB_URL` è presente ha precedenza sui componenti. Il ruolo PostgreSQL deve avere
solo permessi di lettura; inoltre ogni connessione imposta
`default_transaction_read_only = on`.

Il profilo attivo si seleziona con `profile = "dev"` e può definire la policy in
`[profiles.dev.policy]`. Se non sono presenti profili nominati, resta supportata la
forma legacy `[policy]`. Un profilo dichiarato ma inesistente attiva una policy
deny-all, senza avviare operazioni sul database.

Le release pubblicate includono binari per Linux x64/ARM64, Windows x64/ARM64 e
macOS Intel/Apple Silicon, con archivi nominati per piattaforma e file
`SHA256SUMS-*` per la verifica degli artifact.

## 🛡️ Perché DataGate?

**Gli LLM non devono parlare direttamente ai database.** Serve un layer sicuro, prevedibile, controllato, auditabile, estensibile e multi-applicazione. DataGate è questo layer.

## 🔧 Tecnologie

- Linguaggio: **Rust**
- Database: PostgreSQL, MySQL/MariaDB e SQLite (oggi), altri DB domani
- Protocollo: **MCP (Model Context Protocol)**
- Architettura: asincrona, policy-driven, schema-aware

## 🚀 Stato del progetto

DataGate è in fase di progettazione. Il repository contiene la struttura iniziale, la documentazione e la roadmap tecnica.

## 🧪 Test integrazione multi-backend

I test di integrazione backend usano variabili ambiente opzionali:

- `DATAGATE_TEST_POSTGRES_URL`
- `DATAGATE_TEST_MYSQL_URL`

Quando una variabile è presente, il relativo test verifica che il backend
read-only esegua `SELECT` controllate e rifiuti statement di scrittura.
Se la variabile non è presente, il test viene saltato senza errore.

Documentazione completa:
- [Architecture.md](Architecture.md) — design principles e architettura a layer
- [ROADMAP.md](ROADMAP.md) — roadmap dettagliata verso la 1.0
- [docs/configuration.md](docs/configuration.md) — quick start, backend, policy e operazioni
- [docs/benchmarks.md](docs/benchmarks.md) — benchmark locali SQLite e PostgreSQL reale
- [docs/hardening.md](docs/hardening.md) — checklist hardening finale per il confine MCP/database
- [docs/http-transport-decision.md](docs/http-transport-decision.md) — decisione sicurezza sul transport MCP HTTP remoto
- [docs/registry-publication.md](docs/registry-publication.md) — strategia MCP Registry, installazione cross-editor e connessioni nominate
- [docs/mcp-registry-metadata.json](docs/mcp-registry-metadata.json) — metadata candidate per registry/gallerie MCP
- [docs/release-candidate.md](docs/release-candidate.md) — criteri e gate della 1.0 candidate
- [docs/mcp-tools.md](docs/mcp-tools.md) — contratti interni e regole dei tool MCP
- [CONTRIBUTING.md](CONTRIBUTING.md) — regole di contribuzione, commit e branch
- [SECURITY.md](SECURITY.md) — policy di sicurezza e segnalazione vulnerabilità

Per l'avvio da VS Code con MCP:
- il transport locale di default è `stdio`
- i template `mcp.json` (locale supportato oggi + HTTP remoto pianificato) sono in [docs/mcp-tools.md](docs/mcp-tools.md) e [docs/configuration.md](docs/configuration.md)

## 📍 Roadmap (sintesi)

- [x] Definizione del policy engine
- [x] Implementazione del query builder controllato (select parametrizzato)
- [x] Connessione read-only garantita
- [x] Schema awareness automatica (catalogo interno PostgreSQL)
- [x] Audit log foundation (JSONL via `AUDIT_LOG_PATH`)
- [x] Tool MCP (`select`, `search`, `aggregate`)
- [x] Supporto multi-database (PostgreSQL, MySQL/MariaDB, SQLite)
- [x] Enterprise layer iniziale (osservabilità, hardening, policy dinamica)
- [x] Stabilità API MCP (descriptor versionato; transport stdio)
- [x] Transport MCP stdio
- [x] Benchmark
- [x] Hardening finale
- [ ] Versione 1.0 candidate

## 📄 Licenza

[Apache License 2.0](LICENSE)

