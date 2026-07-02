# Pagina Avvisi e riepiloghi (templates/alerts.html). Usa nav-settings e
# nav-alerts-digests per gli elementi di chrome. Gli spazi separatori stanno nel
# template, quindi i valori non hanno spazi iniziali/finali. alerts-page-title
# mantiene l'entità &amp; ed è renderizzato con |safe.
alerts-page-title = Avvisi &amp; riepiloghi — Stackpit
alerts-notify-help-pre = Le notifiche vengono inviate tramite le integrazioni nella pagina
alerts-notify-help-post = .

# --- Regole di soglia ---
alerts-threshold-heading = Regole di soglia
alerts-threshold-desc = Si attiva quando un problema riceve più di N eventi in un intervallo di tempo.
alerts-rules-empty = Nessuna regola di avviso.
alerts-col-scope = Ambito
alerts-col-issue = Problema
alerts-col-threshold = Soglia
alerts-col-window = Finestra
alerts-col-cooldown = Attesa
alerts-scope-global = Globale
alerts-fingerprint-any = Qualsiasi
alerts-rule-delete-confirm = Eliminare questa regola di avviso?
alerts-delete-label = Elimina
alerts-add-rule = + Aggiungi regola di avviso
alerts-all-projects = Tutti i progetti
alerts-project-fallback = Progetto { $id }
alerts-fingerprint-label = Impronta del problema
alerts-fingerprint-hint = (vuoto = qualsiasi)
alerts-fingerprint-placeholder = qualsiasi problema
alerts-fingerprint-help = Un'impronta identifica un singolo problema (eventi raggruppati). Visibile nell'URL di qualsiasi pagina del problema. Lascia vuoto per corrispondere a ogni problema nell'ambito.
alerts-unit-s = (s)
alerts-create-rule = Crea regola

# --- Pianificazioni dei riepiloghi ---
alerts-digest-heading = Pianificazioni dei riepiloghi
alerts-digest-desc = Riepiloghi periodici dell'attività — resoconti giornalieri o settimanali invece del rumore per ogni evento.
alerts-digests-empty = Nessuna pianificazione di riepilogo.
alerts-col-interval = Intervallo
alerts-col-last-sent = Ultimo invio
alerts-col-enabled = Abilitato
alerts-never = Mai
alerts-yes = Sì
alerts-no = No
alerts-digest-delete-confirm = Eliminare questa pianificazione di riepilogo?
alerts-add-digest = + Aggiungi pianificazione di riepilogo
alerts-interval-daily = Giornaliero (24h)
alerts-interval-weekly = Settimanale (7g)
alerts-interval-hourly = Ogni ora
alerts-create-schedule = Crea pianificazione
