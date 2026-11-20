//! English and Italian. UI text is written in English and passed through [`tr`]; the Italian
//! table below has every string (a test checks).

use std::sync::atomic::{AtomicBool, Ordering};

use crate::settings::LangPref;

static ITALIAN: AtomicBool = AtomicBool::new(false);

pub fn set(pref: LangPref) {
    let italian = match pref {
        LangPref::Italian => true,
        LangPref::English => false,
        LangPref::Auto => sys_locale::get_locale().is_some_and(|l| l.to_ascii_lowercase().starts_with("it")),
    };
    ITALIAN.store(italian, Ordering::Relaxed);
}

/// `"it"` or `"en"`, for the catalog's localized text.
pub fn code() -> &'static str {
    if ITALIAN.load(Ordering::Relaxed) { "it" } else { "en" }
}

/// `s` in the current language.
pub fn tr(s: &'static str) -> &'static str {
    if ITALIAN.load(Ordering::Relaxed) { italian(s).unwrap_or(s) } else { s }
}

/// `tr` with `{}` replaced by `arg`.
pub fn trf(s: &'static str, arg: impl std::fmt::Display) -> String {
    tr(s).replacen("{}", &arg.to_string(), 1)
}

fn italian(s: &str) -> Option<&'static str> {
    Some(match s {
        // Navigation
        "Library" => "Libreria",
        "Downloads" => "Download",
        "Settings" => "Impostazioni",
        "Apps" => "App",
        "More apps are on the way" => "Altre app in arrivo",
        "Launcher" => "Launcher",
        // App states
        "Installed" => "Installata",
        "Not installed" => "Non installata",
        "Update available" => "Aggiornamento disponibile",
        "Up to date" => "Aggiornata",
        "Running" => "In esecuzione",
        "Checking…" => "Controllo…",
        "Version {}" => "Versione {}",
        "v{} available" => "v{} disponibile",
        // Actions
        "Open" => "Apri",
        "Install" => "Installa",
        "Update" => "Aggiorna",
        "Update to {}" => "Aggiorna alla {}",
        "Restart {}" => "Riavvia {}",
        "Cancel" => "Annulla",
        "Uninstall" => "Disinstalla",
        "Uninstall…" => "Disinstalla…",
        "Reinstall" => "Reinstalla",
        "Install this version" => "Installa questa versione",
        "Check for updates" => "Cerca aggiornamenti",
        "Show in file manager" => "Mostra nella cartella",
        "Release page" => "Pagina della release",
        "Website" => "Sito",
        "More" => "Altro",
        "Retry" => "Riprova",
        "Close" => "Chiudi",
        "Not available for this computer" => "Non disponibile per questo computer",
        "Install as" => "Installa come",
        // Progress
        "Preparing…" => "Preparazione…",
        "Downloading…" => "Download in corso…",
        "Installing…" => "Installazione…",
        "Removing…" => "Rimozione…",
        "Waiting for {} to close…" => "In attesa che {} venga chiuso…",
        "Waiting for permission…" => "In attesa dell'autorizzazione…",
        "{} of {}" => "{} di {}",
        "{} left" => "{} rimanenti",
        // Details
        "What's new in {}" => "Novità della {}",
        "No release notes." => "Nessuna nota di rilascio.",
        "Details" => "Dettagli",
        "Installed version" => "Versione installata",
        "Latest version" => "Ultima versione",
        "Released" => "Pubblicata",
        "Format" => "Formato",
        "Download size" => "Dimensione",
        "Location" => "Posizione",
        "This computer" => "Questo computer",
        "Versions" => "Versioni",
        "Latest" => "Ultima",
        "Pre-release" => "Pre-release",
        "Also installed" => "Installata anche in",
        "About" => "Informazioni",
        "unknown" => "sconosciuta",
        "Couldn't check for updates" => "Impossibile cercare aggiornamenti",
        // Downloads page
        "Nothing downloaded yet" => "Ancora nessun download",
        "Installs and updates show up here." => "Installazioni e aggiornamenti compaiono qui.",
        "Installing {}" => "Installazione di {}",
        "Updating to {}" => "Aggiornamento alla {}",
        "Removing {}" => "Rimozione di {}",
        "Done" => "Completato",
        "Failed" => "Non riuscito",
        "Cancelled" => "Annullato",
        "Clear finished" => "Rimuovi completati",
        // Toasts
        "{} is ready" => "{} è pronta",
        "{} was removed" => "{} è stata rimossa",
        "Restart the app to use the new version." => "Riavvia l'app per usare la nuova versione.",
        // Uninstall dialog
        "Uninstall {}?" => "Disinstallare {}?",
        "Your drawings and settings are kept." => "I tuoi disegni e le impostazioni restano.",
        "The app is moved to the Trash." => "L'app viene spostata nel Cestino.",
        // Settings
        "Appearance" => "Aspetto",
        "Theme" => "Tema",
        "System" => "Sistema",
        "Dark" => "Scuro",
        "Light" => "Chiaro",
        "Language" => "Lingua",
        "Automatic" => "Automatica",
        "Updates" => "Aggiornamenti",
        "Check for updates when the launcher starts" => "Cerca aggiornamenti all'avvio del launcher",
        "Install app updates automatically" => "Installa automaticamente gli aggiornamenti delle app",
        "Only while the app is closed." => "Solo quando l'app è chiusa.",
        "Offer pre-release versions" => "Proponi versioni pre-release",
        "Check now" => "Controlla ora",
        "Installation" => "Installazione",
        "Install apps in" => "Installa le app in",
        "Applications (all users)" => "Applicazioni (tutti gli utenti)",
        "My Applications folder" => "La mia cartella Applicazioni",
        "Install apps for" => "Installa le app per",
        "Only me" => "Solo per me",
        "Everyone" => "Tutti gli utenti",
        "Installing for everyone asks for administrator permission." => "L'installazione per tutti chiede i permessi di amministratore.",
        "Package format" => "Formato dei pacchetti",
        "System package (.deb, .rpm, Arch)" => "Pacchetto di sistema (.deb, .rpm, Arch)",
        "System packages need your password; AppImages don't." => "I pacchetti di sistema chiedono la password, gli AppImage no.",
        "Close the launcher when an app starts" => "Chiudi il launcher quando avvii un'app",
        "Version" => "Versione",
        "Open data folder" => "Apri cartella dati",
        "Open log" => "Apri log",
        "Clear download cache" => "Svuota cache dei download",
        "Download cache cleared" => "Cache dei download svuotata",
        // The launcher's own setup
        "Install {}" => "Installa {}",
        "Update {}" => "Aggiorna {}",
        "Install, update and open the DraftCraft apps, all in one place." => "Installa, aggiorna e apri le app DraftCraft, tutto in un unico posto.",
        "Installs the right version for this computer" => "Installa la versione giusta per questo computer",
        "Keeps your apps up to date" => "Tiene aggiornate le tue app",
        "Opens them with one click" => "Le apre con un clic",
        "Install for" => "Installa per",
        "Shortcut on the desktop" => "Collegamento sul desktop",
        "Run without installing" => "Avvia senza installare",
        "The apps you installed with it stay on this computer." => "Le app installate con il launcher restano su questo computer.",
        "Also remove settings and downloads" => "Rimuovi anche impostazioni e download",
        "Copying files and adding shortcuts…" => "Copia dei file e creazione dei collegamenti…",
        "Removing files and shortcuts…" => "Rimozione di file e collegamenti…",
        "You'll find it in the Start menu." => "Lo trovi nel menu Start.",
        "You'll find it in your applications menu." => "Lo trovi nel menu delle applicazioni.",
        "Open {} now" => "Apri {} adesso",
        "Finish" => "Fine",
        "{} is installed" => "{} è installato",
        "{} has been uninstalled" => "{} è stato disinstallato",
        "The installation didn't finish" => "L'installazione non è stata completata",
        "The uninstall didn't finish" => "La disinstallazione non è stata completata",
        // Menu bar / tray
        "Open D.E.V. Launcher" => "Apri D.E.V. Launcher",
        "Quit D.E.V. Launcher" => "Esci da D.E.V. Launcher",
        "Updating {}…" => "Aggiornamento di {}…",
        "Always on" => "Sempre attivo",
        "Keep running in the menu bar" => "Resta attivo nella barra dei menu",
        "Keep running in the notification area" => "Resta attivo nell'area di notifica",
        "Keep running in the system tray" => "Resta attivo nell'area di sistema",
        "Open your apps and get updates with this window closed." => "Apri le tue app e ricevi gli aggiornamenti anche a finestra chiusa.",
        "Start with the computer" => "Avvia all'accensione del computer",
        "Starts quietly in the background when you sign in." => "Parte in silenzio in background quando accedi.",
        // The launcher's own update
        "Updating to {}…" => "Aggiornamento alla {}…",
        "Version {} is ready" => "Versione {} pronta",
        "Version {} is available" => "Versione {} disponibile",
        "D.E.V. Launcher {} is ready" => "D.E.V. Launcher {} è pronto",
        "Restart to use the new version." => "Riavvialo per usare la nuova versione.",
        "Restart now" => "Riavvia ora",
        "Download" => "Scarica",
        "Try again" => "Riprova",
        "Needs administrator permission." => "Servono i permessi di amministratore.",
        "Couldn't update D.E.V. Launcher" => "Impossibile aggiornare D.E.V. Launcher",
        "Update D.E.V. Launcher to {}" => "Aggiorna D.E.V. Launcher alla {}",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every literal passed to `tr`/`trf` in the UI has an Italian translation.
    #[test]
    fn every_ui_string_is_translated() {
        let sources = [
            include_str!("ui/mod.rs"),
            include_str!("ui/library.rs"),
            include_str!("ui/pages.rs"),
            include_str!("ui/widgets.rs"),
            include_str!("ui/setup.rs"),
            include_str!("tray.rs"),
        ];
        let mut missing = Vec::new();
        for src in sources {
            for marker in ["tr(\"", "trf(\""] {
                let mut rest = src;
                while let Some(i) = rest.find(marker) {
                    // `push_str("…")` isn't a call to `tr`.
                    let call = !rest[..i].ends_with(|c: char| c.is_alphanumeric() || c == '_');
                    rest = &rest[i + marker.len()..];
                    if !call {
                        continue;
                    }
                    let end = rest.find('"').unwrap();
                    let s = &rest[..end];
                    if italian(s).is_none() {
                        missing.push(s.to_string());
                    }
                }
            }
        }
        assert!(missing.is_empty(), "missing Italian for: {missing:?}");
    }
}
