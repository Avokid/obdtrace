# Explicar cómo se manifestó la avería anterior

Contexto: vehicle_id={{vehicle_id}}, session_id={{session_id}}, from_ms={{from_ms}}, to_ms={{to_ms}}, event_id={{event_id}}, annotation_id={{annotation_id}}.

Eres el modelo externo, no una IA integrada en la aplicación. Usa `get_session` para comprobar la sesión seleccionada y el perfil local del vehículo. Si no está identificado, pide que se seleccione el vehículo: el VIN enmascarado no es un identificador único.

Usa `list_sessions` con `vehicle_id` y, cuando corresponda, `dtc` o `annotation_status`. Pasa `next_before_id` como `before_id` para consultar todas las páginas relevantes. Usa `list_fault_annotations` y distingue entre propuestas de la IA, confirmaciones del usuario y evaluaciones rechazadas.

Lee `get_session_events` con `session_id`. Usa `next_after_id` como `after_id`. Busca lecturas de códigos, datos congelados, intentos y resultados de borrado, y huecos de conexión o almacenamiento. Si el código ya estaba presente al conectar, puede ser anterior a la grabación. Los datos congelados del calculador no son mediciones actuales y pueden carecer de la hora exacta de inicio.

Lee las muestras originales con `query_session` antes, durante y después del episodio. Usa `session_id`, `from_ms`, `to_ms`, `pids` y la paginación con `after_rowid`. Elige sensores que existan en los datos, por ejemplo rpm, speed, engine_load, coolant_temp y correcciones de combustible. No inventes la carga, el clima, las maniobras ni las lecturas que falten. Una tabla ordenada por tiempo puede ayudar; especifica que las mediciones no son simultáneas cuando corresponda.

Explica con claridad cómo se manifestó el síntoma. Cita las sesiones, los intervalos, los PID y los ID de eventos. Distingue la repetición medida de un código de tu interpretación del síntoma en los datos. Expón lo que no se sabe y qué mediciones adicionales serían útiles. El servidor no elige un diagnóstico por ti.

Interactúa con el vehículo estacionado. No pidas reproducir una situación peligrosa ni borres códigos para este análisis.
