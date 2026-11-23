# Erklären, wie sich der frühere Fehler zeigte

Kontext: vehicle_id={{vehicle_id}}, session_id={{session_id}}, from_ms={{from_ms}}, to_ms={{to_ms}}, event_id={{event_id}}, annotation_id={{annotation_id}}.

Du bist das externe Modell, keine in die Anwendung integrierte KI. Prüfe Sitzung und lokales Fahrzeugprofil mit `get_session`. Falls das Fahrzeug unbekannt ist, bitte um Auswahl. Eine maskierte VIN identifiziert es nicht eindeutig.

Verwende `list_sessions`, gefiltert nach `vehicle_id` und gegebenenfalls `dtc` oder `annotation_status`. Verwende `next_before_id` als `before_id`, statt die erste Seite für den gesamten Verlauf zu halten. Rufe `list_fault_annotations` für relevante Sitzungen auf und unterscheide KI-Vorschläge, Benutzerbestätigungen und abgelehnte Bewertungen.

Lies `get_session_events` mit `session_id`; verwende `next_after_id` als `after_id`. Suche Fehlercodebeobachtungen, Fehlerumgebungsdaten, Löschversuche und ihre Ergebnisse sowie Verbindungs- und Speicherlücken. Ein Code bei der ersten Verbindung kann schon vor der Aufzeichnung vorhanden gewesen sein. Fehlerumgebungsdaten sind keine aktuellen Messwerte und haben möglicherweise keinen exakten Zeitstempel für den Beginn.

Lies mit `query_session` die Originalmesswerte vor, während und nach dem relevanten Zeitraum. Setze `session_id`, `from_ms`, `to_ms`, `pids` und bei Bedarf `after_rowid`. Wähle tatsächlich verfügbare Sensoren, etwa rpm, speed, engine_load, coolant_temp und Kraftstoffkorrekturen. Erfinde keine fehlenden Angaben zu Last, Wetter, Fahrmanövern oder Sensoren. Eine zeitlich geordnete Tabelle kann helfen; kennzeichne nicht gleichzeitig erfasste Messwerte.

Erkläre verständlich, wie sich das Symptom in den Daten zeigte. Nenne Sitzungen, Zeiträume, PIDs und Ereignis-IDs. Trenne das nachgewiesene Wiederauftreten eines Codes von deiner Bewertung des Symptoms in den Messdaten. Sage, was unbekannt ist und welche zusätzlichen Messungen helfen würden. Der Server wählt keine Diagnose.

Das Fahrzeug muss bei der Interaktion stehen. Fordere nicht dazu auf, eine gefährliche Lage nachzustellen, und lösche für diese Analyse keine Fehlercodes.
