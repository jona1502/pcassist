# Entwicklungsroadmap – Rust Accessibility Assistant

> **Ziel:** Eine barrierefreie Windows-Desktop-Assistenz, die im Hintergrund über das System-Tray läuft und bei Bedarf ein rechts angedocktes Bedienpanel öffnet. Die Software bietet schrittweise Bildschirmzoom, Vorlesen, Sprachsteuerung und weitere Eingabehilfen.

**Plattform zum Start:** Windows 10/11  
**Technologieannahme:** Rust als Core; Slint (winit-Backend) als UI, `tray-icon` fürs System-Tray. Die Architektur soll einen späteren Wechsel der UI-Technik ermöglichen.  
**Zeitangaben:** Grobe Schätzung für **eine entwickelnde Person in Teilzeit (ca. 10–15 Std./Woche)**. Kein verbindlicher Terminplan; die Dauer hängt insbesondere von Windows-Integration, Tests und Feedback betroffener Nutzer ab.

## Meilensteinübersicht

| Phase | Ungefährer Zeitraum | Schwerpunkt | Ergebnis |
|---|---|---|---|
| 0 | Woche 1 | Anforderungen, Techniktests, Barrierefreiheit | Tragfähiges Konzept und technische Machbarkeit geprüft |
| 1 | Wochen 2–3 | Rust-Grundgerüst und System-Tray | App startet im Hintergrund und ist über das Tray steuerbar |
| 2 | Wochen 4–5 | Rechtes Assistenzpanel | Panel lässt sich zuverlässig öffnen, schließen und bedienen |
| 3 | Wochen 6–8 | Zoom und Anzeigehilfen | Erste tatsächlich nutzbare Sehunterstützung |
| 4 | Wochen 9–10 | Text vorlesen (TTS) | Markierter Text kann vorgelesen werden |
| 5 | Wochen 11–14 | Offline-Sprachsteuerung | Definierte Sprachbefehle steuern zentrale Funktionen |
| 6 | Wochen 15–17 | Eingabehilfen und Profile | Personalisierbare Bedienung für unterschiedliche Bedürfnisse |
| 7 | Wochen 18–20 | Tests, Stabilisierung und Beta | Installierbare, getestete Beta-Version |

**Priorität:** Phasen 0–3 ergeben das erste funktionsfähige MVP. Die weiteren Phasen erweitern es zur Assistenzsoftware.

---

## Phase 0 – Projektgrundlagen und Machbarkeit (Woche 1)

**Ziel:** Kritische technische und barrierefreie Anforderungen früh klären.

- [ ] Zielgruppen und typische Aufgaben festhalten (z. B. Sehbehinderung, motorische Einschränkungen, Sprachbedienung).
- [ ] Mit betroffenen Nutzern oder Accessibility-Expertinnen und -Experten erste Bedienabläufe durchgehen.
- [ ] Windows 10/11 als erste Plattform bestätigen; Multi-Monitor- und DPI-Skalierungsanforderungen dokumentieren.
- [ ] Technischen Mini-Prototyp prüfen: Tray-Icon, rahmenloses Fenster, Positionierung am rechten Rand, Tastaturfokus.
- [ ] Zoom-Technik evaluieren: Windows Magnification API und ihre Einschränkungen testen; nicht davon ausgehen, dass ein normal skaliertes UI-Fenster echten systemweiten Zoom ersetzt.
- [ ] Barrierefreiheit des gewählten UI-Stacks evaluieren (Screenreader, Tastaturnavigation, Fokus, Kontrast).
- [ ] Datenschutzgrundsätze definieren: möglichst offline, keine unnötigen Mikrofon-/Bildschirminhalte speichern, explizite Aktivierung.
- [ ] Git-Repository, README, Lizenzentscheidung und Issue-Board anlegen.

**Abnahme:** Tray-/Panel-Demo ist auf einem Windows-PC machbar; Zoom-Ansatz und wesentliche Risiken sind dokumentiert.

## Phase 1 – Rust-Core und System-Tray (Wochen 2–3)

**Ziel:** Eine verlässliche Hintergrund-App bereitstellen.

- [ ] Rust-Projekt mit separaten Modulen für App-State, Plattformzugriff und Features einrichten.
- [ ] Slint-Desktop-App (`apps/desktop`), Build- und Dev-Skripte konfigurieren.
- [ ] System-Tray-Icon und Kontextmenü implementieren.
- [ ] Menüeinträge: **Panel öffnen**, **Zoom aktivieren/deaktivieren** (vorerst Platzhalter), **Einstellungen**, **Beenden**.
- [ ] Ein-Instanz-Verhalten implementieren: zweiter Start öffnet bestehende App statt einer zweiten Instanz.
- [ ] Konfiguration lokal speichern (`serde` + TOML/JSON).
- [ ] Sichere Fehlerbehandlung, Logging ohne sensible Nutzungsdaten und sauberes Beenden einbauen.
- [ ] Optional: Autostart einstellbar machen, standardmäßig nicht ungefragt aktivieren.

**Abnahme:** App läuft im Hintergrund, lässt sich über das Tray bedienen und speichert Einstellungen nach einem Neustart.

## Phase 2 – Rechtes Assistenzpanel (Wochen 4–5)

**Ziel:** Die zentrale Steueroberfläche barrierefrei nutzbar machen.

- [ ] Rahmenloses Panel rechts am Bildschirmrand implementieren (anfangs ca. 360 px breit, anpassbar).
- [ ] Öffnen/Schließen per Tray und globalem Hotkey; optional ein ständig sichtbarer Rand-Reiter.
- [ ] Panel bleibt innerhalb des nutzbaren Bildschirmbereichs (Taskleiste berücksichtigen).
- [ ] Mehrere Monitore, unterschiedliche Skalierungen und geänderte Auflösungen behandeln.
- [ ] Bedienung per Tastatur, sichtbare Fokusmarkierung, ausreichend große Bedienelemente, Screenreader-Labels.
- [ ] Schnellaktionen als UI-Platzhalter: Zoom, Vorlesen, Mikrofon, Kontrast, Einstellungen.
- [ ] Verhalten „immer im Vordergrund“ optional machen; Fokus nicht ungefragt aus anderen Apps stehlen.
- [ ] Animationen optional bzw. reduziert anbieten; nicht ausschließlich auf Hover setzen.
- [ ] Panel-Zustand und bevorzugten Monitor speichern.

**Abnahme:** Panel lässt sich mit Maus, Tastatur und Tray erreichen und auf typischen Multi-Monitor-/DPI-Setups zuverlässig bedienen.

## Phase 3 – Zoom und visuelle Hilfen: MVP (Wochen 6–8)

**Ziel:** Eine echte, praxistaugliche Vergrößerungsfunktion.

- [ ] Windows-Zoom-Backend auf Basis des Machbarkeitstests integrieren.
- [ ] Zoom-Stufen (z. B. 100–800 %) und Plus/Minus/Reset bereitstellen.
- [ ] Lupenmodus um Mauszeiger **oder** Vollbildzoom als zuerst getesteten Modus implementieren; zweiten Modus nur bei gesicherter Stabilität ergänzen.
- [ ] Bewegung des Mauszeigers korrekt verfolgen; Bildschirmränder berücksichtigen.
- [ ] Globale Hotkeys für Zoom implementieren und Konflikte mit bestehenden Windows-Shortcuts prüfen.
- [ ] Sichtbaren Mauszeiger, Kontrastoptionen und ggf. Farbfilter als getrennte Funktionen evaluieren.
- [ ] Leistung, Latenz und Verhalten bei Monitorwechsel, Sperrbildschirm und DPI-Wechsel testen.
- [ ] Systemverhalten bei ausgeschaltetem oder abstürzendem Zoom-Modul sicher zurücksetzen.

**Abnahme (MVP v0.1):** Tray + Panel + mindestens ein stabiler Zoom-Modus funktionieren zusammen. Die Anwendung bleibt ohne Maus bedienbar.

## Phase 4 – Vorlesefunktion (Wochen 9–10)

**Ziel:** Inhalte mit möglichst wenigen Schritten vorlesen lassen.

- [ ] Windows-Text-to-Speech als zunächst systemnahes Backend anbinden.
- [ ] Vorlesen, Pause/Fortsetzen, Stop und Geschwindigkeit anbieten.
- [ ] Zunächst **markierten Text** vorlesen (soweit zuverlässig zugänglich).
- [ ] UI-Automation-Zugriff auf fokussierte Steuerelemente prototypisch ergänzen.
- [ ] Verständliche Meldungen für Anwendungen ausgeben, deren Text nicht ausgelesen werden kann.
- [ ] Sprachausgabe per Tastatur und Panel steuerbar machen.

**Abnahme:** Markierter Text kann in den unterstützten Anwendungen gestartet und gestoppt werden, ohne dass die App hängen bleibt.

## Phase 5 – Offline-Sprachsteuerung (Wochen 11–14)

**Ziel:** Ein überschaubarer Satz gesprochener Befehle steuert die Assistenz.

- [ ] Audioaufnahme mit `cpal` integrieren.
- [ ] Offline-Spracherkennung evaluieren (`vosk` oder `whisper-rs`) und nach Latenz, Speicherbedarf und Qualität auswählen.
- [ ] Mikrofon ausdrücklich aktivieren/deaktivieren; Aufnahmezustand im Tray/Panel sichtbar machen.
- [ ] Begrenzte, eindeutig definierte Befehle einführen: „Panel öffnen“, „Zoom größer“, „Zoom zurücksetzen“, „Vorlesen stoppen“.
- [ ] Sprachbefehle von freier Diktierfunktion trennen.
- [ ] Fehlaktivierungen behandeln und für riskante Aktionen eine Bestätigung vorsehen.
- [ ] Offline-Modell, Sprache und Audio-Gerät konfigurierbar machen.
- [ ] Testen mit verschiedenen Sprechweisen, Akzenten und Hintergrundgeräuschen.

**Abnahme (v0.2):** Die Kernbefehle funktionieren offline bei akzeptabler Verzögerung, und der Mikrofonstatus ist jederzeit erkennbar.

## Phase 6 – Weitere Eingabehilfen und Profile (Wochen 15–17)

**Ziel:** Individuelle Bedienung ohne überladene Oberfläche.

- [ ] Profile wie „Sehen“, „Motorik“ und „Individuell“ als veränderbare Voreinstellungen anbieten.
- [ ] Hotkeys und Schnellaktionen benutzerdefiniert konfigurierbar machen.
- [ ] Mauszeiger- und Klickhilfen evaluieren (z. B. größerer Cursor, Klick durch Verweilen, sofern zuverlässig).
- [ ] Schriftgröße, Kontrast und reduzierte Bewegung für **die eigene App** einstellbar machen.
- [ ] Profile exportierbar/importierbar machen, ohne sensible Daten unnötig zu erfassen.
- [ ] Bedienbarkeit der Einstellungen ohne Maus prüfen.

**Abnahme:** Nutzer können mindestens zwei unterschiedliche Bedienprofile konfigurieren und persistent speichern.

## Phase 7 – Qualitätssicherung und Beta (Wochen 18–20)

**Ziel:** Eine reale Testversion veröffentlichen.

- [ ] Automatisierte Tests für Konfiguration, Zustandswechsel und Befehlsrouting ergänzen.
- [ ] Manuelle Tests mit Tastatur, Windows-Sprachausgabe/Narrator und gängigen Bildschirmauflösungen durchführen.
- [ ] Mit Menschen aus den Zielgruppen testen und die wichtigsten Barrieren beheben.
- [ ] Abstürze, Start-/Shutdown-Verhalten und CPU-/RAM-Verbrauch untersuchen.
- [ ] Windows-Installer, Versionsinformation, Update-Strategie und Deinstallation vorbereiten.
- [ ] Kurzanleitung in einfacher, klarer Sprache sowie Tastenkürzel-Übersicht erstellen.
- [ ] Datenschutzinformationen, bekannte Einschränkungen und Feedback-Kanal veröffentlichen.

**Abnahme (Beta v0.3):** Installierbare Testversion, dokumentierte bekannte Probleme und keine bekannten Fehler, die zentrale Zugangswege komplett blockieren.

---

## Empfohlene Modul- und Ordnerstruktur

```text
accessibility-assistant/
├── apps/
│   └── desktop/                # Slint-App, Tray, Fenster, .slint-UI
├── crates/
│   ├── assistant-core/         # App-State, Ereignisse, Befehle
│   ├── assistant-config/       # Persistente Einstellungen und Profile
│   ├── assistant-platform-win/ # Windows-APIs, Fokus, globale Hotkeys
│   ├── assistant-zoom/         # Zoom-Backend und Steuerung
│   ├── assistant-tts/          # Vorlesen
│   ├── assistant-voice/        # Mikrofon, Erkennung, Befehle
│   └── assistant-input/        # Maus-/Tastaturhilfen
├── docs/
│   ├── architecture.md
│   ├── accessibility.md
│   └── testing.md
└── README.md
```

## Technische Grundregeln

1. **Bedienbarkeit zuerst:** Kein Feature darf ausschließlich per Maus oder Sprache erreichbar sein.
2. **Modularität:** UI und Funktionslogik trennen; Feature-Module sollen unabhängig aktivierbar sein.
3. **Datenschutz:** Offline bevorzugen; Mikrofon und andere sensible Zugriffe transparent kontrollieren.
4. **Ausfallsicherheit:** Bei Fehlern darf die Anwendung keine Eingaben blockieren oder das System dauerhaft verändern.
5. **Keine falschen Versprechen:** „Systemweiter Zoom“, „alle Texte lesen“ und „alle Apps steuern“ erst nach realen Windows-Tests zusagen.
6. **Inkrementell ausliefern:** Jede Phase sollte einen testbaren Zustand erzeugen.

## Danach mögliche Erweiterungen (Backlog)

- [ ] Weitere Zoom-Modi und Fokusverfolgung
- [ ] OCR für Inhalte, die nicht über Accessibility-APIs zugänglich sind
- [ ] Freies Diktieren in Textfeldern
- [ ] Sprachbefehle pro Anwendung
- [ ] Bildschirmtastatur / alternative Eingabegeräte
- [ ] Linux-Unterstützung nach separater Prüfung von Wayland/X11-Beschränkungen
- [ ] macOS-Unterstützung über eigenen Plattformadapter

## Wichtigste Risiken

| Risiko | Gegenmaßnahme |
|---|---|
| Systemweiter Zoom ist technisch komplex | Bereits in Phase 0 Proof of Concept erstellen |
| Tray/Panel verhält sich auf mehreren Monitoren inkonsistent | Ab Phase 2 mit Multi-Monitor + DPI testen |
| Sprachsteuerung erkennt Befehle falsch | Begrenzte Befehlsgrammatik, sichtbarer Status, Bestätigung |
| Vorlesen funktioniert nicht in jeder App | Unterstützte Fälle dokumentieren; später OCR als Alternative |
| Die Assistenz-App ist selbst nicht barrierefrei | Screenreader- und Tastaturtests von Beginn an |

**Nächster konkreter Schritt:** Phase 0 starten und einen kleinen Windows-Prototyp erstellen, der ein Tray-Icon anzeigt und ein rechts angedocktes, per Tastatur erreichbares Panel öffnen kann.
