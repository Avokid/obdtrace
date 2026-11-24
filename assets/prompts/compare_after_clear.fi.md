# Toistuiko oire koodien nollauksen jälkeen?

Konteksti: vehicle_id={{vehicle_id}}, session_id={{session_id}}, from_ms={{from_ms}}, to_ms={{to_ms}}, event_id={{event_id}}, annotation_id={{annotation_id}}.

Sinä, ulkoinen kutsuva malli, teet arvion. Sovellus vain kerää, suodattaa ja säilyttää näytön. Varmista auto työkalulla `get_session`. Hae `list_sessions` suodattimella `vehicle_id` ja jatka `next_before_id`-arvosta argumentilla `before_id` kaikkiin olennaisiin sessioihin, myös käynnissä olevaan.

Etsi aiempi oire ja todellinen `clear_attempt`/`clear_result` työkaluilla `list_fault_annotations` ja `get_session_events`. Sivuta tapahtumia `next_after_id`-arvosta argumentilla `after_id`. Kadonnut kuittaus tarkoittaa epävarmaa nollaustulosta. Koodin katoaminen toisen sovelluksen nollauksen jälkeen ei ole täällä vahvistettu nollaustapahtuma.

Lue aiempi jakso työkalulla `query_session`: `session_id`, `from_ms`, `to_ms`, valitut `pids` ja sivutus `next_after_rowid`-arvosta argumentilla `after_rowid`. SINÄ valitset mittausten perusteella olennaiset olosuhteet ja toleranssit ennen uusiutumisen selvittämistä.

Hae `find_similar_conditions` esimerkiksi argumentein `{"vehicle_id": AUTO, "from_ms": NOLLAUKSEN_JALKEEN, "conditions": [{"pid": "rpm", "min": MIN_RPM, "max": MAX_RPM}], "bucket_ms": LEVEYS_MS, "min_duration_ms": JAKSO_MS, "limit": 50}`. Tämä kuvaa argumentteja, ei suositeltuja raja-arvoja. Johda oikeat rajat ja muut olennaiset PIDit aiemmista näytteistä. Bucket-leveys on 100-60000 ms ja kertoo aikakarkeuden, ei mittausten varmaa samanaikaisuutta. Jatka `next_cursor`-kentän argumenteilla `after_session_id` ja `after_bucket`.

Tarkista jokaisesta osumasta anturikohtainen kattavuus ja max_gap_ms sekä hae alkuperäisnäytteet ja kaikki aikavälin tapahtumat. Suodatin tarkistaa vain tallennetut arvot osuvissa aikakoreissa. Havaintojen aikajänne ei todista jatkuvasti samanlaisia olosuhteita; puuttuva anturi tai harva data heikentää näyttöä. Tutki tarvittaessa vaihtoehtoiset aikavälit, jos oire alkoi kyselyiden välissä.

Erota: vikakoodi havaittiin uudelleen; live-oire näkyy uudelleen SINUN arviosi mukaan; vastaavat olosuhteet toteutuivat ilman näyttöä oireesta; tai aineisto ei riitä. Kerro havaintojen kesto, aukot ja todelliset DTC-lukutilat. Osumien puute ei osoita korjaantumista. Nollauksen jälkeinen tyhjä onnistunut koodiluku, tuntematon lukutila tai kesken olevat readiness-monitorit eivät todista korjausta. Permanent-koodi ei välttämättä katoa heti.

Perustele lähdeviitteillä. Tallenna tarvittaessa OMA arviosi ehdotuksena työkalulla `save_fault_annotation`: `session_id`, `from_ms`, `to_ms`, `title`, `assessment`, sama `problem_tag` sekä kelvolliset kyseisen session `evidence`-viitteet. Älä kumoa käyttäjän käsittelemiä merkintöjä. Älä nollaa koodeja uudelleen tai pyydä vaarallisen tilanteen toistamista. Keskustelun aikana auto on pysäköitynä.
