# Atlas: dezelfde graph, op ieder zoomniveau

Onafhankelijke wiskundige en UI-audit voor [issue #36](https://github.com/raoulg/atlas/issues/36) · 16 oktober 2026 · Codex

**Hoofdbevinding.** De zorg is terecht: vier reproduceerbare gevallen schenden het behoud van incidentie, grensverbindingen of waarde-identiteit. De structurele oorzaak is dat de viewer op verschillende zoomniveaus verschillende reconstructies maakt, zonder een expliciete projectie die aantoont dat de grove voorstelling het beeld van de fijne is. Een controle op het aantal lijnen is onvoldoende.

Dit document bevat onderzoeksbevindingen en een voorstel voor toetsbare wetten. Het is geen wijziging van de normatieve wiskundereferentie van Atlas.

## 1. Leeswijzer en bewijsgrens

De analyse begint bij `Graph`, volgt `Flow` en de semantische planning in `renderPlaat`, en eindigt bij containment, ELK, SVG en interactie. Het uitgangspunt is `docs/reference/design.md` §0–1 en `docs/reference/mathematics.md`.

| Basis | Vastlegging |
|---|---|
| Onderzoeksbranch | `codex/topology-audit-36`, eigen worktree |
| Issuebasis | `9ee8ff16a6760bf693a1a78d518b5c5f2407c0d8` |
| Geteste installatie | `atlas 0.66.1 (a4ad421)` |
| Bronverwijzingen hieronder | Vastgezet op commit `a4ad4212d6e208bca91f21f26ec65104b2e227b9` |
| Onderwerp | `/Users/rgrouls/code/PU/ruimtelijke-knowledge-graph` |
| Methode | Geëxporteerde HTML van de geïnstalleerde binary, geïsoleerde headless Chrome, meting vóór ELK |

De enginebestanden `flow.rs`, `families.rs` en `view.rs` zijn op beide bases gelijk. De viewer verschilt in prophecy-presentatie; de onderzochte plaatsings- en projectiecode is gelijk. De analyse beoordeelt deze vaste code, niet Claudes lopende wijzigingen. De broncode en live serve-sessie van het onderwerp zijn niet gewijzigd.

**Bewijslabels:** **aangetoond** betekent uitgevoerd op de echte viewer; **codefeit** betekent rechtstreeks uit de bron afleidbaar; **te toetsen** betekent een concrete bewijslast, geen bevestigde bug. Een uitgevoerde toestand zonder fout is geen bewijs voor alle mogelijke toestanden.

## 2. Wat de wiskunde precies moet beloven

### Topologie alleen beschrijft de pijlen niet

Een partitie van een vaste verzameling bepaalt een topologie: alle verenigingen van blokken zijn open. Een fijnere partitie geeft op diezelfde verzameling een fijnere topologie. Daarin staan echter nog geen richting, edge-soort, call occurrence, waardeversie of betekenis van een kader. Twee graphs met dezelfde vertices en verschillende pijlen kunnen precies dezelfde partitietopologie hebben.

Daarom is “een edge verdwijnt, dus een topologisch axioma faalt” te onbepaald. Het precieze verwijt is sterker en beter testbaar: **Atlas verbreekt zijn beloofde structuurbehoud bij refinement.** Voor richting en incidentie is een gerichte graph met afzonderlijke edge-identiteiten nodig; voor frames ook een grens- en eigendomsstructuur. De definitie van quivers als twee verzamelingen met source- en target-afbeeldingen staat bij [Riehl, *Category Theory in Context*, oefeningen 3.6.iv en 6.2.ii](https://emilyriehl.github.io/files/context.pdf). Dit auditcontract is onze toepassing daarop.

### Eén vaste semantische drager

Noem `S` de semantische structuur van één reading: call occurrences, benoemde waarden en hun momenten, plaatsen, eigen effects, bereikte effects, gerichte incidenties en grenzen. Fixeer vervolgens een filter `F`. Een zuivere zoom verandert de partitie van `S_F`, niet `S_F` zelf.

```text
                    π_fijn
       S_F ──────────────────────► P_fijn
        │                            │
        │ π_grof                     │ ρ: terugprojectie
        ▼                            ▼
       P_grof ◄──────────────────────┘

                   π_grof = ρ ∘ π_fijn
```

`P` is hier de semantische voorstelling vóór de geometrie. Het diagram moet gelden voor de identiteiten en incidenties die een voorstelling beweert te tonen. Functionele stapframes vereisen grenspoorten: een functionele pijl openvouwen is niet simpelweg een vertex in een kale quiver splitsen. Onzichtbare interne relaties blijven als bewijs in `S` aanwezig.

### De wetten vertaald naar UI-keuzes

| Wet | Wiskundige eis | Betekenis in de UI |
|---|---|---|
| W1 · vaste fiber | `S_F` verandert niet bij één zoomactie | Openen kiest geen andere routes, effecten of calls. |
| W2 · refinement | `π_grof = ρ ∘ π_fijn` | Iedere verfijnde box heeft een unieke grove vertegenwoordiger. Buiten het geopende blok is die afbeelding de identiteit. |
| W3 · incidentie | Source en target zijn compatibel met projectie | Elke getoonde grensverbinding heeft fijne getuigen met dezelfde richting en semantiek. Geen nieuwe verbinding zonder getuige. |
| W4 · grensbehoud | De interface van een blok is de projectie van zijn verfijnde grens | Wat een gesloten functie teruggeeft, gaat ook vanuit haar geopende kader naar dezelfde consument. |
| W5 · identiteit | Een typequotient werkt op de typecomponent | `(waarde-id, moment, type)` wordt `(waarde-id, moment, typeklasse)`. `a` en `b` blijven verschillende waarden. |
| W6 · eigendom | Eigendom volgt uit `S_F`, niet uit welke gebruikers zichtbaar zijn | Een wereldplaats wordt bij het openen van een package geen bezit van dat package. Een werkelijk intern tussentoestandobject mag binnen een stap staan. |
| W7 · orthogonale zooms | Op dezelfde drager geeft common refinement niet-lege doorsneden van blokken | Package- en familiezoom mogen elkaars drager of waarde-identiteit niet wijzigen. |
| W8 · saturatie | `A = q⁻¹(q(A))` | Filteren vóór groeperen. Een filter op de grove graph is alleen zonder extra informatie hetzelfde als zijn predicate op iedere volledige klasse constant is. |
| W9 · provenance | Geaggregeerde relaties bewaren hun fijne getuigen en kanalen | Eigen effects, reach, dataflow en louter calls blijven te onderscheiden, ook bij één endpointpaar. |
| W10 · layout | Geometrie realiseert de geplande incidenties | ELK, pinnen en camera kunnen geen semantische plaats of verbinding verwijderen. |

W2–W4 eisen gelijkheid van de geprojecteerde **betekenis**, niet gelijkheid van SVG-id’s, lijnaantallen of pixelposities. Twee lijnen mogen één bundel worden met behoud van beide getuigen. Bij een vertexquotient kunnen interne lijnen zelflussen worden; een expliciet looploos presentatiemodel kan deze verbergen. Een ontbrekend extern endpoint is geen intern geworden lijn.

Het universele principe achter een quotient is factorisatie door de identificatie; zie [Conrad, *Quotients by group actions*, lemma 2.10](https://math.stanford.edu/~conrad/diffgeomPage/handouts/qtmanifold.pdf). De typed, gerichte en grensbewarende voorwaarden hierboven zijn aanvullende eisen van Atlas.

## 3. Vier aangetoonde schendingen

### A · Stap openen verliest zijn resultaatverbinding

**Aangetoond · W3, W4 · gebruikersmelding bevestigd.**

Toestand: `{"route":"rkg.cli.cmd_build"}`. De gesloten call occurrence `rkg.cli.cmd_build/4` (`naslag_mappen`) heeft een draad naar `/5` (`OwBronnen.lees`). Met `open.steps = ["rkg.cli.cmd_build/4"]` ontbreekt iedere uitgaande grensverbinding van dat geopende kader naar `/5`. Hetzelfde mechanisme treedt op bij de tweede `naslag_mappen`-call, `/7`.

De viewer telt in de eerste overgang 69 en 72 edges: **meer edges, en toch verlies van de interface**. Een test op monotone aantallen accepteert dus deze fout.

Mechanisme: [`stepMorphism`](https://github.com/raoulg/atlas/blob/a4ad4212d6e208bca91f21f26ec65104b2e227b9/viewer/index.html#L1272) neemt `site.hands` en `site.gives` alleen mee onder `site && !own`. [`build`](https://github.com/raoulg/atlas/blob/a4ad4212d6e208bca91f21f26ec65104b2e227b9/viewer/index.html#L1308) bouwt bij openen de eigen pill met `own=true`. Binnenkomende calls krijgen een `backTo`, maar de resultaatinterface naar de externe consument heeft geen behouden vertegenwoordiger. De [anonymous-wire omzetting](https://github.com/raoulg/atlas/blob/a4ad4212d6e208bca91f21f26ec65104b2e227b9/viewer/index.html#L1522) kan dan de externe draad niet vormen.

**Acceptatie:** projecteer alle interne stappen en de eigen pill terug naar de gesloten occurrence; de resultaatdraad naar `/5` moet terugkomen, met zijn oorspronkelijke producer/consumer-getuige. Een lijn naar de eigen caller is geen vervanging voor een lijn naar die consument.

### B · Package in focus openen verliest “the rest”

**Aangetoond · W1, W3, W4 · onafhankelijk van A.**

Toestand: `{"focus":["rkg.variant"]}`. Gesloten zijn er twee contextverbindingen: `~in → rkg.variant` en `rkg.variant → ~out`, ieder met één type. Bij toevoegen van `open.units = ["rkg.variant"]` verdwijnen beide. De focus is ongewijzigd. Bij `rkg.cli` verdwijnt analoog de uitgaande contextverbinding van drie typen. Bij `rkg.graph` worden de verbindingen wél verdeeld over de subpackages.

Mechanisme: [`unitMorphism`](https://github.com/raoulg/atlas/blob/a4ad4212d6e208bca91f21f26ec65104b2e227b9/viewer/index.html#L1233) registreert `restLinks`. Bij de overgang naar losse routes gebruikt de viewer `morphism`, dat deze contextinterface niet registreert. De [constructie van `restArrows`](https://github.com/raoulg/atlas/blob/a4ad4212d6e208bca91f21f26ec65104b2e227b9/viewer/index.html#L1330) hangt daardoor af van het soort zichtbare kind, niet alleen van de gekozen focus.

**Acceptatie:** behoud de typenverzameling van iedere gerichte contextverbinding onder terugprojectie. Sommeer aantallen niet blind: één type dat bij twee kinderen kruist moet op het grove niveau nog steeds als één distinct type tellen.

### C · Directoryquotient maakt een ongeldig endpoint

**Aangetoond · W2, W3, W10 · onafhankelijk van A en B.**

In de overview en de rkg-packageweergave plannen pills beide richtingen met `w:File:scripts/regelschouw/`. De nodeplanner bevat dat endpoint niet. Hij bevat `w:File:scripts/`. `addEdge` slaat beide geplande relaties stil over. De geëxporteerde semantische planning bevat bovendien twee place-records met dezelfde `id: w:File:scripts/` onder verschillende Map-keys.

Mechanisme: in de [directoryomzetting](https://github.com/raoulg/atlas/blob/a4ad4212d6e208bca91f21f26ec65104b2e227b9/viewer/index.html#L1496) wordt bij het invoegen van directorynodes `places.set("w:File:" + d, filePlace(d))` gebruikt. `filePlace(d)` kan zelf een grovere id opleveren. De Map-key en record-id zijn dan verschillend. [`addEdge`](https://github.com/raoulg/atlas/blob/a4ad4212d6e208bca91f21f26ec65104b2e227b9/viewer/index.html#L1760) behandelt het ontbrekende endpoint als een reden om niets te tekenen.

**Acceptatie:** één totale, canonieke directoryprojectie voor zowel node- als edgeplanning; iedere Map-key is zijn record-id; ieder extern edge-endpoint bestaat. Een ontbrekend endpoint moet de controle laten falen, niet tot een minder verbonden plaat leiden.

### D · Typeprojectie identificeert twee benoemde waarden

**Aangetoond met minimale graph · W5 · ook zonder familiezoom.**

De synthetische graph bevat `consume(a: Record, b: Record)`. De callsite heeft twee afzonderlijke `hands` met namen `a` en `b`. De viewer plant precies één inputplaats, `t:audit.Record`, en één incidentie naar `consume`. Er zijn geen typenhiërarchie, eerdere producers of branches nodig voor deze fout.

Mechanisme: de [constructie van `held`](https://github.com/raoulg/atlas/blob/a4ad4212d6e208bca91f21f26ec65104b2e227b9/viewer/index.html#L1281) bewaart aanvankelijk `~a` en `~b`; [`standsAt`](https://github.com/raoulg/atlas/blob/a4ad4212d6e208bca91f21f26ec65104b2e227b9/viewer/index.html#L1389) reconstrueert de id als `"t:" + typeprojectie + momentOf(k)`. Het naamdeel valt weg. Ook als de typeprojectie de identiteit is, worden de waarden gelijkgesteld.

**Acceptatie:** twee waarden behouden twee identiteiten op ieder typezoomniveau. Voeg daarnaast een geval toe met twee waarden van hetzelfde type uit dezelfde producer, en met een subtype en een basetype. Een gedeeld label of type is geen bewijs van aliasing.

## 4. Van graph naar scherm: inventaris van keuzes

De matrix benoemt de plekken waar een controle hoort. “Te toetsen” is geen geconstateerde schending.

| Laag / keuze | Wat mag veranderen? | Wat moet gelijk blijven? | Stand |
|---|---|---|---|
| `Graph → Flow`: calls naar occurrences | Representatie van calls en wires | Callsite, volgorde, dispatch, handed/given value | Enginefeiten onderzocht; geen parserfout nodig voor A–D. |
| Librarygrens, `view::reached` | Interne librarycode aggregeren | Effecten en interface op de grens | Te toetsen: grens verschuiven bij ander `--libraries` is modelverandering, geen zuivere UI-zoom. |
| Route/kind/part kiezen | De fiber zelf | Herkomst van wat overblijft | Edgeverlies door een ander filter kan geldig zijn. |
| Focus plus `~in`, `~out` | Context groeperen | Grensrelaties naar dezelfde focus | B aangetoond. Dit is een focus mét contextquotient, geen zuivere induced-subgraph fiber. |
| Part openen | Part naar packages | HTTP- en wereldinterface | Te toetsen: `partMorphism` en unitmorphisms bepalen de interface verschillend. |
| Package naar subpackages | Eén codeblok splitsen | Geprojecteerde grens en externe plaatsen | Werkt voor de contextverbindingen van `rkg.graph`; geen algemene garantie. |
| Package naar routes/doors | Interne code tonen | Dezelfde semantische drager | B; daarnaast codefeit: gesloten ziet eigen touches van alle nodes, geopend transitive reach van gekozen routes. Dit vraagt één expliciete grensconventie. |
| Stap naar body | Interne calls zichtbaar maken | Callsite-input, resultaat en externe effects | A aangetoond. |
| Recursie afkappen, `deeper.has` | Oneindig ontvouwen begrenzen | Grensinterface van de recursieve call | Te toetsen: `routeTree` laat zo’n child geheel weg; een grensgetuige is nodig. |
| Choice/dispatch | Branches als alternatieven tonen | Branchidentiteit en voorwaarden | Te toetsen: geen cross-branch waarde-aliasing. |
| Commandlineparse vouwen | Parse intern maken aan entry | Typed input- en outputverbindingen | Te toetsen op dezelfde grenswet als A. |
| Type naar base/family | Typecomponent groeperen | Waarde-id, moment, rol | D aangetoond; er zijn ook uitgevoerde familie-openingen zonder ontbrekende endpoints. |
| Compositie en gedeelde velden | Parts tonen, gedeelde waarden als references | Elke waarde één identiteit | Te toetsen: een reference is geen tweede eigendom. Overlappende relaties vormen niet automatisch een forest. |
| `latest`, `choiceOf`, versioning | Momenten van dezelfde waarde tonen | Aliasing en produceridentiteit | Te toetsen: momenten worden uit zichtbare pills afgeleid, terwijl stapzoom die pills verandert. |
| Opslagquotient, `storage` | Concrete/symbolische plaatsen groeperen | Leden en gerichte getuigen | Te toetsen: groepsleden komen uit `usedWorld`, dus hangen af van zichtbare pills. |
| Files naar directories | Paden onder een directory samennemen | Totale, unieke endpointprojectie | C aangetoond. |
| Grens van zes files | Standaard-detailniveau kiezen | Bij één zoomactie geen ongevraagde coarsening elders | Te toetsen: toevoegen van een zichtbaar pad kan andere directories dichtvouwen. |
| Sibling-pills met gelijke D/C | Parallelle representanten bundelen | Occurrences, volgorde, eigen/reach-kanalen | Codefeit: de sleutel hangt van zichtbare D/C af; te toetsen onder andere quotients. |
| Anonieme `v:#` als wire | Waardevertex door een verbinding vervangen | Producer-consumerincidences en reading | A raakt deze stap. Definieer deze contractie expliciet. |
| `roleOf`: in/out/mid/both | Plaatsingsprojectie volgens routebetekenis | Semantische rol bij vaste fiber | Te toetsen: `roles.used` hangt van zichtbare C en `backTo` af. |
| Twins “from/into” | Twee projecties van één wereldplaats tonen | Gemeenschappelijke semantische identiteit | Geldige presentatie als beide dezelfde plaats blijven vertegenwoordigen; twee id’s zijn geen twee plaatsen. |
| `homeOf`: gemeenschappelijke ancestor | Frame voor werkelijk interne waarden kiezen | Eigenaar van externe plaatsen | Codefeit: codeframes worden voor wereldplaatsen uitgesloten. Dat is een goede lokale safeguard, geen bewijs voor alle refinements. |
| `addEdge`, aggregatie | Lijnen bundelen | Eindpunten, betekenis en provenance | C; sleutel is alleen `a>b`, reach wordt met AND samengevoegd. Bewaar kanaalgetuigen apart. |
| Dotted package calls, `joinedIn` | Pure call-relatie anders tekenen dan dataflow | Relatie blijft aantoonbaar | Te toetsen: een dataverbinding die bij een ander quotient ontstaat mag een call-getuige niet wissen. |
| Diff A/B/union | Reading kiezen en overlay tekenen | Elke reading behoudt eigen getuigen | Te toetsen: gemengde edge-kanalen en roles mogen geen betekenis van één reading uit de andere afleiden. |
| ELK → SVG | Coördinaten, routes, afmetingen | Alle geplande endpoints en incidenties | A–D bestaan al vóór ELK; browsercontrole blijft nodig voor de realisatie. |
| Hover, pin, camera | Belichting en positie | Semantische graph | Pinnen beïnvloedt zichtbaarheid/contrast; toets apart van werkelijk edgeverlies. |
| Serve refresh | Nieuwe reading laden bij bronwijziging | Iedere snapshot intern consistent | Een andere reading mag andere edges hebben; vergelijk voor refinement altijd dezelfde snapshot. |

## 5. Wiskundige aanscherpingen voor de specificatie

### Gluing vereist een gekozen categorie

Een pushout in verzamelingen bewaart niet vanzelf gerichte incidentie. Een pushout in quivers houdt edges en hun endpoints bij. Een looploos geaggregeerde weergave, zoals [`Quotient::apply`](https://github.com/raoulg/atlas/blob/a4ad4212d6e208bca91f21f26ec65104b2e227b9/engine/src/view.rs#L51), verbergt interne edges en behoudt gewichten per edge-soort. Dat is een bruikbare presentatie, maar niet zonder nadere conventie de volledige quiver-coequalizer. Ook `None` in de quotientfunctie is filtering, niet identificatie. Benoem beide operaties afzonderlijk in het contract.

### Een fiber openen vereist zijn grens

De induced subgraph op één quotientklasse bevat alleen interne edges. De pijlen die de klasse verbinden met de omgeving zitten daar juist niet in. “Opening puts its fiber in its place” is daarom onvoldoende als implementatierecept: neem de interne structuur **met haar interface naar het complement** en lijm die terug aan dezelfde externe endpoints. A en B tonen precies het verlies van die interface.

### Saturatie geldt voor de relevante structuur

Voor vertices betekent saturatie `A = q⁻¹(q(A))`; een predicate daalt precies dan af naar quotientklassen. Voor edge-filters moet ook het edge-predicate constant zijn op de gekozen edge-aggregatie. Een nodefilter dat verzadigd is, garandeert niet dat een onafhankelijk call-/effectfilter door een endpoint-only edgebundel heen kan worden toegepast. Atlas moet de “filter eerst”-regel op de canonieke feiten uitvoeren, niet telkens opnieuw op zichtbare samenvattingen.

### Forestwetten gelden alleen voor de gekozen forest

De ancestor-gesloten openingen van één forest vormen een distributief rooster. Het rooster van alle partities is in het algemeen niet distributief. Meervoudige overerving, gedeelde compositie en verschillende hiërarchieën vragen dus een expliciete keuze: een canoniek forest voor die zoomas, references voor gedeeld eigendom en een common-refinementconstructie tussen assen. “De code heeft een hiërarchie” is nog geen bewijs dat iedere huidige groepering zo’n forest volgt.

### Coarse reach kan fictieve routes suggereren

Neem `a → b` en `c → d`, zonder verbinding van `b` naar `c`. Identificeer `b` en `c`. De quotientgraph heeft een pad van `[a]` naar `[d]` dat geen pad in de fijne graph vertegenwoordigt. W3 bewaart individuele incidenties, maar geeft geen lifting van ieder coarse pad. Bereken semantische reach daarom op de fijne graph; laat een quotientoefening geen nieuw bewijs van uitvoerbare call order worden. Provenance maakt het verschil toetsbaar.

## 6. Een uitvoerbare controle voor Claude

### Eerst één canonieke semantische planning

Laat de reading de occurrences, waarden, effects, rollen en eigendom vastleggen. Laat de view daarna uitsluitend filters en expliciete representantafbeeldingen bepalen. Dit hoeft geen grote herschrijving te zijn: de eerste bruikbare stap is een inspecteerbaar plan vóór ELK met voor iedere box en verbinding de fijne feiten die hij vertegenwoordigt, plus framegrens en semantische id.

Een onbekend endpoint is dan een mislukte invariant. Het kan niet stil als een gewone tekensituatie worden afgehandeld. Een pin of een SVG-label is geen semantische identiteit.

### De oracle

Voor iedere legale opening, met dezelfde reading en filters:

```text
coarse = plan(reading, fiber, ideal)
fine   = plan(reading, fiber, ideal + opening)
rho    = refinement_map(fine, coarse)

assert same_semantic_support(coarse, fine)
assert total_and_unique(rho)
assert unique_record_ids(fine)
assert all_endpoints_exist(fine)
assert project_boundary(fine, rho) == boundary(coarse)
assert same_value_ids_and_moments(coarse, fine, rho)
assert unchanged_external_owners(coarse, fine, rho)
assert same_channel_witnesses(coarse, fine, rho)
```

Normaliseer hierbij alleen expliciet toegestane interne verberging, bundeling en wirecontractie. Vergelijk provenanceverzamelingen per richting/kanaal; geen kale edge-aantallen. Test ook sluiten, twee onafhankelijke openingen in beide volgorden, en openen–sluiten–openen. Als synthetische functies worden samengevouwen, moeten hun getuigen nog te vergelijken zijn.

| Testfamilie | Clean kant | Shortcut / stresskant |
|---|---|---|
| Resultaatinterface | Benoemde typed return | Ongenoemde `list[Path]` rechtstreeks in volgende call |
| Waarde-identiteit | `a`, `b` expliciet van hetzelfde type | Twee naamloze resultaten en aliasing; alleen echte aliases mogen samenvallen |
| Focusgrens | Package met subpackages | Leafpackage dat direct routes opent |
| Directoryquotient | Enkele expliciete bestanden | Parentdirectory, childdirectory én bestanden, boven de vouwgrens |
| Wereld-eigendom | Externe input en output | Intern cache-object, read-back, update en een andere externe gebruiker |
| Sums en recursie | Twee benoemde takresultaten | Hetzelfde type in beide takken, recursieve call op de grens |
| Zoomassen | Package × typefamilie | Shared member, subtype en twee momenten tegelijk |
| Edgekanalen | Eén direct effect | Zelfde endpointpaar met direct én reach, en afzonderlijke diff-readings |

Deze gevallen horen in de pairs van de ondersteunde talen, met readerfeiten in Rust-tests en de gezamenlijke semantische vieweroracle erboven. Atlas’ principes controleren het onderwerp; deze oracle controleert de eerlijkheid van Atlas’ voorstelling. Dat zijn twee verschillende verantwoordelijkheden.

De huidige [`pictures.py walk`](https://github.com/raoulg/atlas/blob/a4ad4212d6e208bca91f21f26ec65104b2e227b9/tools/pictures.py#L115) test crashes, lege tekeningen en aantallen familiemomenten. Die controles vangen A–D niet algemeen af. Voeg de oracle toe aan de gelopen transities, en houd de bestaande schermcontrole voor ontbrekende SVG-realisaties. Een full `cargo test` op ongewijzigde productiecode bewijst deze UI-wetten niet; er is voor deze audit geen volledige Rust-suite gedraaid.

## 7. Wat geen schending hoeft te zijn

- Een ander route-, kind-, part- of readingfilter kan relaties verwijderen. Leg dan de filterwijziging vast; noem de actie geen zuivere zoom.
- Interne lijnen kunnen bij coarsening verborgen raken. Hun getuigen blijven beschikbaar; externe relaties mogen niet zonder beeld raken.
- Coördinaten en bochten mogen veranderen. Binnen/buiten een frame is pas een semantische schending als het frame eigendom of groepering belooft die met die verandering onjuist wordt. Een topologie op alleen vertices bepaalt geen schermrechthoek.
- Twee projections “from X” en “into X” kunnen één wereldplaats tonen. Dat is geldig met één onderliggende identiteit en correcte gerichte incidenties.
- Een frame mag een eigen grenspoort gebruiken. Het hoeft geen extra functie of effect te betekenen als de poort een expliciete vertegenwoordiger is.
- Een extra fijne lijn kan een reeds geaggregeerde relatie zichtbaar maken. Nieuw zichtbaar is niet hetzelfde als een nieuw semantisch feit.

## 8. Overdracht en reproduceren

Het pakket bevat `probe.py`, `states.json`, `synthetic.json` en `evidence.json`. De probe leest een export, instrumenteert alleen zijn tijdelijke kopie, neemt het plan vlak vóór ELK op, en logt ontbrekende endpoints vóór de stille return. De eerste tekening gebruikt de volledige viewer; de volgende planningen slaan ELK over. Dit is een semantische probe, geen volledige visuele regressiesuite. Hij is gekoppeld aan de herkenbare functies van 0.66.1 en stopt wanneer de injectiepunten niet meer uniek zijn.

```sh
atlas view /Users/rgrouls/code/PU/ruimtelijke-knowledge-graph \
  --no-open --no-cache --out /private/tmp/atlas-audit-rkg.html

uv run --offline --with playwright probe.py \
  /private/tmp/atlas-audit-rkg.html states.json /private/tmp/atlas-audit-rkg

uv run --offline --with playwright probe.py \
  /private/tmp/atlas-audit-rkg.html states-synthetic.json \
  /private/tmp/atlas-audit-synthetic --graph synthetic.json
```

Voer `uv` uit vanuit deze eigen worktree of een scratchmap, niet vanuit het onderwerp. De uitgebreide verkenning bevat 58 semantische toestanden; de meegeleverde kernset is kleiner en gericht op A–D. De ruwe subjectgraph blijft buiten dit pakket. `evidence.json` bevat alleen de relevante diagnostiek.

De eerste onderzoeksvraag voor de implementatie is: **welke fijne feiten en grenspoorten vertegenwoordigt iedere grove verbinding?** Als dat voor elke view expliciet is, zijn behoud van edges, correcte containment en identiteit gevolgen van dezelfde controleerbare constructie.
