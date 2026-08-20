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

Il loader conserva un modello interno dello schema, ma non lo espone direttamente. Ogni
risposta di introspezione è filtrata dalla policy prima della serializzazione: un client
non può usare il catalogo per scoprire tabelle o colonne non autorizzate.

## 3.1) Catalogo di introspezione controllata

I tool MCP di schema operano su categorie note e con input tipizzati. La prima versione
del catalogo copre `tables`, `views`, `indexes`, `triggers`, `functions`, `procedures`
e `sequences`.

- `list_schema_objects(kind, parent?, name_prefix?, limit, cursor?)` restituisce un
  elenco paginato e policy-filtered degli oggetti di una sola categoria.
- `describe_schema_object(kind, name, parent?)` restituisce i metadati di un oggetto
  noto e autorizzato; `parent` identifica ad esempio la tabella di un indice o trigger.
- `get_schema_definition(kind, name, parent?)` può restituire una definizione **DDL**
  normalizzata e policy-filtered quando è sicuro farlo. Non è DML: DataGate non espone
  statement che modificano dati.

La definizione DDL non è un dump del database: non include segreti, grant, ownership,
estensioni, commenti sensibili o riferimenti a oggetti non consentiti. I nomi passati
come input sono confrontati con lo schema caricato e con la policy; non vengono mai
interpolati in SQL.

## 4) Query Builder Controllato

Non SQL libero. Operazioni semantiche:
- `select(table, columns, filters, limit)`
- `search(table, text, limit)`
- `aggregate(table, op, column, filters)`

Il builder genera SQL sicuro, validato e conforme alle policy.

Per `select`, gli input sono sempre strutturati: `table`, `columns`, `filters`, `limit`
e in seguito `cursor`/ordinamento consentito. Ogni campo è validato dal catalogo/schema
e dalla policy prima che il builder produca una query parametrizzata.

### Nessuna query libera MCP

DataGate non espone un tool `query(sql)` e non prevede eccezioni basate sul prompt,
sull'identità dichiarata dal client o su un flag inviato dal modello. Una query libera
annullerebbe i confini che il gateway deve imporre.

Le necessità amministrative eccezionali devono essere gestite fuori dal protocollo MCP,
con un percorso separato e umano: accesso diretto al database tramite ruolo dedicato,
audit esplicito e autorizzazione organizzativa. Tale percorso non fa parte di DataGate e
non condivide le credenziali read-only usate dal servizio.

## 5) MCP Tools Layer
Espone operazioni a Copilot: catalogo schema controllato, lettura semantica, ricerca e
aggregazione. Ogni tool è documentato, limitato, auditato, deterministico e definito da
un input schema esplicito. I tool di lettura dati riusano il catalogo schema e non
accettano SQL, frammenti SQL o identificatori non validati.

Il primo contratto implementato è `select`: converte il payload JSON in un piano interno,
valida policy e tipi dei filtri, esegue il piano tramite il pool read-only, registra
l'esito nell'audit log e restituisce righe JSON senza il SQL. Il transport MCP resta uno
step successivo; nessun altro percorso può eseguire SQL arbitrario.

## 6) Observability Layer
Include metriche, audit log, errori strutturati, limiti applicati, tempi di risposta.
La foundation attuale scrive eventi JSONL su `AUDIT_LOG_PATH`, senza SQL, valori di
filtro o segreti. Il layer MCP dovrà registrare ogni richiesta e il relativo esito.

## 6.1) Public Error Contract

Gli errori interni non attraversano il confine MCP. Vengono convertiti in un envelope
stabile con `code`, `message` generico e `retryable`: `invalid_request`, `policy_denied`,
`backend_unavailable` oppure `internal_error`. Non vengono mai restituiti SQL, stack
trace, password, valori dei filtri o nomi di oggetti non autorizzati.

## 6.2) Rate Limiting

Il rate limiter usa finestre temporali fisse per client (`request_id`) e viene valutato
prima di policy engine, query builder e backend. `RATE_LIMIT_REQUESTS` e
`RATE_LIMIT_WINDOW_SECS` sono impostazioni server-side; valori assenti disabilitano il
limiter. Un rifiuto produce l'errore pubblico `rate_limited` e un evento audit senza
SQL né valori della richiesta.

Il query builder applica un budget di complessità server-side prima della generazione
SQL. Il costo deterministico corrente è `1 * colonne + 2 * filtri` e il massimo è
definito da `policy.max_query_complexity`; il superamento produce un rifiuto di policy.

Anche il payload JSON in uscita è limitato server-side tramite
`policy.max_output_bytes`: dopo l'esecuzione controllata, DataGate serializza la
risposta MCP e rifiuta output oltre soglia prima di restituire i dati al client.

La foundation metriche mantiene contatori in-process per richieste `select`
(totali/accettate/rifiutate), errori backend, p95 latenza e ultimo snapshot del pool
PostgreSQL (`size`, `idle`) per supportare troubleshooting e tuning.

## 7) Backend Layer
Supporta diversi database dell'applicazione: PostgreSQL (oggi), altri DB domani.
Ogni backend implementa: connessione, read-only, schema loader, query execution.

## 8) Configuration Layer
File di configurazione per policy, limiti e profili (dev/staging/prod). Le credenziali
non sono file-managed: PostgreSQL legge `DB_URL` oppure `DB_HOST`, `DB_PORT`, `DB_USER`,
`DB_PASSWORD`, `DB_NAME` e `DB_OPTIONS` dall'ambiente runtime. `DB_URL` ha precedenza
sui componenti. Il password value non viene mai scritto nei log o nei file del repo.

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
