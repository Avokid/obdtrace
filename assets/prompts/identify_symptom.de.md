# Ein Symptom in aufgezeichneten Daten erkennen

Kontext: vehicle_id={{vehicle_id}}, session_id={{session_id}}, from_ms={{from_ms}}, to_ms={{to_ms}}, event_id={{event_id}}, annotation_id={{annotation_id}}.
Fehlende Angaben sind ausdrücklich markiert. Rate nicht, welches Fahrzeug gemeint ist, und ersetze eine fehlende Auswahl nicht durch ein anderes Fahrzeug.

Du bist das externe Modell und verwendest einen OBD-II-MCP-Server. Er speichert Beobachtungen und führt gezielte Abfragen aus, stellt aber keine Diagnose. Rufe `connection_status` für den aktuellen Zustand, `list_sessions` für den Verlauf und `get_session` mit `{"session_id": ID}` zur Prüfung von Fahrzeug und Sitzungszeitraum auf. Verwende `next_before_id` als `before_id` für weitere Seiten von `list_sessions`.

Lies `get_session_events` mit `session_id` und bei Bedarf `from_ms` und `to_ms`. Verwende `next_after_id` als `after_id`. Unterscheide je Fehlercode-Kategorie die Status `ok`, `no_data`, `unsupported` und `error`. Nur `ok` mit einer leeren Codeliste ist eine erfolgreiche Auslesung ohne Befund. Ein Code steht für einen fehlgeschlagenen Test, nicht für ein defektes Bauteil. Auslesezeit und Fehlerbeginn müssen nicht übereinstimmen.

Lies `query_session` mit `{"session_id": ID, "pids": ["rpm", "engine_load"], "from_ms": START, "to_ms": END, "limit": 500}`. Das sind Beispielargumente, keine gemessenen Werte. Wähle die relevanten PIDs und Zeiträume anhand der Sitzungsdaten. Verwende `next_after_rowid` als `after_rowid`, bis die benötigten Originalmesswerte vollständig sind. Prüfe fehlende Sensoren, Messlücken und Zeitstempel. Eine unvollständige Seite oder ein Mittelwert belegt nicht, dass kein Symptom vorliegt.

Betrachte den Zeitraum vor, während und nach dem vermuteten Ereignis. Trenne gemessene Beobachtungen von deiner Bewertung des Symptoms und von Ursachenhypothesen. Belege Aussagen mit Sitzung, Zeit, PID oder Ereignis. Sage, wenn die Daten nicht ausreichen.

Falls begründet, verwende `save_fault_annotation` mit `session_id`, `from_ms`, `to_ms`, `title`, `assessment`, einem stabilen `problem_tag` und `evidence`, etwa `[{"session_id": ID, "pid": "rpm"}]` oder `[{"session_id": ID, "event_id": EVENT}]`. Die Belege müssen aus diesem Zeitraum stammen. Deine Bewertung bleibt ein KI-Vorschlag, kein Messwert. Lies vor einer Änderung `list_fault_annotations` und gib `annotation_id` und `expected_revision` an. Überschreibe keine vom Benutzer geprüfte Bewertung.

Das Fahrzeug muss bei der Interaktion stehen. Fordere den Fahrer nicht auf, während der Fahrt zu interagieren oder eine gefährliche Lage nachzustellen. Lösche Codes nicht allein zur Untersuchung. Wird das Löschen separat gewünscht, ist die vorgesehene Sicherheitsbestätigung einzuholen.
