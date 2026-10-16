//! Formato `.poxipass`:
//! `"POXIPASS" | versión u16 LE | longitud cabecera u32 LE | cabecera JSON | XChaCha20-Poly1305(VaultKey, MessagePack)`.
//! Todo lo anterior al payload va como AAD: tocar la cabecera rompe el descifrado.

use super::model::Vault;
use crate::crypto::{self, derive_password_key, hkdf_key, random, random_key, KdfParams, Key};
use crate::error::{AppError, Res};
use crate::recovery::{self, Phrase};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

const MAGIC: &[u8; 8] = b"POXIPASS";
const VERSION: u16 = 1;
const MAX_HEADER: usize = 64 * 1024;
const KEEP_BACKUPS: usize = 20;
const BACKUP_EVERY_SECS: u64 = 3600;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Slot {
    /// `password` o `recovery`
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub salt: Option<String>,
    /// base64(nonce || Vault Key cifrada)
    pub wrapped: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Header {
    pub vault_id: String,
    pub created: i64,
    pub kdf: KdfParams,
    pub slots: Vec<Slot>,
}

/// SHA-256 del archivo tal y como lo dejamos (para saber si alguien lo ha cambiado por fuera)
pub type Fingerprint = [u8; 32];

pub struct Opened {
    pub header: Header,
    pub key: Key,
    pub vault: Vault,
    pub fp: Fingerprint,
}

fn slot_aad(id: &str, vault_id: &str) -> Vec<u8> {
    format!("poxipass-slot:{id}:{vault_id}").into_bytes()
}

fn recovery_kek(entropy: &[u8; 32], vault_id: &str) -> Key {
    hkdf_key(entropy, vault_id.as_bytes(), b"poxipass-recovery")
}

fn put_slot(header: &mut Header, slot: Slot) {
    header.slots.retain(|s| s.id != slot.id);
    header.slots.push(slot);
}

fn wrap(kek: &Key, key: &Key, id: &str, vault_id: &str, salt: Option<String>) -> Slot {
    Slot { id: id.into(), salt, wrapped: B64.encode(crypto::seal(kek, key.as_ref(), &slot_aad(id, vault_id))) }
}

fn unwrap(header: &Header, id: &str, kek: &Key) -> Option<Key> {
    let slot = header.slots.iter().find(|s| s.id == id)?;
    let data = B64.decode(&slot.wrapped).ok()?;
    let raw = crypto::open(kek, &data, &slot_aad(id, &header.vault_id))?;
    let mut k = Zeroizing::new([0u8; 32]);
    k.copy_from_slice(raw.get(..32)?);
    Some(k)
}

/// Pone (o cambia) la cerradura de la contraseña maestra.
pub fn set_password(header: &mut Header, key: &Key, password: &str) -> Res<()> {
    let salt = random::<16>();
    let kek = derive_password_key(password, &salt, &header.kdf)?;
    let slot = wrap(&kek, key, "password", &header.vault_id, Some(B64.encode(salt)));
    put_slot(header, slot);
    Ok(())
}

/// Pone una cerradura de recuperación nueva (el kit anterior deja de valer).
pub fn set_recovery(header: &mut Header, key: &Key) -> Phrase {
    let phrase = recovery::generate();
    let slot = wrap(&recovery_kek(&phrase.entropy, &header.vault_id), key, "recovery", &header.vault_id, None);
    put_slot(header, slot);
    phrase
}

pub fn encode(header: &Header, key: &Key, vault: &Vault) -> Res<Vec<u8>> {
    let json = serde_json::to_vec(header).map_err(|_| AppError::new("vault.corrupt"))?;
    let mut out = Vec::with_capacity(json.len() + 14);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(&json);
    let plain = Zeroizing::new(rmp_serde::to_vec_named(vault).map_err(|_| AppError::new("vault.corrupt"))?);
    let payload = crypto::seal(key, &plain, &out);
    out.extend_from_slice(&payload);
    Ok(out)
}

/// Lee la cabecera. Devuelve también dónde empieza el payload.
pub fn read_header(bytes: &[u8]) -> Res<(Header, usize)> {
    if bytes.len() < 14 || &bytes[..8] != MAGIC {
        return Err(AppError::new("vault.notPoxipass"));
    }
    let version = u16::from_le_bytes([bytes[8], bytes[9]]);
    if version > VERSION {
        return Err(AppError::new("vault.newerVersion"));
    }
    let len = u32::from_le_bytes(bytes[10..14].try_into().unwrap()) as usize;
    if len > MAX_HEADER || bytes.len() < 14 + len {
        return Err(AppError::new("vault.corrupt"));
    }
    let header = serde_json::from_slice(&bytes[14..14 + len]).map_err(|_| AppError::new("vault.corrupt"))?;
    Ok((header, 14 + len))
}

fn read_payload(bytes: &[u8], start: usize, key: &Key) -> Res<Vault> {
    let plain = crypto::open(key, &bytes[start..], &bytes[..start]).ok_or_else(|| AppError::new("vault.corrupt"))?;
    rmp_serde::from_slice(&plain).map_err(|_| AppError::new("vault.corrupt"))
}

/// Abre unos bytes de llavero con una Vault Key que ya tenemos (para fusionar con la versión del disco).
pub fn decode_with_key(bytes: &[u8], key: &Key) -> Res<(Header, Vault)> {
    let (header, start) = read_header(bytes)?;
    let vault = read_payload(bytes, start, key)?;
    Ok((header, vault))
}

pub fn fingerprint(bytes: &[u8]) -> Fingerprint {
    Sha256::digest(bytes).into()
}

pub fn open_with_password(path: &Path, password: &str) -> Res<Opened> {
    let bytes = fs::read(path)?;
    let (header, start) = read_header(&bytes)?;
    let slot = header.slots.iter().find(|s| s.id == "password").ok_or_else(|| AppError::new("vault.corrupt"))?;
    let salt = slot.salt.as_deref().and_then(|s| B64.decode(s).ok()).ok_or_else(|| AppError::new("vault.corrupt"))?;
    let kek = derive_password_key(password, &salt, &header.kdf)?;
    let key = unwrap(&header, "password", &kek).ok_or_else(|| AppError::new("vault.wrongPassword"))?;
    let vault = read_payload(&bytes, start, &key)?;
    Ok(Opened { header, key, vault, fp: fingerprint(&bytes) })
}

pub fn open_with_recovery(path: &Path, entropy: &[u8; 32]) -> Res<Opened> {
    let bytes = fs::read(path)?;
    let (header, start) = read_header(&bytes)?;
    let key = unwrap(&header, "recovery", &recovery_kek(entropy, &header.vault_id))
        .ok_or_else(|| AppError::new("recovery.wrongKit"))?;
    let vault = read_payload(&bytes, start, &key)?;
    Ok(Opened { header, key, vault, fp: fingerprint(&bytes) })
}

/// Abre con una Vault Key que ya tenemos (Windows Hello).
pub fn open_with_key(path: &Path, key: Key) -> Res<Opened> {
    let bytes = fs::read(path)?;
    let (header, start) = read_header(&bytes)?;
    let vault = read_payload(&bytes, start, &key)?;
    Ok(Opened { header, key, vault, fp: fingerprint(&bytes) })
}

/// Bóveda nueva. Falla si el archivo ya existe.
pub fn create(path: &Path, vault: Vault, password: &str, kdf: KdfParams, backups: &Path) -> Res<(Opened, Phrase)> {
    if path.exists() {
        return Err(AppError::new("vault.exists"));
    }
    let key = random_key();
    let mut header = Header { vault_id: super::model::new_id(), created: vault.created, kdf, slots: vec![] };
    set_password(&mut header, &key, password)?;
    let phrase = set_recovery(&mut header, &key);
    let fp = write(path, &encode(&header, &key, &vault)?, None, backups)?;
    Ok((Opened { header, key, vault, fp }, phrase))
}

/// Escribe de forma atómica (temporal + renombrar). Si `expected` no coincide con lo que hay en disco, no toca
/// nada y devuelve `vault.changedOnDisk`. Antes de sobrescribir guarda una copia (como mucho una por hora).
pub fn write(path: &Path, bytes: &[u8], expected: Option<Fingerprint>, backups: &Path) -> Res<Fingerprint> {
    if path.exists() {
        let current = fs::read(path)?;
        if expected.is_some_and(|fp| fp != fingerprint(&current)) {
            return Err(AppError::new("vault.changedOnDisk"));
        }
        backup(&current, backups, false)?;
    } else if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("poxipass.tmp");
    let result = (|| -> std::io::Result<()> {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, path)
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(fingerprint(bytes))
}

pub fn list_backups(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir)
        .map(|it| it.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|x| x == "poxipass")).collect())
        .unwrap_or_default();
    v.sort();
    v
}

/// Copia del archivo actual. Sin `force`, como mucho una por hora.
pub fn backup(current: &[u8], dir: &Path, force: bool) -> Res<()> {
    fs::create_dir_all(dir)?;
    let existing = list_backups(dir);
    let recent = existing
        .last()
        .and_then(|p| p.metadata().ok()?.modified().ok()?.elapsed().ok())
        .is_some_and(|age| age.as_secs() < BACKUP_EVERY_SECS);
    if recent && !force {
        return Ok(());
    }
    let mut name = crate::util::stamp(crate::util::now());
    // Dos copias en el mismo segundo (solo pasa en tests): sufijo
    while dir.join(format!("{name}.poxipass")).exists() {
        name.push('b');
    }
    fs::write(dir.join(format!("{name}.poxipass")), current)?;
    let all = list_backups(dir);
    for old in all.iter().take(all.len().saturating_sub(KEEP_BACKUPS)) {
        let _ = fs::remove_file(old);
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::vault::model::*;

    pub const FAST: KdfParams = KdfParams { m: 1024, t: 1, p: 1 };

    pub fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("poxipass-test-{tag}-{}", new_id()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn sample() -> Vault {
        Vault {
            name: "Prueba".into(),
            entries: vec![Entry {
                id: new_id(),
                kind: Kind::Login,
                title: "Netflix".into(),
                folder: None,
                tags: vec![],
                favorite: false,
                fields: vec![Field {
                    id: new_id(),
                    key: "password".into(),
                    label: String::new(),
                    value: Sv::text("hunter2"),
                    secret: true,
                    kind: FieldKind::Password,
                }],
                attachments: vec![],
                icon: None,
                autotype: None,
                history: vec![],
                created: 1,
                modified: 1,
                used: None,
                deleted: None,
                breach: None,
                archived: false,
                protected: false,
            }],
            created: 1,
            modified: 1,
            ..Default::default()
        }
    }

    fn password_of(v: &Vault) -> String {
        v.entries[0].fields[0].value.plain_str().to_string()
    }

    #[test]
    fn create_open_and_wrong_password() {
        let d = tmpdir("open");
        let path = d.join("b.poxipass");
        let (_, phrase) = create(&path, sample(), "Contraseña buena", FAST, &d.join("bk")).unwrap();
        assert_eq!(password_of(&open_with_password(&path, "Contraseña buena").unwrap().vault), "hunter2");
        assert_eq!(open_with_password(&path, "mala").err().unwrap().code, "vault.wrongPassword");
        assert_eq!(password_of(&open_with_recovery(&path, &phrase.entropy).unwrap().vault), "hunter2");
        assert_eq!(open_with_recovery(&path, &[0u8; 32]).err().unwrap().code, "recovery.wrongKit");
        assert_eq!(create(&path, sample(), "x", FAST, &d.join("bk")).err().unwrap().code, "vault.exists");
    }

    #[test]
    fn tampering_and_truncation_never_open() {
        let d = tmpdir("tamper");
        let path = d.join("b.poxipass");
        create(&path, sample(), "pw", FAST, &d.join("bk")).unwrap();
        let good = fs::read(&path).unwrap();
        // Un byte de la fecha de creación de la cabecera (la cerradura sigue valiendo, el payload no)
        let json_start = 14;
        let pos = json_start + good[json_start..].windows(9).position(|w| w == b"\"created\"").unwrap() + 10;
        let mut bad = good.clone();
        bad[pos] = if bad[pos] == b'1' { b'2' } else { b'1' };
        fs::write(&path, &bad).unwrap();
        assert!(open_with_password(&path, "pw").is_err());
        // Último byte del payload
        let mut bad = good.clone();
        *bad.last_mut().unwrap() ^= 0x80;
        fs::write(&path, &bad).unwrap();
        assert_eq!(open_with_password(&path, "pw").err().unwrap().code, "vault.corrupt");
        fs::write(&path, &good[..good.len() / 2]).unwrap();
        assert!(open_with_password(&path, "pw").is_err());
        fs::write(&path, b"hola").unwrap();
        assert_eq!(open_with_password(&path, "pw").err().unwrap().code, "vault.notPoxipass");
    }

    #[test]
    fn password_change_keeps_data_and_kit() {
        let d = tmpdir("rewrap");
        let path = d.join("b.poxipass");
        let (mut o, phrase) = create(&path, sample(), "vieja", FAST, &d.join("bk")).unwrap();
        set_password(&mut o.header, &o.key, "nueva").unwrap();
        write(&path, &encode(&o.header, &o.key, &o.vault).unwrap(), Some(o.fp), &d.join("bk")).unwrap();
        assert_eq!(open_with_password(&path, "vieja").err().unwrap().code, "vault.wrongPassword");
        assert!(open_with_password(&path, "nueva").is_ok());
        assert!(open_with_recovery(&path, &phrase.entropy).is_ok());
    }

    #[test]
    fn changed_on_disk_is_never_overwritten() {
        let d = tmpdir("conflict");
        let path = d.join("b.poxipass");
        let (o, _) = create(&path, sample(), "pw", FAST, &d.join("bk")).unwrap();
        fs::write(&path, b"otra version de Drive").unwrap();
        let bytes = encode(&o.header, &o.key, &o.vault).unwrap();
        assert_eq!(write(&path, &bytes, Some(o.fp), &d.join("bk")).err().unwrap().code, "vault.changedOnDisk");
        assert_eq!(fs::read(&path).unwrap(), b"otra version de Drive");
    }

    #[test]
    fn failed_write_keeps_original() {
        let d = tmpdir("fail");
        let path = d.join("b.poxipass");
        let (o, _) = create(&path, sample(), "pw", FAST, &d.join("bk")).unwrap();
        let before = fs::read(&path).unwrap();
        // El temporal no se puede crear: hay una carpeta con su nombre
        fs::create_dir_all(path.with_extension("poxipass.tmp")).unwrap();
        let bytes = encode(&o.header, &o.key, &o.vault).unwrap();
        assert!(write(&path, &bytes, Some(o.fp), &d.join("bk")).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
    }

    #[test]
    fn backups_rotate_and_wait_an_hour() {
        let d = tmpdir("backup");
        let bk = d.join("bk");
        backup(b"1", &bk, false).unwrap();
        backup(b"2", &bk, false).unwrap();
        assert_eq!(list_backups(&bk).len(), 1, "la segunda llega antes de una hora");
        for i in 0..30 {
            fs::write(bk.join(format!("2000010{}-0000{:02}.poxipass", i % 9, i)), b"x").unwrap();
        }
        // La más nueva por nombre es la de hoy y es reciente: se fuerza una vieja como última
        fs::remove_file(list_backups(&bk).last().unwrap()).unwrap();
        let newest = list_backups(&bk).last().unwrap().clone();
        let f = fs::File::options().write(true).open(&newest).unwrap();
        f.set_modified(std::time::SystemTime::UNIX_EPOCH).unwrap();
        drop(f);
        backup(b"3", &bk, false).unwrap();
        assert_eq!(list_backups(&bk).len(), KEEP_BACKUPS);
        assert_eq!(fs::read(list_backups(&bk).last().unwrap()).unwrap(), b"3");
    }
}
