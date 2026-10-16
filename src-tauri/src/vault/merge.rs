//! Fusión de dos versiones del mismo llavero (la de memoria y la que ha aparecido en disco: Drive, OneDrive,
//! otro PC…). Por cada entrada gana la modificada más tarde; lo nuevo de cada lado se suma; lo borrado para
//! siempre en cualquiera de los dos no vuelve.

use super::model::Vault;
use crate::crypto::Sealer;
use std::collections::HashSet;

#[derive(Debug, Default, PartialEq)]
pub struct Report {
    /// Entradas que han llegado de la otra versión (nuevas o más recientes)
    pub incoming: usize,
    /// Hay cosas de aquí que la otra versión no tiene: hay que escribir el resultado
    pub ours_newer: bool,
}

/// `ours` está en memoria (secretos sellados); `theirs` viene recién descifrado del disco (en claro).
pub fn merge(ours: &mut Vault, theirs: Vault, s: &Sealer) -> Report {
    let mut r = Report::default();
    let tombs: HashSet<String> = ours.tombstones.iter().chain(theirs.tombstones.iter()).cloned().collect();
    r.ours_newer |= ours.tombstones.iter().any(|t| !theirs.tombstones.contains(t));
    let theirs_ids: HashSet<&str> = theirs.entries.iter().map(|e| e.id.as_str()).collect();
    ours.entries.retain(|e| !tombs.contains(&e.id));
    r.ours_newer |= ours.entries.iter().any(|e| !theirs_ids.contains(e.id.as_str()));

    for mut e in theirs.entries {
        if tombs.contains(&e.id) {
            continue;
        }
        match ours.entries.iter().position(|x| x.id == e.id) {
            Some(i) if e.modified > ours.entries[i].modified => {
                e.used = e.used.max(ours.entries[i].used);
                e.seal(s);
                ours.entries[i] = e;
                r.incoming += 1;
            }
            Some(i) => r.ours_newer |= ours.entries[i].modified > e.modified,
            None => {
                e.seal(s);
                ours.entries.push(e);
                r.incoming += 1;
            }
        }
    }
    r.ours_newer |= ours.folders.iter().any(|f| !theirs.folders.iter().any(|x| x.id == f.id));
    for f in theirs.folders {
        if !ours.folders.iter().any(|x| x.id == f.id) {
            ours.folders.push(f);
            r.incoming += 1;
        }
    }
    let mut t: Vec<String> = tombs.into_iter().collect();
    t.sort();
    ours.tombstones = t;
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vault::model::*;

    fn pw(e: &Entry, s: &Sealer) -> String {
        e.fields[0].value.reveal_string(s).to_string()
    }

    fn entry(id: &str, pw: &str, modified: i64) -> Entry {
        Entry {
            id: id.into(),
            kind: Kind::Login,
            title: id.into(),
            folder: None,
            tags: vec![],
            favorite: false,
            fields: vec![Field { id: "f".into(), key: "password".into(), label: String::new(), value: Sv::text(pw), secret: true, kind: FieldKind::Password }],
            attachments: vec![],
            icon: None,
            autotype: None,
            history: vec![],
            created: 1,
            modified,
            used: None,
            deleted: None,
            breach: None,
            archived: false,
            protected: false,
        }
    }

    fn vault(entries: Vec<Entry>) -> Vault {
        Vault { name: "x".into(), entries, ..Default::default() }
    }

    #[test]
    fn newest_wins_and_new_ones_add_up() {
        let s = Sealer::new();
        let mut ours = vault(vec![entry("a", "mia-vieja", 10), entry("b", "mia-nueva", 30), entry("solo-aqui", "x", 5)]);
        ours.seal(&s);
        let theirs = vault(vec![entry("a", "suya-nueva", 20), entry("b", "suya-vieja", 15), entry("solo-alli", "y", 5)]);
        let r = merge(&mut ours, theirs, &s);
        let get = |id: &str| pw(ours.entries.iter().find(|e| e.id == id).unwrap(), &s);
        assert_eq!(get("a"), "suya-nueva");
        assert_eq!(get("b"), "mia-nueva");
        assert_eq!(get("solo-alli"), "y");
        assert_eq!(get("solo-aqui"), "x");
        assert_eq!(r.incoming, 2);
        assert!(r.ours_newer, "«b» y «solo-aqui» no están en disco: hay que escribir");
        assert!(ours.entries.iter().all(|e| e.fields[0].value.is_sealed()), "lo que llega se sella");
    }

    #[test]
    fn purged_entries_never_come_back() {
        let s = Sealer::new();
        let mut ours = vault(vec![entry("viva", "1", 1)]);
        ours.tombstones = vec!["borrada-aqui".into()];
        let mut theirs = vault(vec![entry("viva", "1", 1), entry("borrada-aqui", "2", 99), entry("borrada-alli", "3", 1)]);
        theirs.tombstones = vec!["borrada-alli".into()];
        let mut ours_with = ours;
        ours_with.entries.push(entry("borrada-alli", "3", 1));
        merge(&mut ours_with, theirs, &s);
        let ids: Vec<&str> = ours_with.entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["viva"]);
        assert_eq!(ours_with.tombstones, ["borrada-alli", "borrada-aqui"]);
    }

    #[test]
    fn same_content_needs_no_write() {
        let s = Sealer::new();
        let mut ours = vault(vec![entry("a", "1", 5)]);
        ours.seal(&s);
        let r = merge(&mut ours, vault(vec![entry("a", "1", 5)]), &s);
        assert_eq!(r, Report { incoming: 0, ours_newer: false });
    }
}
