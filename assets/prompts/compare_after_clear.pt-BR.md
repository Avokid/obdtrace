# Comparar o sintoma depois de apagar os códigos

Contexto: vehicle_id={{vehicle_id}}, session_id={{session_id}}, from_ms={{from_ms}}, to_ms={{to_ms}}, event_id={{event_id}}, annotation_id={{annotation_id}}.

Você, o modelo externo, faz a avaliação. O aplicativo só coleta, filtra e guarda evidências. Verifique a identidade do veículo com `get_session`. Use `list_sessions` com `vehicle_id` e passe `next_before_id` como `before_id` para percorrer todas as sessões relevantes, inclusive a ativa.

Use `list_fault_annotations` e `get_session_events` para localizar o sintoma anterior e os eventos reais `clear_attempt` e `clear_result`. Passe `next_after_id` como `after_id`. Sem o aviso de confirmação, o resultado da limpeza é incerto. Um código desaparecer após a limpeza em outro aplicativo não confirma a limpeza aqui.

Leia o intervalo anterior com `query_session`, `session_id`, `from_ms`, `to_ms`, os `pids` escolhidos e `after_rowid` obtido de `next_after_rowid`. Escolha as condições medidas relevantes e suas tolerâncias antes de procurar a recorrência.

Use `find_similar_conditions` com `{"vehicle_id": VEHICLE, "from_ms": AFTER_CLEAR, "conditions": [{"pid": "rpm", "min": MIN_RPM, "max": MAX_RPM}], "bucket_ms": WIDTH_MS, "min_duration_ms": SPAN_MS, "limit": 50}`. É um exemplo de formato, não uma recomendação de limites. Derive as faixas e outros PIDs relevantes das medições anteriores. A janela tem de 100 a 60000 ms. Isso define a resolução temporal, não a simultaneidade. Para continuar, passe os campos `after_session_id` e `after_bucket` de `next_cursor`.

Para cada correspondência, verifique a cobertura e max_gap_ms por PID e recupere as medições originais e todos os eventos do intervalo. O filtro verifica apenas os valores gravados nas janelas correspondentes. O intervalo observado não comprova condições comparáveis o tempo todo, e sensores ausentes ou leituras esparsas enfraquecem a evidência. Consulte outros intervalos se o início puder ter ocorrido entre duas leituras.

Diferencie os resultados: o código foi observado novamente; na sua avaliação, o sintoma reapareceu nos dados; condições comparáveis foram observadas sem evidência do sintoma; ou as evidências são insuficientes. Informe as durações, as lacunas e os estados reais de leitura dos códigos. A ausência de correspondência, uma leitura bem-sucedida sem códigos após a limpeza, estados desconhecidos ou monitores incompletos não provam um reparo. Códigos permanentes podem continuar presentes.

Cite as fontes. Se houver justificativa, salve a sua proposta com `save_fault_annotation`, `session_id`, `from_ms`, `to_ms`, `title`, `assessment`, o mesmo `problem_tag` e referências `evidence` válidas na sessão. Não sobrescreva avaliações revisadas pelo usuário. Não apague os códigos novamente nem peça que o motorista recrie condições perigosas. Interaja apenas com o veículo estacionado.
