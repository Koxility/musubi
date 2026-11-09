# Architektur

## Aufbau

```
                 musubi (Binary: clap, Logging, Start)
                              │
                         musubi-gui
                     (Iced, Elm-Muster)
                              │
                         musubi-core
        Fassade · Matching · Sync-Regeln · Outbox · Wiedergabe
          │             │              │             │
   musubi-jellyfin  musubi-anilist  musubi-player  musubi-store
    REST-Client     GraphQL, OAuth,  PlayerBackend  Config, Secrets,
                    Rate-Limiter     mpv-IPC        SQLite
          └─────────────┴──────┬───────┴─────────────┘
                         musubi-domain
                     reine Typen, kein IO
```

Die Abhängigkeiten zeigen nur nach unten, und der Compiler erzwingt das über die Crate-Grenzen. Die GUI hängt nur am Kern und kennt weder HTTP noch mpv. Im Workspace gilt `unsafe_code = "forbid"`.

### Vertrag zwischen Kern und GUI

`Core` ist billig klonbar und bietet typisierte async-Methoden, dazu `subscribe()` für Ereignisse wie Wiedergabefortschritt, geänderte Listen oder Rate-Limits. Die GUI spricht ausschließlich mit dieser Schnittstelle. Funktionen entstehen dadurch einmal im Kern und nicht pro Oberfläche.

## Technologie-Entscheidungen

### GUI: Iced

Die GUI ist reiner Rust-Code ohne Systembibliotheken wie GTK und sieht auf jedem Desktop gleich aus. Icons sind eigene SVGs in `icons.rs`, gezeichnet von resvg. Jede Seite bringt Zustand, Nachrichten und Ansicht mit, `app.rs` verbindet sie mit dem Kern. Poster werden erst geladen, wenn sie sichtbar werden. Die Cache-Dateien heißen `*.img`, deshalb liest die GUI sie als Bytes: Bei Pfaden wählt Iced den Decoder nach der Dateiendung. Der Preis: Iced hat keine virtualisierten Raster, die Bibliothek baut also für jeden Titel Widgets.

### Player: GStreamer im Fenster, mpv als Alternative

Standard ist `iced_video_player` mit GStreamer: Das Bild läuft nativ in der Iced-Oberfläche, die Steuerung kann darüber liegen, und das Projekt bleibt ohne eigenen `unsafe`-Code. Untertitel rendert `assrender` ins Bild. `EmbeddedPlayer` erfüllt dasselbe `PlayerBackend` wie mpv, Fortschritt und Sync im Kern merken keinen Unterschied. Der Stream öffnet abseits des UI-Threads, GStreamer-Objekte leben danach auf dem UI-Thread. Fehlen Plugins, fällt die App auf mpv zurück.

mpv läuft als eigener Prozess über JSON-IPC:

Hardware-Decoding, ASS-Untertitel und Wayland/X11 kommen fertig von mpv, und deine `mpv.conf` gilt weiter. Ein Absturz des Players reißt die App nicht mit, es gibt kein FFI und GUI und TUI funktionieren gleich. Der Preis ist das eigene Fenster. Der Trait `PlayerBackend` ist der Austauschpunkt für eine spätere Einbettung.

### Persistenz: SQLite auf eigenem Thread

Zuordnungen und Warteschlange müssen Abstürze überleben. rusqlite ist synchron, deshalb gehört die Verbindung einem eigenen Thread, und async-Code schickt ihm Aufträge.

### GraphQL: handgeschriebene Queries

Rund ein Dutzend Operationen rechtfertigen keinen Codegen-Schritt samt eingechecktem Schema.

## Abläufe

### Wiedergabe

1. `PlaybackInfo` liefert Direct Stream oder Transcoding-URL. Das Geräteprofil erlaubt alles, weil mpv praktisch jedes Format abspielt.
2. mpv startet ohne Zugangsdaten. Der Auth-Header wird danach per IPC gesetzt, das Token erscheint also nie in `argv` oder `ps`.
3. Fortschritt geht alle 10 s sowie bei Pause und Seek an Jellyfin. mpv bekommt die Folgeepisode, sobald die aktuelle läuft, damit `<` und `>` funktionieren.
4. Ab der Schwelle (Standard 85 %) oder bei Dateiende greifen die Sync-Regeln.

### Matching

AniList führt pro Staffel, oft sogar pro Cour, einen eigenen Eintrag, Jellyfin zählt pro Staffel. Deshalb wird jede Jellyfin-Staffel einzeln zugeordnet, in dieser Reihenfolge:

1. Eine manuelle Zuordnung der Staffel gewinnt immer und dient der nächsten Staffel als Ausgangspunkt.
2. `ProviderIds` der Staffel selbst, also AniList direkt oder MyAnimeList über `Media(idMal:)`. Eine ID, die schon eine frühere Staffel belegt, zählt nicht, denn manche Plugins kopieren die ID der Serie in jede Staffel.
3. Erste Staffel: `ProviderIds` der Serie, sonst Titelsuche mit Normalisierung und Jaro-Winkler über alle Titel und Synonyme, dazu Bonus oder Abzug für Jahr, Format und Episodenzahl. Automatisch übernommen wird ab 0.92 und mit mindestens 0.05 Abstand zum Zweitplatzierten.
4. Spätere Staffeln: der Rest des Werks der Vorstaffel, wenn es sich über beide erstreckt, sonst die nächste `SEQUEL`-Fortsetzung. Filme, OVAs und Specials dazwischen werden übersprungen, ihre Fortsetzungen aber weiter verfolgt.
5. Rückfall: Suche nach der Staffel selbst, etwa „Titel Season 2“ oder „Titel <Staffelname>“. Kandidaten ohne oder mit falscher Staffelnummer bekommen einen Abzug.
6. Alles andere bekommt ab einem Mindestwert den besten Kandidaten als unsicheren Treffer (`Guess`). Findet sich nichts, bleibt nur diese Staffel offen. Unsicher gilt pro Werk: Eine geratene Staffel schreibt nicht von selbst auf AniList und wird im Sync mit `Skip` vorgeschlagen, die übrigen Staffeln synchronisieren normal.

`plan_season` verankert jede Staffel neu. Eine Staffel über mehrere Cours bekommt mehrere Werke, ein Werk über mehrere Staffeln wird fortgesetzt, absolute Nummerierung geht über den Offset. Weicht die Zählung nur um bis zu zwei Folgen ab (Recap, Bonusfolge), beginnt die nächste Staffel trotzdem bei Episode 1 ihres eigenen Werks, statt sich zu verschieben. Jede Zuordnung gilt für einen Episodenbereich und trägt einen Offset.

Das Matching läuft beim Start, nach dem Login und bei *Refresh* für alle noch nie versuchten Titel. Nach dem Update auf die Staffel-Logik (`matching_version` 2) werden automatische Zuordnungen von Serien mit mehreren Staffeln einmal neu geplant. Die erste Staffel behält dabei ihr Werk, manuelle Staffeln werden nie überschrieben. Im Ändern-Dialog lässt sich jede Staffel einzeln oder alle auf einmal zuordnen.

### Sync-Regeln

`sync::decide_watched` ist eine reine Funktion und tabellarisch getestet.

| AniList-Status | gesehene Episode | Ergebnis |
|---|---|---|
| kein Eintrag, `PLANNING` | beliebig | `CURRENT` mit `startedAt` |
| kein Eintrag, `PLANNING`, `CURRENT` | letzte | `COMPLETED` mit `completedAt` |
| `CURRENT` | höher als Fortschritt | Fortschritt steigt |
| `CURRENT`, `REPEATING` | nicht höher | nichts |
| `REPEATING` | letzte | `COMPLETED`, `repeat` + 1 |
| `PAUSED`, `DROPPED` | beliebig | nichts, Meldung als Konflikt |
| `COMPLETED` | beliebig | nichts |

Automatisch läuft der Sync nur von Jellyfin nach AniList, damit keine Schleifen entstehen. Der manuelle Abgleich zeigt pro Abweichung eine Richtung, die du ändern kannst.

### Outbox

Jeder Schreibzugriff auf AniList geht zuerst in die SQLite-Tabelle `outbox`, die Anzeige aktualisiert sich sofort. Pro Werk gibt es einen Auftrag, neuere Änderungen werden hineingemischt. Eine Revisionsnummer verhindert, dass ein gerade gesendeter Auftrag eine währenddessen eingegangene Änderung löscht. Vorübergehende Fehler werden mit Backoff wiederholt, bis maximal eine Stunde Abstand. Dauerhafte Fehler bleiben als fehlgeschlagen sichtbar und lassen sich neu anstoßen.

Der Rate-Limiter startet mit dem gemessenen Limit von 30 Anfragen pro Minute, übernimmt `X-RateLimit-Limit` aus jeder Antwort und beachtet `Retry-After`.

## Sicherheit

- Standardablage ist `credentials.toml` mit 0600 in einem 0700-Verzeichnis, atomar geschrieben. Zu offene Rechte führen zum Abbruch. Secret Service ist per Konfiguration wählbar.
- Passwörter werden nie gespeichert. Tokens liegen im Speicher als `SecretString` und tauchen nicht in Logs auf.
- Der AniList-Login nutzt den Implicit Grant, also kein Client-Secret. Der Loopback-Server hört nur auf 127.0.0.1, prüft den Host-Header, nutzt einen zufälligen Pfad pro Login und nimmt das Token erst nach Prüfung per `Viewer` an.
- Der mpv-Socket liegt in einem 0700-Verzeichnis unter `$XDG_RUNTIME_DIR`.

## Bekannte Limitationen

- Im eingebetteten Player gilt `mpv.conf` nicht. Von mehreren externen Untertiteldateien wird nur die erste geladen, eingebettete Spuren sind davon nicht betroffen.
- Ein Untertitel, der exakt bei Sekunde 0 beginnt, kann beim Start fehlen.
- Ob transkodiert wird, entscheidet der Server. Eigene Profile gibt es nicht, nur `max_bitrate`.
- Es gibt keinen Offline-Download, ohne Server läuft nur die AniList-Verwaltung über den Cache.
- Kein automatischer Sync von AniList nach Jellyfin, nur manuell.
- AniList-Tokens müssen jährlich erneuert werden.
- Provider-IDs von AniDB und TVDB werden nicht ausgewertet, AniList bietet dafür keine direkte Suche. Specials (Staffel 0) werden nur manuell zugeordnet.
- Mit dem aktuellen Limit von 30 Anfragen pro Minute dauert die erste Zuordnung einer großen Bibliothek spürbar, weil jeder Titel eine bis mehrere Anfragen braucht.
- Eigene CA-Zertifikate kommen ausschließlich aus dem System-Store.
- Umbenennen einer Custom List ist bei AniList nicht atomar. Bricht der Vorgang ab, existieren beide Listen.
- Die Oberfläche ist nur auf Englisch.

## Mögliche Erweiterungen

- Quick Connect als dritte Jellyfin-Anmeldung.
- Offline-Modus mit Downloads.
- Weitere Tracker wie MyAnimeList oder Kitsu über einen `Tracker`-Trait.
- Benachrichtigungen für neue Episoden über die Airing-Daten von AniList.
- Eine Oberfläche für Qt, falls die native Optik auf KDE wichtiger wird.
