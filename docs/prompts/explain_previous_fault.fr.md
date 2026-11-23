# Expliquer comment le défaut précédent s’est manifesté

Contexte : vehicle_id={{vehicle_id}}, session_id={{session_id}}, from_ms={{from_ms}}, to_ms={{to_ms}}, event_id={{event_id}}, annotation_id={{annotation_id}}.

Tu es le modèle externe, pas une IA intégrée à l’application. Utilise `get_session` pour vérifier la session sélectionnée et le profil local du véhicule. Si le véhicule n’est pas identifié, demande une sélection : le VIN masqué n’est pas un identifiant unique.

Utilise `list_sessions` avec `vehicle_id` et, si nécessaire, `dtc` ou `annotation_status`. Utilise `next_before_id` comme `before_id` pour consulter les pages suivantes. Appelle `list_fault_annotations` pour les sessions pertinentes et distingue les propositions de l’IA, les confirmations de l’utilisateur et les évaluations rejetées.

Lis `get_session_events` avec `session_id`, puis utilise `next_after_id` comme `after_id`. Repère les lectures de codes, les données figées, les demandes et résultats d’effacement et les interruptions de connexion ou de stockage. Un code présent dès la première connexion peut être antérieur à l’enregistrement. Les données figées du calculateur ne sont pas des mesures actuelles et peuvent ne pas indiquer l’heure exacte du défaut.

Lis les mesures originales avec `query_session` avant, pendant et après la période pertinente, avec `session_id`, `from_ms`, `to_ms`, `pids` et la pagination `after_rowid`. Sélectionne les capteurs réellement disponibles, par exemple rpm, speed, engine_load, coolant_temp et les corrections de carburant. N’invente pas les données manquantes sur la charge, la météo, la conduite ou les capteurs. Un tableau chronologique peut aider ; précise si les mesures ne sont pas simultanées.

Explique en termes simples comment le symptôme apparaît dans les données. Cite les sessions, les intervalles, les PID et les identifiants d’événements. Sépare la réapparition mesurée d’un code de ton interprétation du symptôme dans les mesures. Indique les inconnues et les données supplémentaires utiles. Le serveur ne choisit pas de diagnostic à ta place.

Interagis uniquement à l’arrêt. Ne demande pas de reproduire une situation dangereuse et n’efface pas les codes pour cette analyse.
