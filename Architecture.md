# 🧭 Design Principles

## 1) Security First
DataGate è progettato come un *confine di sicurezza* tra un LLM e una base dati reale.
Ogni operazione è validata, limitata, controllata, auditata.
Nessun SQL libero, nessuna scrittura, nessun accesso non autorizzato.

## 2) LLM-Safe by Design
Il modello non genera query SQL. DataGate interpreta richieste semantiche e costruisce
query sicure, prevedibili e deterministiche.

## 3) Database Abstraction
DataGate non espone lo schema reale del database. Espone un layer di operazioni
controllate (select, search, aggregate, ecc.). Il backend può cambiare senza rompere
l'interfaccia.

## 4) Policy-Driven
Ogni operazione è governata da un file di policy: tabelle accessibili, colonne
consentite, limiti di righe, limiti di complessità, filtri ammessi. Le policy possono
essere modificate senza ricompilare.

## 5) Schema Awareness
DataGate carica automaticamente lo schema del database e applica regole di sicurezza
basate su tipi, vincoli, colonne, indici, relazioni.

## 6) Deterministic Behavior
Stessa richiesta → stessa risposta. Nessun comportamento emergente, nessun side-effect,
nessuna variabilità.

## 7) Minimal Exposure
DataGate espone solo ciò che è necessario: nessuna introspezione del DB, nessun dump,
nessuna enumerazione completa, nessuna informazione sensibile.

## 8) Observability
Ogni operazione è tracciata: metriche, audit log, errori strutturati, limiti applicati.

## 9) Concurrency & Performance
Architettura asincrona, sicura, con pool di connessioni e latenza stabile. Progettato
per richieste parallele da Copilot.

## 10) Extensibility
Nuovi tool MCP, nuovi backend, nuove policy e nuove operazioni possono essere aggiunti
senza modificare il core.

---

# 🏗️ Architecture

## 1) Core Layer
Il cuore del sistema: gestione connessioni, read-only garantito, schema loader, policy
engine, query builder controllato, validazione input, limiti di complessità. È il layer
che garantisce sicurezza e coerenza.

## 2) Policy Engine
File YAML/TOML che definisce: tabelle accessibili, colonne consentite, filtri ammessi,
limiti di righe, limiti di tempo, limiti di complessità, operazioni disponibili.
Il policy engine è valutato *prima* di generare SQL.

## 3) Schema Loader
Carica automaticamente tabelle, colonne, tipi, vincoli, indici. Serve per validare
richieste e prevenire query non sicure.

## 4) Query Builder Controllato
Non SQL libero. Operazioni semantiche:
- `select(table, columns, filters, limit)`
- `search(table, text, limit)`
- `aggregate(table, op, column, filters)`

Il builder genera SQL sicuro, validato e conforme alle policy.

## 5) MCP Tools Layer
Espone operazioni a Copilot: tool semantici, tool di introspezione controllata, tool di
ricerca, tool di aggregazione. Ogni tool è documentato, limitato, auditato,
deterministico.

## 6) Observability Layer
Include metriche, audit log, errori strutturati, limiti applicati, tempi di risposta.

## 7) Backend Layer
Supporta diversi database dell'applicazione: PostgreSQL (oggi), altri DB domani.
Ogni backend implementa: connessione, read-only, schema loader, query execution.

## 8) Configuration Layer
File di configurazione per: credenziali, policy, limiti, backend, profili
(dev/staging/prod).

## Module Layout (Rust crate)

```
src/
  main.rs        entrypoint, wiring
  config.rs      configuration loading (profiles, backends, limits)
  policy.rs      policy engine (allow-lists, limits, complexity rules)
  schema/        schema loader per backend
  query/         controlled query builder (select/search/aggregate)
  backend/       backend trait + per-database implementations (postgres, ...)
  mcp/           MCP tools layer (tool registration, request/response mapping)
  observability/ metrics, audit log, structured errors
```

Ogni nuovo modulo deve rispettare i confini sopra: i tool MCP non parlano mai
direttamente al backend, passano sempre da policy engine + query builder.
