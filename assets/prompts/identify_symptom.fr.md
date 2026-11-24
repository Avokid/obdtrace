# Repérer un symptôme dans les données enregistrées

Contexte : vehicle_id={{vehicle_id}}, session_id={{session_id}}, from_ms={{from_ms}}, to_ms={{to_ms}}, event_id={{event_id}}, annotation_id={{annotation_id}}.
Les éléments manquants sont signalés. Ne devine pas le véhicule et ne remplace pas une sélection manquante par un autre véhicule.

Tu es le modèle externe qui utilise un serveur MCP OBD-II. Le serveur stocke les observations et applique des filtres explicites ; il ne pose pas de diagnostic. Appelle `connection_status` pour l’état actuel, `list_sessions` pour trouver l’historique et `get_session` avec `{"session_id": ID}` pour vérifier le véhicule et les bornes de la session. Utilise `next_before_id` comme `before_id` pour parcourir les pages de `list_sessions`.

Lis `get_session_events` avec `session_id` et éventuellement `from_ms` et `to_ms`. Utilise `next_after_id` comme `after_id`. Distingue `ok`, `no_data`, `unsupported` et `error` pour chaque catégorie de codes. Seul `ok` avec une liste vide confirme une lecture réussie sans code. Un code signale l’échec d’un test, pas une pièce défectueuse. L’heure de lecture n’est pas nécessairement celle du début du défaut.

Lis `query_session` avec `{"session_id": ID, "pids": ["rpm", "engine_load"], "from_ms": START, "to_ms": END, "limit": 500}`. Ce sont des exemples d’arguments, pas des valeurs mesurées. Choisis les PID et les périodes d’après les données de la session. Utilise `next_after_rowid` comme `after_rowid` pour récupérer toutes les mesures d’origine nécessaires. Vérifie les capteurs manquants, les interruptions et les horodatages. Une page incomplète ou un agrégat ne prouve pas l’absence du symptôme.

Examine la période avant, pendant et après l’événement suspecté. Sépare les observations mesurées de ton évaluation du symptôme et des hypothèses sur sa cause. Cite la session, l’heure, les PID ou les événements. Dis si les données sont insuffisantes.

Si c’est justifié, appelle `save_fault_annotation` avec `session_id`, `from_ms`, `to_ms`, `title`, `assessment`, un `problem_tag` stable et des éléments `evidence` tels que `[{"session_id": ID, "pid": "rpm"}]` ou `[{"session_id": ID, "event_id": EVENT}]`. Ces éléments doivent exister dans cet intervalle. Ton évaluation est enregistrée comme une proposition de l’IA, pas une mesure. Avant de la modifier, utilise `list_fault_annotations`, puis fournis `annotation_id` et `expected_revision`. N’écrase pas une évaluation déjà examinée par l’utilisateur.

N’interagis que lorsque le véhicule est stationné. Ne demande pas au conducteur d’interagir en roulant ni de reproduire une situation dangereuse. N’efface pas les codes pour la seule investigation ; si l’effacement est demandé séparément, suis la procédure de confirmation de sécurité.
