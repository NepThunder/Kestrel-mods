//! RMotd for Kestrel: the original's MOTD list, its commands and
//! its 10-second rotation — and its one honest gap: the client
//! SDK has no surface that writes the LAN server's MOTD, so the
//! rotation turns in the mod and shows in chat, and reaches no
//! server list.
//!
//! Behaviour reimplemented from RMotd by adasjusk (CC-BY-SA-4.0,
//! <https://github.com/adasjusk/RMotd>), as described by the
//! Horizon registry entry for the port by Vakarux12; this file
//! shares no code with either. The original rotates the MOTD of
//! a world opened to LAN every 10 seconds through the list in
//! `config/rmotd.json`, toggles rotation with `/randommotd`, and
//! manages the list with `/motd create <name> <motd>` and
//! `/motd reload`; MOTDs take `&` colour codes. This port keeps
//! the list (one MOTD per line of the file `motds` in the mod's
//! own storage, the file `/motd reload` re-reads), the commands,
//! the toggle and the 10-second beat; the current MOTD is shown
//! in the player's chat as it changes, which is also how a
//! player reads the list back. Two things do not carry over and
//! are not reimagined: the MOTD write itself (the Horizon port
//! reaches a private field of the LAN server; the client SDK has
//! no LAN or MOTD surface at all — see the README), and the `&`
//! colour codes (chat lines a mod shows are plain text, so a
//! code would print literally; stored MOTDs are kept verbatim,
//! codes and all, so the list loses nothing). This port's home
//! is <https://github.com/NepThunder/Kestrel-mods/tree/main/mods/rmotd>.

use horizon_mod_sdk::{Mod, Registration, chat, storage};

/// The storage file holding the MOTD list, one per line: the
/// counterpart of the original's `config/rmotd.json`, in the
/// mod's own storage folder.
const MOTDS_KEY: &str = "motds";
/// The rotation's period in ticks: the original's 10 seconds at
/// 20 ticks per second.
const ROTATE_TICKS: u64 = 200;

/// The index the rotation shows next: it walks the list in
/// order and wraps, as the original cycles its list.
fn next_index(current: usize, len: usize) -> usize {
    if len == 0 {
        0
    } else {
        (current + 1) % len
    }
}

/// Parse the stored list: one MOTD per line, blank lines skipped.
fn parse_list(text: &str) -> Vec<String> {
    text.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect()
}

/// Split `/motd create <name> <motd>`: the name is one word, the
/// MOTD is the rest of the line. None when either part is absent.
fn split_create(rest: &str) -> Option<(&str, &str)> {
    let rest = rest.trim();
    let (name, motd) = rest.split_once(char::is_whitespace)?;
    let motd = motd.trim();
    if name.is_empty() || motd.is_empty() {
        None
    } else {
        Some((name, motd))
    }
}

struct Rmotd {
    /// Rotation on or off: the original's default is on, and
    /// `/randommotd` toggles it.
    rotating: bool,
    /// The MOTD list, as last read from storage.
    motds: Vec<String>,
    /// Which entry the rotation currently shows.
    index: usize,
    /// The last MOTD handed to chat, to speak only on a change.
    shown: Option<String>,
}

impl Rmotd {
    fn load_list() -> Vec<String> {
        let Some(bytes) = storage::get(MOTDS_KEY) else { return Vec::new() };
        let Ok(text) = std::str::from_utf8(&bytes) else { return Vec::new() };
        parse_list(text)
    }

    fn save_list(&self) {
        let text = self.motds.join("\n");
        let _ = storage::set(MOTDS_KEY, text.as_bytes());
    }

    /// The MOTD the rotation currently points at, if any.
    fn current(&self) -> Option<&str> {
        self.motds.get(self.index).map(String::as_str)
    }

    /// Show the current MOTD in chat when it changed.
    fn announce(&mut self) {
        let Some(current) = self.current() else { return };
        if self.shown.as_deref() != Some(current) {
            chat::show(current);
            self.shown = Some(current.to_string());
        }
    }
}

impl Mod for Rmotd {
    fn init() -> Result<(Self, Registration), String> {
        let state =
            Rmotd { rotating: true, motds: Self::load_list(), index: 0, shown: None };
        Ok((state, Registration::new().command("randommotd").command("motd").on_tick()))
    }

    fn tick(&mut self, tick: u64) {
        if !self.rotating || self.motds.is_empty() {
            return;
        }
        if tick % ROTATE_TICKS == 0 {
            if tick > 0 {
                self.index = next_index(self.index, self.motds.len());
            }
            self.announce();
        }
    }

    fn command(&mut self, name: &str, args: &str) -> Result<Option<String>, String> {
        match name {
            "randommotd" => {
                self.rotating = !self.rotating;
                Ok(Some(if self.rotating {
                    "RMotd: rotation on".to_string()
                } else {
                    "RMotd: rotation off".to_string()
                }))
            }
            "motd" => {
                let args = args.trim();
                if args.is_empty() {
                    // The bare command reads the list back: the
                    // current MOTD and how many are stored.
                    return Ok(Some(match self.current() {
                        Some(current) => format!(
                            "RMotd: {} of {}: {current}",
                            self.index + 1,
                            self.motds.len()
                        ),
                        None => "RMotd: the list is empty".to_string(),
                    }));
                }
                if args == "reload" {
                    self.motds = Self::load_list();
                    if self.index >= self.motds.len() {
                        self.index = 0;
                    }
                    self.shown = None;
                    return Ok(Some(format!(
                        "RMotd: reloaded, {} MOTDs",
                        self.motds.len()
                    )));
                }
                if let Some(rest) = args.strip_prefix("create ") {
                    let Some((_name, motd)) = split_create(rest) else {
                        return Err("Usage: /motd create <name> <motd>".into());
                    };
                    // The original's names label entries in its
                    // JSON; this port's list is lines of a file,
                    // so the name is accepted and the MOTD is
                    // what is stored.
                    self.motds.push(motd.to_string());
                    self.save_list();
                    return Ok(Some(format!(
                        "RMotd: added, {} MOTDs",
                        self.motds.len()
                    )));
                }
                Err("Usage: /motd, /motd create <name> <motd>, /motd reload".into())
            }
            _ => Err("This command does nothing".into()),
        }
    }
}

horizon_mod_sdk::export_mod!(Rmotd);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_walks_and_wraps() {
        assert_eq!(next_index(0, 3), 1);
        assert_eq!(next_index(2, 3), 0);
        assert_eq!(next_index(0, 1), 0);
        assert_eq!(next_index(0, 0), 0);
    }

    #[test]
    fn list_parsing_keeps_lines_and_skips_blanks() {
        assert_eq!(parse_list("one\ntwo\n"), vec!["one", "two"]);
        assert_eq!(parse_list("\n  \n one \n\ntwo\n"), vec!["one", "two"]);
        assert!(parse_list("").is_empty());
    }

    #[test]
    fn create_splits_name_from_motd() {
        assert_eq!(
            split_create("lobby Welcome to the server"),
            Some(("lobby", "Welcome to the server"))
        );
        assert_eq!(split_create("oneword"), None);
        assert_eq!(split_create("name "), None);
        assert_eq!(split_create(""), None);
    }
}
