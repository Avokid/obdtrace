# Das Symptom nach dem Löschen der Fehlercodes vergleichen

Kontext: vehicle_id={{vehicle_id}}, session_id={{session_id}}, from_ms={{from_ms}}, to_ms={{to_ms}}, event_id={{event_id}}, annotation_id={{annotation_id}}.

Du als externes Modell bewertest die Daten. Diese Anwendung sammelt, filtert und speichert nur Belege. Prüfe die Fahrzeugidentität mit `get_session`. Verwende `list_sessions` mit `vehicle_id` und nutze `next_before_id` als `before_id` für alle relevanten Sitzungen einschließlich der laufenden.

Verwende `list_fault_annotations` und `get_session_events` für das frühere Symptom und die tatsächlichen Ereignisse `clear_attempt` und `clear_result`. Verwende `next_after_id` als `after_id`. Ohne Quittung ist der Ausgang des Löschvorgangs unklar. Das Verschwinden eines Codes nach dem Löschen in einer anderen Anwendung belegt hier keinen Löschvorgang.

Lies den früheren Zeitraum mit `query_session`, `session_id`, `from_ms`, `to_ms`, gewählten `pids` und `after_rowid` aus `next_after_rowid`. Lege relevante gemessene Bedingungen und Toleranzen fest, bevor du nach einem Wiederauftreten suchst.

Verwende `find_similar_conditions` mit `{"vehicle_id": VEHICLE, "from_ms": AFTER_CLEAR, "conditions": [{"pid": "rpm", "min": MIN_RPM, "max": MAX_RPM}], "bucket_ms": WIDTH_MS, "min_duration_ms": SPAN_MS, "limit": 50}`. Das ist ein Schemabeispiel, keine Empfehlung für Grenzwerte. Leite Bereiche und andere relevante PIDs aus den früheren Messungen ab. Zeitfenster sind 100 bis 60000 ms breit. Das bestimmt die zeitliche Auflösung, beweist aber keine gleichzeitige Messung. Übergib zum Fortsetzen `after_session_id` und `after_bucket` aus `next_cursor`.

Prüfe für jeden Treffer Messabdeckung und max_gap_ms je PID. Hole Originalmessungen und alle Ereignisse für den Zeitraum. Der Filter prüft nur aufgezeichnete Werte in passenden Fenstern. Ein beobachteter Zeitraum beweist nicht durchgehend vergleichbare Bedingungen. Fehlende Sensoren und wenige Werte schwächen die Aussage. Prüfe andere Zeitfenster, wenn der Beginn zwischen zwei Abfragen liegen könnte.

Unterscheide: Der Code wurde wieder ausgelesen; das Symptom tritt nach deiner Bewertung in den Messdaten erneut auf; unter vergleichbaren Bedingungen gab es keinen Hinweis auf das Symptom; oder die Daten reichen nicht aus. Nenne Messdauer, Lücken und tatsächliche Auslesestatus. Kein Treffer, eine erfolgreiche leere Auslesung nach dem Löschen, unbekannte Auslesestatus oder unvollständige Abgasmonitore beweisen keine Reparatur. Permanente Codes können länger vorhanden bleiben.

Belege deine Aussagen. Speichere bei Bedarf deinen Vorschlag mit `save_fault_annotation`, `session_id`, `from_ms`, `to_ms`, `title`, `assessment`, demselben `problem_tag` und gültigen `evidence`-Verweisen aus der Sitzung. Überschreibe keine vom Benutzer geprüfte Bewertung. Lösche die Codes nicht erneut und fordere niemanden auf, gefährliche Bedingungen nachzustellen. Interagiere nur bei stehendem Fahrzeug.
