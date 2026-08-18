# Rust Style Guide

Questo progetto segue le convenzioni standard della community Rust, con alcune regole
specifiche legate al ruolo di security-boundary di DataGate.

## 1) Formattazione e lint

- `cargo fmt` prima di ogni commit (config in `rustfmt.toml`).
- `cargo clippy --all-targets --all-features -- -D warnings` deve passare senza errori.
- Nessun `unwrap()`/`expect()`/`panic!` su input esterno o dati provenienti dal
  database/dall'utente: gli errori vanno propagati con `Result` (`anyhow`/`thiserror`).
- `unwrap()` è accettabile solo in test o su invarianti verificate a compile-time.

## 2) Naming

- Moduli e file: `snake_case`.
- Tipi/trait/enum: `PascalCase`.
- Funzioni/variabili: `snake_case`.
- Costanti: `SCREAMING_SNAKE_CASE`.

## 3) Error handling

- Errori di libreria/dominio: `thiserror` con varianti esplicite.
- Errori applicativi/bootstrap: `anyhow::Result`.
- Gli errori restituiti ai tool MCP devono essere **deterministici e strutturati**:
  mai stack trace, mai SQL grezzo, mai nomi di tabelle/colonne non autorizzate.

## 4) Sicurezza (regole non negoziabili)

- Nessuna concatenazione di stringhe per costruire SQL: solo query parametrizzate
  tramite il query builder controllato.
- Ogni chiamata al backend deve passare da una validazione di policy esplicita prima
  della build della query.
- Nessuna funzione deve esporre schema/introspezione completa senza filtro di policy.
- I limiti (righe, complessità, tempo) vanno applicati sempre lato server, mai fidarsi
  di limiti impostati solo lato client/tool.

## 5) Async

- Usare `tokio` per tutta l'I/O; evitare blocking calls nel runtime async (usare
  `spawn_blocking` se strettamente necessario).
- Le connessioni ai backend vanno gestite tramite pool (niente connessioni ad-hoc per
  singola richiesta).

## 6) Test

- Ogni modulo pubblico (`policy`, `query`, `schema`, `backend`, `mcp`) deve avere unit
  test per i casi principali, inclusi i casi di **rifiuto** (policy che nega, filtro
  non ammesso, limite superato).
- Test di integrazione per i backend vanno collocati in `tests/` e devono poter girare
  contro un database di test (es. via `testcontainers` o docker-compose locale).
- Nessuna nuova feature viene accettata senza test associati.

## 7) Documentazione

- Ogni tool MCP esposto deve avere un doc comment (`///`) che ne descrive input, output
  e limiti applicati.
- Aggiorna `Architecture.md` quando cambi i confini tra i layer.
- Aggiorna `ROADMAP.md` quando completi o ridefinisci un item.
