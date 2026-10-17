//! `.kdbx` de KeePass y KeePassXC (KDBX 3.1 y 4), con contraseña y/o archivo llave.

use super::{custom, f, host_title, Imported, ImportedFile, Key};
use crate::error::{AppError, Res};
use crate::vault::model::Kind;
use keepass::db::{fields, EntryRef};
use keepass::error::{DatabaseKeyError, DatabaseOpenError};
use keepass::{Database, DatabaseKey};

/// Campos con sitio propio (el resto entran como campos personalizados)
const KNOWN: [&str; 6] = [fields::TITLE, fields::USERNAME, fields::PASSWORD, fields::URL, fields::NOTES, fields::OTP];

pub fn parse(bytes: &[u8], key: &Key) -> Res<Vec<Imported>> {
    if key.password.is_empty() && key.keyfile.is_none() {
        return Err(AppError::new("import.needsPassword"));
    }
    let mut dk = DatabaseKey::new();
    if !key.password.is_empty() {
        dk = dk.with_password(&key.password);
    }
    if let Some(kf) = &key.keyfile {
        dk = dk.with_keyfile(&mut kf.as_slice()).map_err(|_| AppError::new("import.badKeyFile"))?;
    }
    let db = Database::parse(bytes, dk).map_err(|e| {
        AppError::new(match e {
            DatabaseOpenError::Key(DatabaseKeyError::IncorrectKey) => "import.wrongKey",
            DatabaseOpenError::Key(_) => "import.badKeyFile",
            DatabaseOpenError::UnsupportedVersion => "import.unsupported",
            _ => "import.badFile",
        })
    })?;
    let bin = db.recycle_bin().map(|g| g.id());
    let mut out: Vec<Imported> = db
        .iter_all_entries()
        .filter_map(|e| {
            // Ruta del grupo sin la raíz («Bancos/España»); lo que está en la papelera no se importa
            let mut path = vec![];
            let mut g = Some(e.parent().id());
            while let Some(group) = g.and_then(|id| db.group(id)) {
                if Some(group.id()) == bin {
                    return None;
                }
                g = group.parent().map(|p| p.id());
                if g.is_some() {
                    path.push(group.name.clone());
                }
            }
            path.reverse();
            Some(entry(&e, path.join("/")))
        })
        .collect();
    out.sort_by(|a, b| (&a.folder, a.title.to_lowercase()).cmp(&(&b.folder, b.title.to_lowercase())));
    Ok(out)
}

fn entry(e: &EntryRef, folder: String) -> Imported {
    let get = |k: &str| e.get(k).unwrap_or("");
    let (totp, used) = totp(e);
    let (user, pass, url) = (get(fields::USERNAME), get(fields::PASSWORD), get(fields::URL));
    let note = [user, pass, url, &totp].iter().all(|x| x.trim().is_empty());
    let mut fields: Vec<_> = if note {
        vec![f("content", get(fields::NOTES))]
    } else {
        vec![f("username", user), f("password", pass), f("url", url), f("totp", &totp), f("notes", get(fields::NOTES))]
    };
    let mut extra: Vec<_> = e
        .fields
        .iter()
        .filter(|(k, _)| !KNOWN.contains(&k.as_str()) && !used.contains(&k.as_str()) && k.as_str() != "KPRPC JSON")
        .collect();
    extra.sort_by(|a, b| a.0.cmp(b.0));
    fields.extend(extra.into_iter().map(|(k, v)| custom(k, v.get(), v.is_protected())));
    let mut title = get(fields::TITLE).trim().to_string();
    if title.is_empty() {
        title = host_title(url);
    }
    let mut it = Imported::new(
        if note { Kind::Note } else { Kind::Login },
        if title.is_empty() { "Sin título".into() } else { title },
        (!folder.is_empty()).then_some(folder),
        false,
        fields.into_iter().flatten().collect(),
    );
    it.tags = e.tags.clone();
    it.files = e.attachments_named().map(|(name, a)| ImportedFile { name: name.into(), data: a.data.get().clone() }).collect();
    it
}

/// 2FA: KeePassXC lo guarda en `otp` (una URI `otpauth://`); KeePass 2.47+ en `TimeOtp-*`.
/// Devuelve el valor y los campos que ya ha usado (para no repetirlos como personalizados).
fn totp(e: &EntryRef) -> (String, Vec<&'static str>) {
    if let Some(o) = e.get(fields::OTP).filter(|o| !o.trim().is_empty()) {
        return (o.trim().into(), vec![]);
    }
    let Some(secret) = e.get("TimeOtp-Secret-Base32").filter(|s| !s.trim().is_empty()) else {
        return (String::new(), vec![]);
    };
    let secret: String = secret.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let digits = e.get("TimeOtp-Length").unwrap_or("6").trim();
    let period = e.get("TimeOtp-Period").unwrap_or("30").trim();
    let alg = match e.get("TimeOtp-Algorithm").unwrap_or("") {
        "HMAC-SHA-256" => "SHA256",
        "HMAC-SHA-512" => "SHA512",
        _ => "SHA1",
    };
    let used = vec!["TimeOtp-Secret-Base32", "TimeOtp-Length", "TimeOtp-Period", "TimeOtp-Algorithm"];
    if (digits, period, alg) == ("6", "30", "SHA1") {
        return (secret, used);
    }
    (format!("otpauth://totp/KeePass?secret={secret}&digits={digits}&period={period}&algorithm={alg}"), used)
}

#[cfg(test)]
mod tests {
    use super::super::parse as parse_any;
    use super::*;
    use keepass::db::Value;
    use zeroize::Zeroizing;

    fn key(pw: &str, kf: Option<&[u8]>) -> Key {
        Key { password: Zeroizing::new(pw.into()), keyfile: kf.map(|k| Zeroizing::new(k.to_vec())) }
    }

    fn sample(dk: DatabaseKey) -> Vec<u8> {
        let mut db = Database::new();
        let mut root = db.root_mut();
        root.name = "Raíz".into();
        let mut bancos = root.add_group();
        bancos.name = "Bancos".into();
        let mut es = bancos.add_group();
        es.name = "España".into();
        es.add_entry().edit(|e| {
            e.set_unprotected(fields::TITLE, "Banco");
            e.set_unprotected(fields::USERNAME, "yo");
            e.set_protected(fields::PASSWORD, "pw");
            e.set_unprotected(fields::URL, "https://banco.es");
            e.set_unprotected("TimeOtp-Secret-Base32", "JBSWY3DPEHPK3PXP");
            e.set_unprotected("TimeOtp-Length", "8");
            e.set_protected("PIN", "1234");
            e.tags.push("curro".into());
            e.add_attachment("contrato.txt", Value::unprotected(b"hola".to_vec()));
        });
        let mut root = db.root_mut();
        root.add_entry().edit(|e| {
            e.set_unprotected(fields::URL, "https://www.github.com/login");
            e.set_unprotected(fields::USERNAME, "poxi");
            e.set_unprotected(fields::OTP, "otpauth://totp/x?secret=JBSWY3DPEHPK3PXP");
        });
        root.add_entry().edit(|e| {
            e.set_unprotected(fields::TITLE, "Nota");
            e.set_protected(fields::NOTES, "secreto");
        });
        let mut bin = root.add_group();
        bin.name = "Papelera".into();
        bin.add_entry().edit(|e| e.set_unprotected(fields::TITLE, "Borrada"));
        let bin_id = bin.id();
        db.meta.recyclebin_uuid = Some(bin_id.uuid());
        let mut out = vec![];
        db.save(&mut out, dk).unwrap();
        out
    }

    #[test]
    fn kdbx_with_password() {
        let file = sample(DatabaseKey::new().with_password("demo"));
        assert_eq!(parse_any(&file, None).unwrap_err().code, "import.needsPassword");
        assert_eq!(parse_any(&file, Some(&key("mal", None))).unwrap_err().code, "import.wrongKey");
        let (fmt, v) = parse_any(&file, Some(&key("demo", None))).unwrap();
        assert_eq!(fmt, "KeePass (KDBX)");
        assert_eq!(v.len(), 3, "la papelera no se importa");
        let val = |i: &Imported, k: &str| i.fields.iter().find(|f| f.key == k).map(|f| f.value.clone()).unwrap_or_default();
        let banco = v.iter().find(|i| i.title == "Banco").unwrap();
        assert_eq!(banco.folder.as_deref(), Some("Bancos/España"));
        assert_eq!(val(banco, "password"), "pw");
        assert_eq!(val(banco, "totp"), "otpauth://totp/KeePass?secret=JBSWY3DPEHPK3PXP&digits=8&period=30&algorithm=SHA1");
        assert!(banco.fields.iter().any(|f| f.label == "PIN" && f.secret && f.value == "1234"));
        assert!(!banco.fields.iter().any(|f| f.label.starts_with("TimeOtp")));
        assert_eq!(banco.tags, vec!["curro"]);
        assert_eq!((banco.files[0].name.as_str(), banco.files[0].data.as_slice()), ("contrato.txt", &b"hola"[..]));
        let gh = v.iter().find(|i| i.title == "github.com").unwrap();
        assert_eq!((gh.folder.as_deref(), val(gh, "totp").as_str()), (None, "otpauth://totp/x?secret=JBSWY3DPEHPK3PXP"));
        let nota = v.iter().find(|i| i.title == "Nota").unwrap();
        assert_eq!((nota.kind, val(nota, "content").as_str()), (Kind::Note, "secreto"));
    }

    #[test]
    fn kdbx_with_keyfile_only() {
        let kf = [7u8; 32];
        let file = sample(DatabaseKey::new().with_keyfile(&mut &kf[..]).unwrap());
        assert_eq!(parse_any(&file, Some(&key("", Some(&[1u8; 32])))).unwrap_err().code, "import.wrongKey");
        assert_eq!(parse_any(&file, Some(&key("", Some(&kf)))).unwrap().1.len(), 3);
    }
}
