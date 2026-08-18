# Contributing

Grazie per l'interesse a contribuire a DataGate.

## Regole generali

- Ogni operazione esposta deve passare da policy engine + query builder controllato:
  niente SQL libero, niente scritture, niente introspezione non filtrata.
- Preferisci moduli piccoli e coesi rispetto a file monolitici.
- Non introdurre dipendenze pesanti quando la stdlib o una crate già presente bastano.
- Ogni nuova feature deve essere coperta da test (unit e, se applicabile, integrazione).

## Struttura del codice

- `src/config.rs` — caricamento configurazione (profili, backend, limiti).
- `src/policy.rs` — policy engine (allow-list tabelle/colonne, limiti, complessità).
- `src/schema/` — schema loader per backend.
- `src/query/` — query builder controllato (select/search/aggregate).
- `src/backend/` — trait di backend + implementazioni per database.
- `src/mcp/` — tool MCP esposti a Copilot.
- `src/observability/` — metriche, audit log, errori strutturati.

Vedi [Architecture.md](Architecture.md) per il dettaglio dei layer.

## Branching

- `master` è l'unico branch di release; è protetto e non riceve push diretti.
- Ogni modifica (anche piccola, anche CI) va sviluppata su un branch dedicato con
  prefisso `feature/`, `fix/`, o `chore/` a seconda della natura del cambiamento:
  - `feature/<issue>-<slug>` per nuove funzionalità
  - `fix/<issue>-<slug>` per bug fix
  - `chore/<slug>` per manutenzione, CI, dipendenze
- Integrazione via Pull Request verso `master`, mai commit diretti (salvo interventi
  di emergenza sul processo di release, da evitare quando possibile).
- Prima di iniziare a lavorare, verifica di essere sul branch giusto e aggiornato con
  `github/master` (vedi regola sui remote qui sotto).

## Remote Git

Questo repository ha due remote:
- `github` → `https://github.com/afurlane/copilot-datagate.git` — **remote primario**,
  usato per pull/fetch/push, branch tracking, PR.
- `origin` → server privato di backup (gitblit) — usato **solo** per sincronizzazione
  di backup, non per il lavoro quotidiano né per le PR.

Non pushare su `origin` a meno che non sia esplicitamente richiesto un backup.

## Conventional Commits

Questo progetto segue la specifica [Conventional Commits](https://www.conventionalcommits.org/).

### Formato

```
<type>(optional scope): <description>
```

Esempio: `feat(policy): add column allow-list validation`

### Tipi ammessi

- **feat**: nuova funzionalità
- **fix**: bug fix
- **docs**: solo documentazione
- **style**: formattazione, nessuna modifica logica
- **refactor**: modifiche che non aggiungono feature né correggono bug
- **perf**: miglioramenti di performance
- **test**: aggiunta/aggiornamento test
- **build**: build system o dipendenze
- **ci**: configurazione/workflow CI
- **chore**: manutenzione (nessuna modifica al codice di produzione)
- **revert**: revert di un commit precedente

### Regole

- Tipo in minuscolo.
- Descrizione breve, imperativa.
- Scope solo quando utile (`policy`, `query`, `schema`, `mcp`, `backend`, `config`, ...).
- Un commit per ogni modifica logica; non mischiare cambi non correlati.
- Evita `BREAKING CHANGE`/`!` accidentali: se intenzionali, la PR deve avere la label
  `allow-breaking-change`.

## Pull Request

- Usa sempre il template in `.github/pull_request_template.md`.
- Prima del merge, squash dei commit di lavoro obbligatorio per mantenere una history
  pulita (`release-please` genera changelog/versioni dai commit su `master`, quindi il
  messaggio dello squash deve essere un Conventional Commit corretto).
- Ogni nuova feature deve includere test; le PR senza test per il codice nuovo non
  vengono accettate.
- I check richiesti (build, commit lint, CodeQL, SonarQube quality gate) devono essere
  verdi prima del merge.

Grazie per il tuo contributo!
