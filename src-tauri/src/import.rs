//! Importar desde Chrome/Edge, Firefox, Bitwarden (CSV y JSON) y KeePass/KeePassXC. Un solo lector de CSV que
//! reconoce las columnas por su nombre, así que también vale para CSV «a mano». Exporta en el CSV de Bitwarden,
//! que entienden casi todos los gestores.

use crate::error::{AppError, Res};
use crate::vault::model::{FieldKind, Kind};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub struct ImportedField {
    pub key: &'static str,
    pub label: String,
    pub value: String,
    pub secret: bool,
    pub kind: FieldKind,
}

#[derive(Debug, Clone)]
pub struct Imported {
    pub kind: Kind,
    pub title: String,
    pub folder: Option<String>,
    pub favorite: bool,
    pub fields: Vec<ImportedField>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub format: String,
    pub count: usize,
    pub sample: Vec<String>,
}

fn f(key: &'static str, value: &str) -> Option<ImportedField> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let (secret, kind) = match key {
        "password" => (true, FieldKind::Password),
        "totp" => (true, FieldKind::Totp),
        "notes" | "content" => (true, FieldKind::Multiline),
        "url" => (false, FieldKind::Url),
        "email" => (false, FieldKind::Email),
        "number" | "cvv" | "pin" | "document" => (true, FieldKind::Password),
        _ => (false, FieldKind::Text),
    };
    Some(ImportedField { key, label: String::new(), value: value.into(), secret, kind })
}

fn custom(label: &str, value: &str, secret: bool) -> Option<ImportedField> {
    let value = value.trim();
    (!value.is_empty()).then(|| ImportedField {
        key: "",
        label: label.trim().chars().take(100).collect(),
        value: value.into(),
        secret,
        kind: if secret { FieldKind::Password } else { FieldKind::Text },
    })
}

fn host_title(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or(url);
    rest.split(['/', '?', '#', ':']).next().unwrap_or("").trim_start_matches("www.").to_string()
}

/// Lee un archivo (CSV o JSON de Bitwarden). Quita el BOM y acepta `,` o `;` como separador.
pub fn parse(bytes: &[u8]) -> Res<(String, Vec<Imported>)> {
    let text = String::from_utf8_lossy(bytes);
    let text = text.trim_start_matches('\u{feff}');
    if text.trim_start().starts_with('{') {
        return Ok(("Bitwarden (JSON)".into(), bitwarden_json(text)?));
    }
    csv_any(text)
}

fn csv_any(text: &str) -> Res<(String, Vec<Imported>)> {
    let first = text.lines().next().unwrap_or("");
    let delim = if first.matches(';').count() > first.matches(',').count() { b';' } else { b',' };
    let mut rdr = csv::ReaderBuilder::new().delimiter(delim).flexible(true).from_reader(text.as_bytes());
    let headers: Vec<String> =
        rdr.headers().map_err(|_| AppError::new("import.unknown"))?.iter().map(|h| h.trim().to_lowercase()).collect();
    let col = |names: &[&str]| headers.iter().position(|h| names.contains(&h.as_str()));
    let c_title = col(&["name", "title", "account", "nombre", "título", "titulo"]);
    let c_user = col(&["login_username", "username", "login name", "user", "usuario", "email"]);
    let c_pass = col(&["login_password", "password", "contraseña", "pass"]);
    let c_url = col(&["login_uri", "url", "web site", "website", "uri", "sitio"]);
    let c_notes = col(&["notes", "note", "comments", "notas"]);
    let c_totp = col(&["login_totp", "totp", "otp", "otpauth"]);
    let c_folder = col(&["folder", "group", "carpeta", "grupo"]);
    let c_fav = col(&["favorite"]);
    let c_type = col(&["type"]);
    if c_pass.is_none() && c_notes.is_none() {
        return Err(AppError::new("import.unknown"));
    }
    let has = |h: &str| headers.iter().any(|x| x == h);
    let format = if has("login_uri") {
        "Bitwarden (CSV)"
    } else if has("httprealm") {
        "Firefox"
    } else if has("group") && has("title") {
        "KeePassXC"
    } else if has("login name") {
        "KeePass"
    } else if has("name") && has("url") && has("username") {
        "Chrome / Edge"
    } else {
        "CSV"
    };
    let mut out = vec![];
    for rec in rdr.records() {
        let rec = rec.map_err(|_| AppError::new("import.badFile"))?;
        let get = |c: Option<usize>| c.and_then(|i| rec.get(i)).unwrap_or("").to_string();
        let (url, notes) = (get(c_url), get(c_notes));
        let is_note = get(c_type).eq_ignore_ascii_case("note");
        let mut title = get(c_title).trim().to_string();
        if title.is_empty() {
            title = host_title(&url);
        }
        // KeePassXC exporta la carpeta raíz como «Root/…»
        let folder = get(c_folder).trim().trim_start_matches("Root").trim_start_matches('/').to_string();
        let fields: Vec<ImportedField> = if is_note {
            [f("content", &notes)].into_iter().flatten().collect()
        } else {
            [f("username", &get(c_user)), f("password", &get(c_pass)), f("url", &url), f("totp", &get(c_totp)), f("notes", &notes)]
                .into_iter()
                .flatten()
                .collect()
        };
        if fields.is_empty() && title.is_empty() {
            continue;
        }
        out.push(Imported {
            kind: if is_note { Kind::Note } else { Kind::Login },
            title: if title.is_empty() { "Sin título".into() } else { title },
            folder: (!folder.is_empty()).then_some(folder),
            favorite: matches!(get(c_fav).trim(), "1" | "true"),
            fields,
        });
    }
    Ok((format.into(), out))
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}

fn bitwarden_json(text: &str) -> Res<Vec<Imported>> {
    let root: Value = serde_json::from_str(text).map_err(|_| AppError::new("import.badFile"))?;
    if root.get("encrypted").and_then(Value::as_bool) == Some(true) {
        return Err(AppError::new("import.encrypted"));
    }
    let folders: std::collections::HashMap<&str, &str> = root["folders"]
        .as_array()
        .map(|a| a.iter().map(|f| (s(f, "id"), s(f, "name"))).collect())
        .unwrap_or_default();
    let items = root["items"].as_array().ok_or_else(|| AppError::new("import.unknown"))?;
    Ok(items
        .iter()
        .map(|it| {
            let notes = s(it, "notes");
            let (kind, mut fields): (Kind, Vec<Option<ImportedField>>) = match it["type"].as_i64() {
                Some(3) => {
                    let c = &it["card"];
                    let exp = format!("{}/{}", s(c, "expMonth"), s(c, "expYear"));
                    (
                        Kind::Card,
                        vec![f("holder", s(c, "cardholderName")), f("number", s(c, "number")), f("expiry", exp.trim_matches('/')), f("cvv", s(c, "code")), f("notes", notes)],
                    )
                }
                Some(4) => {
                    let i = &it["identity"];
                    let name = format!("{} {}", s(i, "firstName"), s(i, "lastName"));
                    let addr = [s(i, "address1"), s(i, "postalCode"), s(i, "city"), s(i, "country")]
                        .iter()
                        .filter(|x| !x.is_empty())
                        .copied()
                        .collect::<Vec<_>>()
                        .join(", ");
                    let doc = [s(i, "ssn"), s(i, "passportNumber"), s(i, "licenseNumber")].into_iter().find(|x| !x.is_empty()).unwrap_or("");
                    (
                        Kind::Identity,
                        vec![f("fullname", &name), f("email", s(i, "email")), f("phone", s(i, "phone")), f("address", &addr), f("document", doc), f("notes", notes)],
                    )
                }
                Some(2) => (Kind::Note, vec![f("content", notes)]),
                _ => {
                    let l = &it["login"];
                    let url = l["uris"].as_array().and_then(|u| u.first()).map(|u| s(u, "uri")).unwrap_or("");
                    (
                        Kind::Login,
                        vec![f("username", s(l, "username")), f("password", s(l, "password")), f("url", url), f("totp", s(l, "totp")), f("notes", notes)],
                    )
                }
            };
            for cf in it["fields"].as_array().into_iter().flatten() {
                fields.push(custom(s(cf, "name"), &cf["value"].as_str().map(String::from).unwrap_or_default(), cf["type"].as_i64() == Some(1)));
            }
            Imported {
                kind,
                title: s(it, "name").to_string(),
                folder: it["folderId"].as_str().and_then(|id| folders.get(id)).map(|n| n.to_string()),
                favorite: it["favorite"].as_bool().unwrap_or(false),
                fields: fields.into_iter().flatten().collect(),
            }
        })
        .collect())
}

pub fn preview(bytes: &[u8]) -> Res<Preview> {
    let (format, items) = parse(bytes)?;
    Ok(Preview { format, count: items.len(), sample: items.iter().take(5).map(|i| i.title.clone()).collect() })
}

/// Fila de exportación ya en claro: carpeta, favorito, tipo, título, campos (clave, etiqueta, valor).
pub struct ExportRow {
    pub folder: String,
    pub favorite: bool,
    pub kind: Kind,
    pub title: String,
    pub fields: Vec<(String, String, String)>,
}

pub fn export_csv(rows: &[ExportRow]) -> Res<Vec<u8>> {
    let mut w = csv::Writer::from_writer(vec![]);
    let err = |_| AppError::new("io.failed");
    w.write_record(["folder", "favorite", "type", "name", "notes", "fields", "reprompt", "login_uri", "login_username", "login_password", "login_totp"])
        .map_err(err)?;
    for r in rows {
        let get = |k: &str| r.fields.iter().find(|(key, _, _)| key == k).map(|(_, _, v)| v.as_str()).unwrap_or("");
        let main = ["username", "password", "url", "totp", "notes", "content"];
        let extra: Vec<String> =
            r.fields.iter().filter(|(k, _, _)| !main.contains(&k.as_str())).map(|(k, l, v)| format!("{}: {v}", if l.is_empty() { k } else { l })).collect();
        let login = r.kind == Kind::Login;
        let notes = if get("notes").is_empty() { get("content") } else { get("notes") };
        w.write_record([
            r.folder.as_str(),
            if r.favorite { "1" } else { "" },
            if login { "login" } else { "note" },
            r.title.as_str(),
            notes,
            &extra.join("\n"),
            "0",
            get("url"),
            get("username"),
            get("password"),
            get("totp"),
        ])
        .map_err(err)?;
    }
    w.into_inner().map_err(|_| AppError::new("io.failed"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn val<'a>(i: &'a Imported, k: &str) -> &'a str {
        i.fields.iter().find(|f| f.key == k).map(|f| f.value.as_str()).unwrap_or("")
    }

    #[test]
    fn chrome() {
        let (fmt, v) = parse(b"name,url,username,password,note\nNetflix,https://www.netflix.com/login,poxi@x.com,hunter2,\n,https://www.twitch.tv/,poxiitv,abc,nota").unwrap();
        assert_eq!(fmt, "Chrome / Edge");
        assert_eq!(v.len(), 2);
        assert_eq!((v[0].title.as_str(), val(&v[0], "password")), ("Netflix", "hunter2"));
        assert_eq!(v[1].title, "twitch.tv");
        assert_eq!(val(&v[1], "notes"), "nota");
    }

    #[test]
    fn firefox() {
        let (fmt, v) = parse(b"\"url\",\"username\",\"password\",\"httpRealm\",\"formActionOrigin\",\"guid\",\"timeCreated\",\"timeLastUsed\",\"timePasswordChanged\"\n\"https://accounts.google.com\",\"yo\",\"pw\",,\"\",\"{1}\",\"1\",\"1\",\"1\"").unwrap();
        assert_eq!((fmt.as_str(), v[0].title.as_str(), val(&v[0], "username")), ("Firefox", "accounts.google.com", "yo"));
    }

    #[test]
    fn bitwarden_csv_with_bom_note_and_folder() {
        let (fmt, v) = parse("\u{feff}folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp\nCurro,1,login,GitHub,,,0,https://github.com,poxi,pw,JBSWY3DPEHPK3PXP\n,,note,Secreto,\"línea 1\nlínea 2\",,0,,,,".as_bytes()).unwrap();
        assert_eq!(fmt, "Bitwarden (CSV)");
        assert_eq!(v[0].folder.as_deref(), Some("Curro"));
        assert!(v[0].favorite);
        assert_eq!(val(&v[0], "totp"), "JBSWY3DPEHPK3PXP");
        assert_eq!(v[1].kind, Kind::Note);
        assert_eq!(val(&v[1], "content"), "línea 1\nlínea 2");
    }

    #[test]
    fn keepassxc_and_semicolon_excel() {
        let (fmt, v) = parse(b"\"Group\",\"Title\",\"Username\",\"Password\",\"URL\",\"Notes\",\"TOTP\"\n\"Root/Bancos\",\"Banco\",\"yo\",\"pw\",\"\",\"\",\"\"").unwrap();
        assert_eq!((fmt.as_str(), v[0].folder.as_deref()), ("KeePassXC", Some("Bancos")));
        let (_, v) = parse("\u{feff}nombre;usuario;contraseña;url\nCorreo;yo;p,q;https://mail.com".as_bytes()).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(val(&v[0], "password"), "p,q");
    }

    #[test]
    fn bitwarden_json_all_kinds() {
        let json = r#"{"encrypted":false,"folders":[{"id":"f1","name":"Personal"}],"items":[
          {"type":1,"name":"Google","folderId":"f1","favorite":true,"login":{"username":"yo","password":"pw","uris":[{"uri":"https://google.com"}]},"fields":[{"name":"PIN","value":"1234","type":1}]},
          {"type":3,"name":"Visa","card":{"cardholderName":"Poxi","number":"4111111111111111","expMonth":"12","expYear":"2030","code":"123"}},
          {"type":4,"name":"Yo","identity":{"firstName":"Poxi","lastName":"TV","email":"a@b.c","passportNumber":"X1"}},
          {"type":2,"name":"Nota","notes":"hola"}]}"#;
        let (_, v) = parse(json.as_bytes()).unwrap();
        assert_eq!(v.len(), 4);
        assert_eq!((v[0].folder.as_deref(), v[0].favorite), (Some("Personal"), true));
        assert!(v[0].fields.iter().any(|f| f.label == "PIN" && f.secret));
        assert_eq!((v[1].kind, val(&v[1], "number"), val(&v[1], "expiry")), (Kind::Card, "4111111111111111", "12/2030"));
        assert_eq!((v[2].kind, val(&v[2], "document")), (Kind::Identity, "X1"));
        assert_eq!(val(&v[3], "content"), "hola");
        assert_eq!(parse(br#"{"encrypted":true}"#).unwrap_err().code, "import.encrypted");
    }

    #[test]
    fn unknown_and_export_roundtrip() {
        assert_eq!(parse(b"a,b\n1,2").unwrap_err().code, "import.unknown");
        let rows = vec![ExportRow {
            folder: "Curro".into(),
            favorite: true,
            kind: Kind::Login,
            title: "Git, Hub".into(),
            fields: vec![("username".into(), "".into(), "yo".into()), ("password".into(), "".into(), "p\"w".into()), ("".into(), "PIN".into(), "1".into())],
        }];
        let (fmt, v) = parse(&export_csv(&rows).unwrap()).unwrap();
        assert_eq!(fmt, "Bitwarden (CSV)");
        assert_eq!((v[0].title.as_str(), val(&v[0], "password")), ("Git, Hub", "p\"w"));
    }
}
