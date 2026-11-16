# Changelog

Alle Versionen von PlexCloud für Linux.

## [0.1.1] - 2026-11-16

- Schneller: bis zu 4 Dateien gleichzeitig hoch- oder herunterladen (bei vielen Fotos etwa drei- bis viermal so schnell). Die Upload-Grenze gilt für alle zusammen
- Weniger Speicher im Hintergrund: Ohne Fenster etwa 75 MB statt rund 400. Nach dem Schließen startet die App leise neu, sobald nichts übertragen wird, und lädt die Oberfläche erst beim Öffnen wieder
- Behoben: Nach einem Neustart in der Pause zeigte die Übersicht „0 Ordner“
- Behoben: Neustart nach einem Update außerhalb von systemd konnte sich selbst blockieren
- Behoben: Während eines Laufs zeigten alle Ordner den Fortschritt der gerade übertragenen Datei, und der Startknopf für noch nicht freigegebene Ordner verschwand. Jetzt zeigt nur der aktive Ordner Fortschritt, die anderen behalten ihren Stand
- „Letzter Abgleich“ zählt ab dem ersten fertigen Ordner, nicht erst nach dem ganzen Lauf
- Installationsskript: Prüfung der Bibliotheken korrigiert

## [0.1.0] - 2026-11-16

- Erste Version. Cloud-Ordner landen in den echten Systemordnern: /Bilder ⇄ ~/Bilder, /Videos ⇄ ~/Videos, /Dokumente ⇄ ~/Dokumente (aus xdg-user-dirs). Weitere Ordner frei zuordenbar
- Abgleich in beide Richtungen, Unterordner 1 : 1. Gelöscht heißt Papierkorb auf beiden Seiten; geänderte Dateien werden als neue Version hochgeladen
- Sicherungen: erster Abgleich je Ordner erst nach Bestätigung (mit Vorschau), Löschstopp bei vielen Löschungen auf einmal, Konflikte behalten beide Fassungen
- Monatsordner der Handy-App (Bilder/2026-11) werden zu Jahresordnern (Bilder/2026) zusammengelegt
- Tray-Symbol für Plasma, Akzentfarbe frei wählbar, Autostart als systemd-Benutzerdienst, Upload-Grenze, Ausnahmen
- Anmeldung mit Google oder E-Mail über den Browser
- Aktualisiert sich selbst aus GitHub (Installation in ~/.local/bin, Prüfsumme wird kontrolliert)
