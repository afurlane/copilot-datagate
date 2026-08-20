# Purpose
Queste istruzioni definiscono come Copilot deve generare codice e proporre modifiche in
questo repository. L'obiettivo è coerenza, correttezza e rispetto del ruolo di
security-boundary che DataGate ha tra un LLM e un database reale.

## Source of Truth
- In caso di conflitto tra documentazione e codice sorgente, il codice sorgente è
  autorevole.
- Prima di implementare una feature, verifica i pattern esistenti nei moduli
  corrispondenti (`src/policy.rs`, `src/query/`, `src/backend/`, `src/mcp/`, ecc.).
- Se rilevi drift tra istruzioni e implementazione, aggiorna questo file nello stesso
  lavoro.

## Architettura (vedi Architecture.md per il dettaglio)
- Core layer: connessioni, read-only, schema loader, policy engine, query builder,
  validazione input, limiti di complessità.
- Nessun modulo può bypassare policy engine + query builder controllato per raggiungere
  un backend.
- I tool MCP (`src/mcp/`) non parlano mai direttamente al backend: passano sempre da
  policy → query builder → backend.

## Regole di sicurezza (non negoziabili)
- **Nessun SQL libero.** Ogni query deve passare dal query builder controllato
  (`select`, `search`, `aggregate`) e da query parametrizzate.
- **Read-only garantito.** Nessuna operazione di scrittura (INSERT/UPDATE/DELETE/DDL)
  deve mai essere raggiungibile da un tool MCP o da un path di codice applicativo.
- **Policy-first.** Ogni operazione deve essere validata dal policy engine (tabelle,
  colonne, filtri, limiti di righe/complessità) *prima* di generare qualunque SQL.
- **Nessun leak di schema.** Errori e risposte non devono mai esporre stack trace, SQL
  grezzo, o nomi di tabelle/colonne non autorizzate dalla policy.
- **Determinismo.** Stessa richiesta → stessa risposta. Evitare comportamenti
  emergenti o dipendenti da stato nascosto.
- **Limiti sempre lato server.** Rate limiting, limiti di output e di complessità non
  vanno mai delegati solo al client/tool.

## Regole di codice (Rust)
- Segui [STYLEGUIDE.md](../STYLEGUIDE.md): niente `unwrap()`/`panic!` su input esterno,
  `cargo fmt` + `cargo clippy -D warnings` puliti prima di ogni commit.
- Errori di dominio con `thiserror`, errori applicativi con `anyhow`.
- I/O asincrona con `tokio`; connessioni ai backend sempre tramite pool.
- Credenziali backend solo da environment variables/runtime secret store (`DB_URL` o
  `DB_*`); mai password, URL con credenziali o segreti nei file del repository.
- Mantieni i moduli piccoli e allineati ai confini descritti in Architecture.md; non
  introdurre dipendenze pesanti se non strettamente necessarie.

## Regole sui test
- **Ogni nuova feature deve avere test.** Nessuna PR viene considerata completa senza
  copertura di test per il codice nuovo (unit test come minimo; integration test in
  `tests/` per i backend).
- Includi sempre test dei casi di rifiuto: policy che nega una tabella/colonna, filtro
  non ammesso, limite di righe/complessità superato.
- Non ridurre la copertura di test esistente senza motivazione esplicita nella PR.

## Regole di branch e commit
- `master` è l'unico branch di release ed è protetto: nessun push diretto (salvo
  emergenze sul processo di release, da evitare quando possibile).
- Ogni modifica va su un branch dedicato: `feature/<slug>`, `fix/<slug>`,
  `chore/<slug>` — anche per task di CI o manutenzione.
- Commit e titolo PR devono seguire [Conventional Commits](https://www.conventionalcommits.org/)
  (vedi `commitlint.config.mjs` e CONTRIBUTING.md). Il messaggio dello squash-merge è
  ciò che `release-please` userà per changelog/versioning: deve essere corretto.
- Evita `BREAKING CHANGE`/`!` accidentali; se intenzionali, applica la label PR
  `allow-breaking-change`.
- Prima del merge in `master`, squash dei commit obbligatorio.
- Usa sempre il template in `.github/pull_request_template.md` per le PR.

## Regole sui remote Git
- `github` (`https://github.com/afurlane/copilot-datagate.git`) è il remote
  **primario**: usalo per pull/fetch/push, branch tracking, PR.
- `origin` è un remote di **backup** su server privato (gitblit): non è il target di
  default. Usalo solo quando esplicitamente richiesto per sincronizzazione di backup.
- Non assumere che `origin` e `github` siano allineati: verifica sempre lo stato prima
  di operazioni cross-remote.

## Release
- Le versioni sono gestite da **release-please** su `master`, basato sui Conventional
  Commits. Non modificare manualmente `Cargo.toml` version o `CHANGELOG.md`: sono
  gestiti dal workflow di release.
- Il gate di qualità **SonarQube** (project key `copilot-datagate`) e i controlli CI (build,
  clippy, test, CodeQL, commitlint) devono essere verdi prima che una release parta.

## Vincoli
- Non introdurre operazioni di scrittura verso i database collegati.
- Non esporre introspezione completa dello schema senza filtro di policy.
- Non proporre riscritture architetturali non richieste esplicitamente.
- Preferisci modifiche piccole, esplicite e mantenibili.

## Goal
Copilot deve agire da assistente che rispetta l'architettura, la strategia di branch, le
convenzioni di naming e i design principle di DataGate. Tutto il codice generato deve
integrarsi in modo pulito con la struttura corrente e non deve mai indebolire il ruolo
di security-boundary del progetto.
