# Specifica workflow stile CapCut – Video Editor

Documento di riferimento per agenti AI · Versione 2

## 0. Contesto e regole di lavoro

Il progetto esiste già e ha un motore valido (decodifica, accelerazione, rendering, encoding). Questo documento descrive il **nuovo workflow, interfaccia e logica di editing**, ispirati a CapCut Desktop, da costruire sopra quel motore.

**Regole per l'agente**

1. Leggi `handoff.md` e il codice esistente prima di tutto. Non compilare.
2. Il motore (decodifica, accelerazione, pipeline di rendering, encoding) **non si modifica**. Se un requisito lo richiede, fermati e segnalalo.
3. Prima di scrivere codice produci una **gap analysis**: tabella «voce di questa spec → esiste / parziale / da creare / da modificare». Attendi l'approvazione.
4. Lavora **una fase alla volta** (sezione 12) e rispetta il «Fatto quando» di ogni fase.
5. **Compatibilità dati:** lo schema del progetto deve avere un numero di versione. I progetti esistenti devono continuare ad aprirsi (migrazione automatica). Se non è possibile, segnalalo prima di procedere.
6. **Undo/redo:** ogni modifica passa dal sistema esistente. Un trascinamento continuo (slider, spostamento clip, trim) è **un'unica** azione annullabile.
7. **Preview = export:** stessa pipeline, stesso risultato.
8. Verifica subito se il toolkit UI attuale supporta pannelli ridimensionabili, spostabili e scorporabili in finestre separate (sezione 2). Se no, proponi un'alternativa prima di procedere.
9. Le voci marcate **\[placeholder\]**: UI visibile ma disabilitata, con la dicitura «Presto disponibile». Non inventare funzionalità.
10. Se la spec è ambigua o contraddice il codice esistente, segnala e chiedi: non improvvisare.
11. Lingua UI: italiano, con tutte le stringhe in un file centralizzato (predisposto per traduzioni). I termini inglesi tra parentesi (Trim, Blade, ecc.) sono riferimenti, non etichette obbligatorie.
12. Gli screenshot di riferimento (home, cruscotto, testo, sticker, transizioni, filtri) sono allegati a parte. In loro assenza, segui la descrizione testuale.

## 1. Home progetti

- Griglia di card dei progetti presenti nella cartella di default `~/Videos/Projects` (percorso configurabile).
- Ogni card mostra: anteprima (primo frame o copertina), nome, peso su disco, durata e data di ultima modifica.
- Barra di **ricerca** per nome e **filtro/ordinamento** (più recenti, nome, peso, formato).
- Menu della card: Apri, Rinomina, Duplica, Elimina, Mostra nella cartella.
- Pulsante **Nuovo progetto**: crea subito il progetto con nome automatico (`Progetto_01`, `Progetto_02`...) e apre il cruscotto, **senza chiedere nulla**. Orientamento e risoluzione vengono definiti dal primo media inserito (sezione 4).
- Stato vuoto: messaggio «Nessun progetto» e pulsante Nuovo progetto.

## 2. Cruscotto di lavoro: layout

L'interfaccia è divisa in **blocchi**: Multifunzione (libreria), Player, Proprietà, Timeline.

- Layout predefinito: in alto, da sinistra, Multifunzione, Player, Proprietà; in basso la Timeline a tutta larghezza.
- Ogni blocco è ridimensionabile (divisori trascinabili), spostabile (drag and drop della testata) e **scorporabile** in una finestra separata (per esempio su un secondo monitor), da cui può essere riagganciato.
- Il layout viene salvato e ripristinato all'apertura.

**Top bar**

- Tasto Home (casetta) per tornare ai progetti.
- Nome del progetto al centro, cliccabile per rinominarlo, con indicatore di salvataggio («Salvataggio…» / «Salvato»).
- Menu **Layout**: «Ripristina layout di default» e riapertura dei blocchi chiusi.
- Pulsante **Esporta** in alto a destra (apre il modale, sezione 9).

## 3. Timeline

Stato iniziale: una sola **Main Track** vuota, con il righello del tempo sopra. La timeline deve essere ampia, per vedere bene video e audio.

**Tracce**

- Main Track (video) al centro. Le tracce aggiuntive si creano automaticamente rilasciando una clip sopra (overlay, testi, sticker, effetti/filtri) o sotto (audio) quelle esistenti.
- Le clip si possono spostare tra tracce compatibili.

**Intestazione di traccia** (colonna fissa a sinistra, circa 120 px, non scorre con le clip)

- Icona del tipo (telecamera, nota musicale, T) e, sulla traccia centrale, l'etichetta «Main Track».
- 3 pulsanti: **Lucchetto** (blocca la traccia), **Occhio** (nasconde il livello visivo; sui sottotitoli nasconde anche il rendering nel video), **Speaker** (muta l'audio della traccia).

**Aspetto delle clip**

- Colori per tipo: video blu/teal, audio verde/turchese, testo arancione/marrone, effetti e filtri viola/indaco.
- Le clip video mostrano una striscia di miniature dei frame.
- Le clip video con audio e le tracce musicali disegnano la **waveform** (picchi del parlato, pause di silenzio): è fondamentale, perché chi monta taglia guardando i picchi.
- **Maniglie di trim** ai bordi: al passaggio del mouse compare una parentesi spessa `[` o `]` e il cursore diventa ↔.
- **Bordo bianco** sulla clip selezionata.
- Sulle clip audio, **due pallini bianchi** negli angoli superiori, trascinabili verso l'interno, disegnano la curva di fade (collegati a Fade In/Out delle proprietà).

**Selezione:** click; Ctrl+click aggiunge; trascinando sul vuoto selezione ad area. Un click sul vuoto deseleziona e mostra le Proprietà del progetto.

**Barra comandi sopra la timeline (da sinistra)**

1. **Modalità mouse** (menu a tendina con icona):
   - **Seleziona** (`A`): cursore standard per spostare, selezionare e fare trim dei bordi.
   - **Dividi / Lametta** (`B`): il cursore diventa una lama; cliccando su una clip la taglia.
   - **Seleziona verso sinistra** (`[`): cliccando una clip seleziona quella e tutte quelle alla sua sinistra.
   - **Seleziona verso destra** (`]`): cliccando una clip seleziona quella e tutte quelle alla sua destra (serve per fare spazio a metà montaggio senza spostare tutto a mano).
2. Annulla (`Ctrl+Z`) e Ripeti (`Ctrl+Y`).
3. **Dividi nel punto del playhead** (`Ctrl+B`).
4. **Elimina a sinistra del playhead** (`Q`) ed **Elimina a destra del playhead** (`W`).
5. **Cestino** (`Canc`): elimina le clip selezionate.

**Opzioni timeline** (lato destro della barra)

- **Main Track Magnet** (`P`): se attivo, niente buchi tra le clip della traccia principale (li chiude con un ripple delete); se disattivo, i buchi neri sono consentiti.
- **Auto Snapping** (`N`): magnetismo quando avvicini una clip al playhead o al bordo di un'altra clip.
- **Linkage**: collega le tracce secondarie (B-roll, testi, SFX) alla clip della traccia principale sottostante. Spostandola, tutto ciò che le sta sopra o sotto si muove con lei mantenendo il sincrono.
- **Preview Axis** (`S`): linea verticale che segue il mouse sulla timeline e permette l'hover scrubbing nel player senza spostare il playhead vero.
- **Slider di zoom**; zoom anche con `Ctrl+rotella` e con il pinch sul trackpad.

**Regole di split e trim**

- `Ctrl+B`, `Q` e `W` agiscono sulle clip selezionate. Se nessuna è selezionata, agiscono su tutte le clip **non bloccate** attraversate dal playhead.
- `Q` elimina la parte a sinistra del playhead, `W` quella a destra. **Non fanno ripple di per sé**: se il Magnet è attivo, il buco sulla traccia principale si chiude da solo.
- `B` attiva la modalità Lametta (poi si clicca sulla clip); `Ctrl+B` divide subito al playhead.

**Menu tasto destro sulla clip**

- **Freeze Frame**: fermo immagine di 3 secondi del frame sotto il playhead.
- **Reverse**: riproduce la clip al contrario.
- **Mirror**: ribalta orizzontalmente in un click.
- **Extract / Separate Audio** (`Ctrl+Shift+S`): separa l'audio mettendolo su una riga audio indipendente sotto.
- **Compound Clip** (`Alt+G`): raggruppa più livelli (video, testo, effetti) in una clip contenitore, per applicare effetti o zoom a tutto il gruppo; `Alt+Shift+G` per scompattarla.
- Copia, Taglia, Incolla, Duplica, Disabilita (`V`), Elimina.

## 4. Libreria (blocco multifunzione)

Schede: Media, Audio, Testo, Sticker, Effetti, Transizioni, Sottotitoli, Filtri.

**Regole comuni**

- In ogni scheda con categorie la prima voce è **Preferiti**. Passando il mouse su un elemento compare una **stellina** per aggiungerlo o toglierlo. I preferiti appartengono all'utente (validi in tutti i progetti).
- Per portare un elemento in timeline: trascinarlo, oppure cliccare il **+** che compare al passaggio del mouse (lo inserisce nel punto del playhead).
- Durate predefinite all'inserimento: foto 5 s, sticker 3 s, testo 3 s, effetto o filtro 3 s.

**Media** (individuali per progetto)

- Barra di ricerca in alto.
- Importazione di video, audio e foto con il pulsante Importa (file manager di sistema), trascinando i file nel blocco, o con `Ctrl+I`.
- **Il primo media inserito in timeline determina orientamento e risoluzione del progetto**: risoluzione nativa (massimo 4K), orientamento effettivo (considerando i metadati di rotazione), frame rate uguale a quello del primo video (arrotondato a 24/25/30/50/60; 30 se è una foto). Se il rapporto coincide con un preset si imposta quello, altrimenti «Original». Tutto modificabile poi dalle Proprietà del progetto.
- Stato vuoto: riquadro tratteggiato con icona `+` e la scritta «Trascina qui video, audio e foto o clicca per importare».

**Audio**

- Libreria divisa in **Musica** e **Sound effect**.
- &#91;v1\] L'utente importa i file e sceglie la categoria.
- Un click riproduce l'anteprima del suono; l'aggiunta in timeline funziona come per i media.

**Testo**

- **Aggiungi testo**: inserisce un testo predefinito al playhead.
- **Text effects**: template di testo colorati con font e animazioni. \[placeholder: vuoto per ora\]
- **Tuoi**: nelle proprietà del testo, «Salva come template» crea un template dell'utente che compare qui.

**Sticker**

- Collezione divisa in categorie in base al contenuto.
- &#91;v1\] Categorie visibili, contenuto importato dall'utente (PNG, SVG, GIF, WebP).

**Effetti** (divisi in categorie), due modi d'uso:

1. Trascinati **sopra una clip video**: si applicano a quella clip.
2. Trascinati **sulla timeline**, come elemento di una traccia effetti: elaborano tutto ciò che sta nelle tracce sotto, per la durata dell'elemento.

**Filtri** (divisi in categorie): funzionamento **identico agli effetti**.

**Transizioni**

- Divise in categorie. Trascinabili **solo tra due clip adiacenti sulla stessa traccia** (taglio contiguo, tolleranza 1 frame). Altrimenti compare il messaggio «Nessuna coppia di clip adiacenti trovata».
- Durata predefinita 0,5 s, regolabile nelle proprietà. Se le clip non hanno abbastanza frame di margine, la durata viene limitata.
- **Anteprima al passaggio del mouse** su ogni casella: due immagini di esempio incluse nell'app, su cui si applica la transizione con lo stesso shader, in loop su un piccolo canvas. Se è troppo pesante, usare un micro video in loop a bassa risoluzione (per esempio 160×90), pre-renderizzato e messo in cache.

**Sottotitoli** (3 sezioni)

1. **Genera sottotitoli**: trascrive il video con un **modello AI locale** e piazza i sottotitoli sulla timeline con il template predefinito.
2. **Template sottotitoli**: stili già pronti, divisi per categoria **word by word** e **riga**. Selezionando un template, il pulsante «Genera sottotitoli» diventa cliccabile, apre la sezione 1 e crea i sottotitoli con quel template.
   - Soluzione adottata: la trascrizione (parole con timestamp) viene salvata **separata dallo stile**. Se i sottotitoli esistono già, cambiare template (anche da word by word a riga) li **rigenera subito dal testo salvato, senza ritrascrivere**, sostituendo i vecchi.
3. **Aggiungi sottotitoli**: creazione manuale. \[placeholder\]

## 5. Player

- Anteprima del video. Sotto: **tempo corrente / durata totale**, Play/Pausa, **qualità anteprima** (Full, 1/2, 1/4 della risoluzione originale), **Fullscreen** (`Ctrl+Shift+F`) e tasto **Ratio** (`Original`, `16:9`, `9:16`, `1:1`, `4:3`).
- Il tasto Ratio e le Proprietà del progetto modificano lo **stesso valore**.
- **Manipolazione diretta:** cliccando un video, foto, sticker o testo nel player compare il **gizmo**: rettangolo bianco con 4 pallini agli angoli (scala mantenendo le proporzioni), 4 trattini sui lati (ritaglio/crop) e un pallino sopra (rotazione). Trascinando l'elemento lo si sposta.
- **Guide:** 2 linee tratteggiate cian (orizzontale e verticale) compaiono solo quando l'elemento è esattamente al centro del canvas (snap al centro).
- Ogni modifica avviene in **tempo reale** e agisce sulle stesse proprietà del pannello.

## 6. Pannello Proprietà

Il contenuto cambia in base alla selezione.

**Regole di controllo (valgono ovunque)**

- Dove comodo, **slider**. I valori numerici sono **trascinabili** (cursore con le due frecce ai bordi) e anche digitabili. Il risultato è in **tempo reale** nel player.
- Accanto a ogni controllo e nell'intestazione di ogni sezione: icona **↺ Reset** (o doppio click sul pallino dello slider) che riporta il valore al default (per esempio Scala 100%, Posizione 0, 0).
- Le proprietà animabili hanno il pulsante **◇ keyframe** (sezione 7).

**Clip video / foto / sticker**

- **Video → Basic:** Scale %, Position (X, Y), Rotate (gradi), Blend (Normal, Multiply, Screen, Overlay + Opacity %), Stabilize \[placeholder se assente\], Canvas (sfondo blur o colore se la clip non riempie il frame).
- **Video → Cutout:** Chroma Key (color picker + slider Strength e Shadow); Auto Cutout (rimozione sfondo AI della persona) \[placeholder\].
- **Video → Mask:** None, Split, Filmstrip, Circle, Rectangle, Star, Heart, con Feather e Invert.
- **Video → Enhance:** ritocco viso/corpo o upscaling base. \[placeholder\]
- **Audio:** Volume (da −∞ a +12 dB), Fade In / Fade Out (secondi), Noise Reduction (toggle AI) \[placeholder\], Voice Changer \[placeholder\].
- **Speed:** Normal (moltiplicatore 0,1×–100×, durata risultante, toggle **Keep Pitch** per evitare l'effetto scoiattolo); Curve (speed ramping con punti personalizzabili su grafico di Bezier).
- **Animation:** In (entrata), Out (uscita), Combo (continua), ognuna con slider di durata.
- **Adjustment (color grading):** LUT (import `.cube`), Basic (Temp, Hue, Saturation, Brightness, Contrast, Highlight, Shadow, Sharpen, Vignette), HSL (8 colori), Curves (curve RGB).

**Testo**

- **Text → Basic:** font, stile (B, I, U), dimensione, colore di riempimento, allineamento, Stroke (spessore), Background (rettangolo con opacità e angoli arrotondati), Shadow (distanza, sfocatura, angolo), Curve (curvatura). Pulsante **Salva come template**.
- **Text → Bubble / Effects:** cambio stile al volo.
- **Animation:** In, Out, Loop (con slider velocità).

**Transizione selezionata:** tipo e durata.

**Nulla selezionato (click sul vuoto della timeline):** Proprietà del progetto: nome, Aspect Ratio (Original, 16:9, 9:16, 1:1, 4:3), Frame Rate (24, 25, 30, 50, 60 fps), spazio colore.

## 7. Keyframe

- `Alt+Click` su una proprietà, o il pulsante ◇ accanto, aggiunge o rimuove un keyframe al playhead.
- Se esiste già un keyframe, modificare il valore al playhead lo aggiorna; altrimenti lo crea.
- Frecce ◀ ▶ per saltare al keyframe precedente o successivo.
- Sulla clip in timeline compaiono piccoli rombi nei punti dei keyframe.
- &#91;v1\] Interpolazione lineare; curve di easing in una fase successiva.

## 8. Shortcut

Gli shortcut sono disattivati quando il focus è in un campo di testo.

| Gruppo | Tasti e azioni |
| --- | --- |
| Modifica base | `Spazio` Play/Pausa · `Ctrl+B` Dividi al playhead · `Q` Elimina a sinistra · `W` Elimina a destra · `Canc`/`Backspace` Elimina selezione · `Ctrl+Z` Annulla · `Ctrl+Y` o `Ctrl+Shift+Z` Ripeti · `Ctrl+C`/`X`/`V` Copia/Taglia/Incolla · `Ctrl+D` Duplica |
| Modalità e strumenti | `A` Seleziona · `B` Lametta · `[` Seleziona a sinistra · `]` Seleziona a destra · `P` Main Track Magnet · `N` Auto Snapping · `S` Preview Axis · `M` Marker al playhead |
| Navigazione e zoom | `Ctrl++` / `Ctrl+-` o `Ctrl+rotella` Zoom · `Shift+Z` Adatta timeline alla finestra · `←`/`→` 1 frame · `Shift+←`/`→` 10 frame · `↑`/`↓` Taglio precedente/successivo · `Home`/`End` Inizio/fine · `J` Indietro · `K` Pausa · `L` Avanti (premuto più volte: 2×, 4×, 8×) |
| Clip, gruppi, audio | `Ctrl+Shift+S` Estrai audio · `Alt+G` Compound Clip · `Alt+Shift+G` Scompatta · `Ctrl+G` Raggruppa · `V` Disabilita clip · `Alt+Click` Keyframe |
| File e finestra | `Ctrl+I` Importa media · `Ctrl+E` Esporta · `Ctrl+Shift+F` Player a schermo intero |

## 9. Esportazione (modale)

Si apre con il tasto **Esporta** o con `Ctrl+E`. È un modale centrato che blocca l'editor fino alla chiusura o alla fine del rendering.

**1. Intestazione e destinazione**

- **Copertina** a sinistra: miniatura del primo frame (o del frame scelto) con il rapporto d'aspetto.
- **Name:** campo editabile, default il nome del progetto. Se il file esiste, si aggiunge un suffisso numerico.
- **Export to:** percorso di default `~/Videos/Exports`, con pulsante **Browse** che apre il file manager.

**2. Video** (checkbox; se attiva mostra i menu a tendina)

- **Resolution:** 480p, 720p, 1080p, 2K, 4K, **Match the best video** (la più alta tra le clip video in timeline). Default: la risoluzione del progetto (primo media), oppure 1080p se il progetto non ha media. Il valore indica il **lato corto** (un 9:16 a 1080p è 1080×1920).
- **Bitrate:**
  - **Match the best video** (default consigliato): legge con `ffprobe` (o dai metadati) il bitrate di ogni clip video attiva in timeline e imposta quello più alto (per esempio con clip a 12, 25 e 60 Mbps esporta a 60 Mbps). Le foto e gli elementi nascosti o disabilitati sono ignorati. Mostra il valore rilevato: `Match the best video (60.0 Mbps)`. Se non ci sono clip video, ricade su Recommended.
  - **Recommended:** calcolato da risoluzione, frame rate e codec.
  - **Lower:** circa 30% in meno rispetto a Recommended.
  - **Higher:** circa 50% in più rispetto a Recommended.
  - **Custom:** slider + campo numerico in Mbps (o Kbps), con scelta tra **CBR** e **VBR**.
- **Codec:** `H.264` (default), `HEVC (H.265)`, `AV1`. Mostrare solo quelli disponibili sul sistema; usare l'encoder hardware se il motore lo supporta.
- **Format:** `MP4` (default), `MOV`. Disabilitare le combinazioni non valide (per esempio AV1 in MOV).
- **Frame Rate:** 24, 25, 30, 50, 60 fps, oppure **Match the best video** (il frame rate più alto tra le clip video; con almeno una clip a 60 fps esporta a 60).

**3. Audio** (checkbox)

- Esporta **anche** (o **solo**, se Video è disattivato) la traccia audio come file separato.
- Format: `MP3`, `WAV`, `AAC`, `FLAC`.

**4. Sottotitoli** (visibile solo se ci sono sottotitoli in timeline)

- Checkbox **Export Captions** per un file separato. Format: `SRT` (con timestamp) o `TXT` (solo testo continuo).
- I sottotitoli visivi vengono sempre impressi nel video, a meno che la loro traccia sia nascosta con l'icona dell'occhio.

**5. Footer: stima e azioni**

- A sinistra, in tempo reale: **Duration** (per esempio `02:34`) ed **Estimated size** (per esempio `412 MB`).
- Formula: Peso stimato (MB) = (Bitrate video in Mbps + Bitrate audio in Mbps) × Durata in secondi ÷ 8. Bitrate audio predefinito: 0,192 Mbps (AAC 192 kbps). Con VBR la stima è approssimativa.
- Si aggiorna **istantaneamente** a ogni cambio di risoluzione, bitrate, codec o con Match the best video.
- A destra: **Cancel** (chiude il popup) ed **Export**.
- Durante il rendering, nella stessa finestra: barra di avanzamento (0–100%), anteprima dei frame renderizzati, tempo rimanente stimato, **Annulla** (ferma il render ed elimina il file parziale). A fine processo, pulsante per aprire la cartella di destinazione.

## 10. Salvataggio e stati vuoti

**Salvataggio automatico in tempo reale**

- Salva dopo ogni modifica confermata, con un ritardo di circa 2 secondi dall'ultima azione, e comunque almeno ogni 30 secondi se ci sono modifiche non salvate. Salva anche alla chiusura e al ritorno alla Home.
- Scrittura atomica (file temporaneo + rinomina) per evitare file corrotti in caso di crash.
- L'indicatore nella top bar mostra lo stato; al riavvio dopo un crash, recupera l'ultimo salvataggio.

**Stati vuoti** (l'interfaccia non deve sembrare rotta)

- Blocco Media vuoto: riquadro tratteggiato con `+` e «Trascina qui video, audio e foto o clicca per importare».
- Timeline vuota: scritta semitrasparente sulla traccia centrale, «Trascina qui i media per iniziare a creare».
- Home senza progetti: «Nessun progetto» e pulsante Nuovo progetto.

## 11. Funzioni AI e asset

**Funzioni AI locali**

- **Sottotitoli:** usa il modello di trascrizione locale già presente nel progetto, se c'è. Altrimenti proponilo nella gap analysis (per esempio whisper.cpp o faster-whisper). Requisiti: download del modello al primo uso, scelta della lingua (automatica o manuale), barra di avanzamento, annullabile, esecuzione fuori dal thread UI.
- Auto Cutout, Noise Reduction, Voice Changer, Enhance, Stabilize: **\[placeholder\]** salvo che il motore le abbia già.

**Asset**

- Effetti, filtri e transizioni sono definiti da cartelle con manifest JSON + shader, usando il meccanismo di shader già presente nel motore, così se ne possono aggiungere altri senza toccare il codice.
- Set iniziale minimo: circa 6 transizioni, 6 effetti e 6 filtri funzionanti, per poter testare l'intero flusso.
- Sticker, musica, sound effect e template di testo: vuoti o importati dall'utente nella v1.

## 12. Piano a fasi

Ogni fase si chiude con la verifica del «Fatto quando». Non passare alla successiva senza l'ok.

| Fase | Contenuto | Fatto quando |
| --- | --- | --- |
| 0 | Gap analysis: tabella spec → codice, rischi (toolkit UI, schema progetto) | Tabella consegnata e approvata, nessun codice scritto |
| 1 | Home progetti, nuovo progetto senza dialoghi, layout a blocchi (ridimensiona, sposta, scorpora), top bar, menu Layout, autosave, versionamento schema | Creo un progetto in un click, sposto un blocco su un secondo monitor, chiudo e riapro mantenendo layout e dati |
| 2 | Timeline: Main Track, intestazioni, aspetto clip, waveform, modalità mouse, barra comandi, Magnet, Snapping, Linkage, Preview Axis, zoom, selezione, shortcut di modifica e navigazione, menu tasto destro | Monto un video di prova solo con tastiera e mouse: split, `Q`/`W`, ripple, undo/redo, un'azione annullabile per gesto |
| 3 | Player (tempo, qualità, Ratio, fullscreen, gizmo, guide), pannello Proprietà per tipo, reset, slider in tempo reale, keyframe | Sposto e scalo una clip dal player, vedo i valori cambiare nel pannello e viceversa, con ↺ e keyframe funzionanti |
| 4 | Schede Media, Audio, Testo, Sticker, Preferiti, + al passaggio, stati vuoti, regola del primo media | Importo, inserisco con trascinamento e con +, il primo media imposta orientamento e risoluzione |
| 5 | Effetti, Filtri, Transizioni (con anteprima e regola delle clip adiacenti), Compound Clip | Applico un effetto sia su clip sia come elemento in timeline; la transizione si rifiuta tra clip non adiacenti |
| 6 | Sottotitoli (3 sezioni, trascrizione separata dallo stile) e modale di Esportazione completo | Genero sottotitoli, cambio template senza ritrascrivere, esporto con stima peso corretta e Match the best video |
| 7 | Funzioni AI (placeholder o reali), asset iniziali, rifinitura | Nessun controllo cliccabile che non faccia nulla: o funziona o è marcato «Presto disponibile» |
