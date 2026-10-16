//! Lo que hay dentro de una bóveda. Todos los valores de los campos son `Sv`: en disco van en claro dentro del
//! payload cifrado; en RAM, los secretos van sellados con el `Sealer` de la sesión.

use crate::crypto::Sealer;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_bytes::ByteBuf;
use zeroize::Zeroize;

/// Valor de un campo o adjunto.
pub enum Sv {
    Plain(Vec<u8>),
    Sealed(Vec<u8>),
}

impl Sv {
    pub fn text(s: &str) -> Self {
        Sv::Plain(s.as_bytes().to_vec())
    }
    pub fn is_sealed(&self) -> bool {
        matches!(self, Sv::Sealed(_))
    }
    /// Si es secreto y está en claro, lo sella (y borra el claro).
    pub fn seal(&mut self, s: &Sealer) {
        if let Sv::Plain(v) = self {
            let sealed = s.seal(v);
            v.zeroize();
            *self = Sv::Sealed(sealed);
        }
    }
    /// Copia en claro, se esté sellado o no.
    pub fn reveal(&self, s: &Sealer) -> zeroize::Zeroizing<Vec<u8>> {
        match self {
            Sv::Plain(v) => zeroize::Zeroizing::new(v.clone()),
            Sv::Sealed(v) => s.open(v),
        }
    }
    pub fn reveal_string(&self, s: &Sealer) -> zeroize::Zeroizing<String> {
        zeroize::Zeroizing::new(String::from_utf8_lossy(&self.reveal(s)).into_owned())
    }
    /// Solo para valores que no son secretos (siempre en claro)
    pub fn plain_str(&self) -> &str {
        match self {
            Sv::Plain(v) => std::str::from_utf8(v).unwrap_or(""),
            Sv::Sealed(_) => "",
        }
    }
    pub fn is_empty(&self) -> bool {
        match self {
            Sv::Plain(v) | Sv::Sealed(v) => v.is_empty(),
        }
    }
    /// Copia para el disco: siempre en claro.
    pub fn to_plain(&self, s: &Sealer) -> Sv {
        Sv::Plain(self.reveal(s).to_vec())
    }
    pub fn duplicate(&self) -> Sv {
        match self {
            Sv::Plain(v) => Sv::Plain(v.clone()),
            Sv::Sealed(v) => Sv::Sealed(v.clone()),
        }
    }
}

impl Drop for Sv {
    fn drop(&mut self) {
        match self {
            Sv::Plain(v) | Sv::Sealed(v) => v.zeroize(),
        }
    }
}

impl Serialize for Sv {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        match self {
            Sv::Plain(v) => ser.serialize_bytes(v),
            Sv::Sealed(_) => Err(serde::ser::Error::custom("valor sellado: hay que pasarlo a claro antes de guardar")),
        }
    }
}

impl<'de> Deserialize<'de> for Sv {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        Ok(Sv::Plain(ByteBuf::deserialize(de)?.into_vec()))
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Login,
    Card,
    Note,
    Identity,
    Wifi,
    Ssh,
    License,
    Bank,
    Document,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum FieldKind {
    Text,
    Password,
    Url,
    Email,
    Totp,
    Multiline,
    Date,
}

#[derive(Serialize, Deserialize)]
pub struct Field {
    pub id: String,
    /// Clave estable de las plantillas (`username`, `password`, `url`, `totp`, `notes`…) o vacía si es personalizado
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub label: String,
    pub value: Sv,
    pub secret: bool,
    pub kind: FieldKind,
}

impl Field {
    pub fn to_plain(&self, s: &Sealer) -> Field {
        Field { value: self.value.to_plain(s), ..self.shallow() }
    }
    pub fn duplicate(&self) -> Field {
        Field { value: self.value.duplicate(), ..self.shallow() }
    }
    fn shallow(&self) -> Field {
        Field {
            id: self.id.clone(),
            key: self.key.clone(),
            label: self.label.clone(),
            value: Sv::Plain(vec![]),
            secret: self.secret,
            kind: self.kind,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Attachment {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub data: Sv,
}

#[derive(Serialize, Deserialize)]
pub struct Snapshot {
    pub at: i64,
    pub title: String,
    pub fields: Vec<Field>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Breach {
    pub fp: ByteBuf,
    pub count: u32,
    pub checked: i64,
}

#[derive(Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub kind: Kind,
    pub title: String,
    #[serde(default)]
    pub folder: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub favorite: bool,
    pub fields: Vec<Field>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    #[serde(default)]
    pub icon: Option<ByteBuf>,
    #[serde(default)]
    pub autotype: Option<String>,
    #[serde(default)]
    pub history: Vec<Snapshot>,
    pub created: i64,
    pub modified: i64,
    #[serde(default)]
    pub used: Option<i64>,
    #[serde(default)]
    pub deleted: Option<i64>,
    #[serde(default)]
    pub breach: Option<Breach>,
    /// Archivada: fuera de la lista, la búsqueda, el auto-type y la salud, pero sin borrar
    #[serde(default)]
    pub archived: bool,
    /// Sensible: para ver, copiar o escribir sus secretos se vuelve a pedir Windows Hello (o la contraseña)
    #[serde(default)]
    pub protected: bool,
}

impl Entry {
    pub fn field(&self, key: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.key == key)
    }

    /// Sella todos los valores secretos (y los adjuntos), también los del historial.
    pub fn seal(&mut self, s: &Sealer) {
        let seal_fields = |fields: &mut Vec<Field>| {
            for f in fields.iter_mut().filter(|f| f.secret) {
                f.value.seal(s);
            }
        };
        seal_fields(&mut self.fields);
        for h in &mut self.history {
            seal_fields(&mut h.fields);
        }
        for a in &mut self.attachments {
            a.data.seal(s);
        }
    }

    /// Copia en claro, lista para serializar al disco.
    pub fn to_plain(&self, s: &Sealer) -> Entry {
        Entry {
            id: self.id.clone(),
            kind: self.kind,
            title: self.title.clone(),
            folder: self.folder.clone(),
            tags: self.tags.clone(),
            favorite: self.favorite,
            fields: self.fields.iter().map(|f| f.to_plain(s)).collect(),
            attachments: self
                .attachments
                .iter()
                .map(|a| Attachment {
                    id: a.id.clone(),
                    name: a.name.clone(),
                    mime: a.mime.clone(),
                    size: a.size,
                    data: a.data.to_plain(s),
                })
                .collect(),
            icon: self.icon.clone(),
            autotype: self.autotype.clone(),
            history: self
                .history
                .iter()
                .map(|h| Snapshot { at: h.at, title: h.title.clone(), fields: h.fields.iter().map(|f| f.to_plain(s)).collect() })
                .collect(),
            created: self.created,
            modified: self.modified,
            used: self.used,
            deleted: self.deleted,
            breach: self.breach.clone(),
            archived: self.archived,
            protected: self.protected,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Folder {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub icon: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct Vault {
    pub name: String,
    #[serde(default)]
    pub folders: Vec<Folder>,
    #[serde(default)]
    pub entries: Vec<Entry>,
    pub created: i64,
    pub modified: i64,
    /// Ids borrados para siempre: al fusionar con otra copia del archivo (Drive, otro PC) no resucitan
    #[serde(default)]
    pub tombstones: Vec<String>,
}

impl Vault {
    pub fn seal(&mut self, s: &Sealer) {
        for e in &mut self.entries {
            e.seal(s);
        }
    }
    pub fn to_plain(&self, s: &Sealer) -> Vault {
        Vault {
            name: self.name.clone(),
            folders: self.folders.clone(),
            entries: self.entries.iter().map(|e| e.to_plain(s)).collect(),
            created: self.created,
            modified: self.modified,
            tombstones: self.tombstones.clone(),
        }
    }

    /// Borra para siempre (y deja la lápida para las fusiones)
    pub fn purge(&mut self, keep: impl Fn(&Entry) -> bool) {
        let (gone, kept): (Vec<Entry>, Vec<Entry>) = std::mem::take(&mut self.entries).into_iter().partition(|e| !keep(e));
        self.tombstones.extend(gone.into_iter().map(|e| e.id.clone()));
        self.entries = kept;
    }
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}
