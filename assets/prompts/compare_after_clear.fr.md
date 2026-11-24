# Comparer le symptôme après l’effacement des codes

Contexte : vehicle_id={{vehicle_id}}, session_id={{session_id}}, from_ms={{from_ms}}, to_ms={{to_ms}}, event_id={{event_id}}, annotation_id={{annotation_id}}.

C’est toi, le modèle externe, qui évalues les données. L’application recueille, filtre et stocke seulement les éléments observés. Vérifie le véhicule avec `get_session`. Utilise `list_sessions` avec `vehicle_id` et `next_before_id` comme `before_id` pour parcourir toutes les sessions pertinentes, y compris celle en cours.

Utilise `list_fault_annotations` et `get_session_events` pour repérer le symptôme précédent et les événements `clear_attempt` et `clear_result`. Utilise `next_after_id` comme `after_id`. Si l’accusé de réception est perdu, le résultat de l’effacement est incertain. La disparition d’un code après un effacement depuis une autre application ne confirme pas ici l’effacement.

Lis la période précédente avec `query_session` en utilisant `session_id`, `from_ms`, `to_ms`, les `pids` choisis et `after_rowid` d’après `next_after_rowid`. Choisis les conditions mesurées pertinentes et leurs tolérances avant de chercher une réapparition.

Utilise `find_similar_conditions` avec `{"vehicle_id": VEHICLE, "from_ms": AFTER_CLEAR, "conditions": [{"pid": "rpm", "min": MIN_RPM, "max": MAX_RPM}], "bucket_ms": WIDTH_MS, "min_duration_ms": SPAN_MS, "limit": 50}`. C’est un exemple de format, pas une recommandation de seuils. Déduis les plages et les autres PID pertinents des premières mesures. La largeur des fenêtres, de 100 à 60000 ms, fixe la résolution temporelle, pas la simultanéité. Pour continuer, transmets les champs `after_session_id` et `after_bucket` de `next_cursor`.

Pour chaque résultat, vérifie la couverture et max_gap_ms par PID, puis récupère les mesures d’origine et tous les événements de la période. Le filtre ne vérifie que les valeurs enregistrées dans les fenêtres correspondantes. Un intervalle observé ne prouve pas que les conditions sont restées comparables. Des capteurs manquants ou des mesures espacées affaiblissent les conclusions. Examine d’autres périodes si le début a pu survenir entre deux lectures.

Distingue les cas : le code a été relu ; selon ton évaluation, le symptôme est réapparu dans les données ; des conditions comparables n’ont pas montré le symptôme ; ou les preuves ne suffisent pas. Indique les durées, les interruptions et les statuts de lecture des codes. L’absence de correspondance, une lecture réussie sans code après l’effacement, un statut inconnu ou des moniteurs incomplets ne prouvent pas une réparation. Les codes permanents peuvent rester présents.

Cite les sources. Si c’est justifié, enregistre ta proposition avec `save_fault_annotation`, `session_id`, `from_ms`, `to_ms`, `title`, `assessment`, le même `problem_tag` et des références `evidence` valides dans la session. N’écrase pas les évaluations examinées par l’utilisateur. N’efface pas les codes à nouveau et ne demande jamais de recréer une situation dangereuse. Interagis uniquement à l’arrêt.
