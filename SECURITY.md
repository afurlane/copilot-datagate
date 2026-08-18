# Security Policy

## Principi

DataGate è un layer di sicurezza tra un LLM e una base dati reale: read-only garantito,
nessun SQL libero, nessuna scrittura, policy engine valutato prima di ogni query.
Qualsiasi bypass di questi confini è considerato una vulnerabilità critica.

## Versioni supportate

Fino alla 1.0, solo l'ultima versione rilasciata su `master` riceve fix di sicurezza.

## Segnalare una vulnerabilità

1. Non aprire una issue pubblica per vulnerabilità di sicurezza.
2. Contatta il maintainer privatamente (email del maintainer o GitHub Security
   Advisories del repository).
3. Includi:
   - Descrizione dettagliata
   - Passi per riprodurre
   - Impatto potenziale (es. bypass del policy engine, accesso a tabelle non
     autorizzate, injection nel query builder, leak di schema/dati)
   - Eventuale fix o mitigazione proposta

## Processo di gestione

- Conferma di ricezione entro 72 ore.
- Valutazione di severità e impatto.
- Preparazione di una fix o mitigazione.
- Disclosure responsabile coordinata con chi ha segnalato.

## Disclosure

Le vulnerabilità vengono divulgate pubblicamente solo dopo che una fix è disponibile.
