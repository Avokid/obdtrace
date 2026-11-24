# Explicar como a falha anterior se manifestou

Contexto: vehicle_id={{vehicle_id}}, session_id={{session_id}}, from_ms={{from_ms}}, to_ms={{to_ms}}, event_id={{event_id}}, annotation_id={{annotation_id}}.

Você é o modelo externo, não uma IA integrada ao aplicativo. Use `get_session` para verificar a sessão selecionada e o perfil local do veículo. Se não estiver identificado, peça que o usuário selecione o veículo. O VIN mascarado não é um identificador único.

Use `list_sessions` com `vehicle_id` e, quando relevante, `dtc` ou `annotation_status`. Passe `next_before_id` como `before_id` para percorrer todas as páginas relevantes. Use `list_fault_annotations` nas sessões pertinentes e diferencie propostas da IA, confirmações do usuário e avaliações rejeitadas.

Leia `get_session_events` com `session_id` e use `next_after_id` como `after_id`. Localize leituras de códigos, dados congelados, tentativas e resultados de limpeza, além de lacunas de conexão ou gravação. Um código presente na primeira conexão pode ser anterior ao registro. Os dados congelados do módulo não são leituras atuais e podem não indicar o horário exato de início da falha.

Leia as medições originais com `query_session` antes, durante e depois do intervalo relevante. Use `session_id`, `from_ms`, `to_ms`, `pids` e a paginação com `after_rowid`. Escolha sensores que realmente apareçam nos dados, como rpm, speed, engine_load, coolant_temp e correções de combustível. Não invente informações ausentes sobre carga, clima, manobras ou sensores. Uma tabela ordenada por horário pode ajudar; indique quando as leituras não forem simultâneas.

Explique em linguagem simples como o sintoma apareceu nos dados. Cite as sessões, os intervalos, os PIDs e os IDs dos eventos. Separe a recorrência observada de um código da sua interpretação do sintoma nas medições. Diga o que não se sabe e que outros dados ajudariam. O servidor não escolhe o diagnóstico.

Interaja com o veículo estacionado. Não peça a reprodução de uma situação perigosa e não apague os códigos para esta análise.
