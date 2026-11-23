# Comparar el síntoma después de borrar los códigos

Contexto: vehicle_id={{vehicle_id}}, session_id={{session_id}}, from_ms={{from_ms}}, to_ms={{to_ms}}, event_id={{event_id}}, annotation_id={{annotation_id}}.

Tú, el modelo externo, haces la evaluación. La aplicación solo recopila, filtra y almacena pruebas. Verifica el vehículo con `get_session`. Usa `list_sessions` con `vehicle_id` y pasa `next_before_id` como `before_id` para leer todas las sesiones pertinentes, incluida la activa.

Usa `list_fault_annotations` y `get_session_events` para localizar el síntoma anterior y los eventos reales `clear_attempt` y `clear_result`. Pasa `next_after_id` como `after_id`. Sin confirmación no se conoce el resultado del borrado. Que un código desaparezca tras usar otra aplicación no confirma aquí el borrado.

Lee el intervalo anterior con `query_session`, `session_id`, `from_ms`, `to_ms`, los `pids` elegidos y `after_rowid` a partir de `next_after_rowid`. Decide qué condiciones medidas y tolerancias importan antes de buscar una repetición.

Usa `find_similar_conditions` con `{"vehicle_id": VEHICLE, "from_ms": AFTER_CLEAR, "conditions": [{"pid": "rpm", "min": MIN_RPM, "max": MAX_RPM}], "bucket_ms": WIDTH_MS, "min_duration_ms": SPAN_MS, "limit": 50}`. Es un ejemplo de formato, no una recomendación de umbrales. Deduce los rangos y demás PID pertinentes de las muestras anteriores. La anchura de cada bloque es de 100 a 60000 ms y determina la resolución temporal, no la simultaneidad. Para continuar, pasa los campos `after_session_id` y `after_bucket` de `next_cursor`.

Para cada coincidencia, comprueba la cobertura y max_gap_ms de cada PID y recupera las muestras originales y todos los eventos del intervalo. El filtro solo evalúa los valores registrados en los bloques coincidentes. El intervalo observado no demuestra que las condiciones fueran comparables en todo momento. Los sensores ausentes y las lecturas escasas debilitan las pruebas. Prueba otros intervalos si el inicio pudo quedar entre consultas.

Distingue cuatro resultados: se volvió a leer el código; a tu juicio, se repitió el síntoma en los datos; se observaron condiciones comparables sin indicios del síntoma; o no hay pruebas suficientes. Informa de la duración, los huecos y los estados de lectura de códigos. La falta de coincidencias, una lectura correcta sin códigos tras el borrado, estados desconocidos o monitores incompletos no demuestran una reparación. Los códigos permanentes no tienen por qué desaparecer inmediatamente.

Cita las fuentes. Si procede, guarda tu propuesta con `save_fault_annotation`, usando `session_id`, `from_ms`, `to_ms`, `title`, `assessment`, el mismo `problem_tag` y referencias válidas en `evidence` de esa sesión. No sobrescribas evaluaciones revisadas por el usuario. No vuelvas a borrar códigos ni pidas recrear condiciones peligrosas. Interactúa solo con el vehículo estacionado.
