//! `.1pux` de 1Password 8: un zip con `export.data` (JSON) y los adjuntos en `files/<documentId>__<nombre>`.

use super::{custom, f, host_title, s, Imported, ImportedField, ImportedFile};
use crate::commands::MAX_ATTACHMENT;
use crate::error::{AppError, Res};
use crate::util::stamp;
use crate::vault::model::Kind;
use serde_json::Value;
use std::io::{Cursor, Read};
use zeroize::Zeroizing;

type Zip<'a> = zip::ZipArchive<Cursor<&'a [u8]>>;

/// Campos de 1Password con sitio propio en PoxiPass: (categoría o «*», id en 1Password, campo).
/// Lo que no está aquí entra como campo personalizado, así que no se pierde nada.
const MAP: &[(&str, &str, &str)] = &[
    ("002", "cardholder", "holder"),
    ("002", "ccnum", "number"),
    ("002", "cvv", "cvv"),
    ("002", "expiry", "expiry"),
    ("002", "pin", "pin"),
    ("004", "email", "email"),
    ("004", "defphone", "phone"),
    ("004", "cellphone", "phone"),
    ("004", "address", "address"),
    ("100", "reg_code", "licensekey"),
    ("100", "reg_email", "email"),
    ("100", "order_date", "purchased"),
    ("101", "bankName", "bank"),
    ("101", "owner", "holder"),
    ("101", "iban", "iban"),
    ("101", "swift", "swift"),
    ("101", "telephonePin", "pin"),
    ("103", "fullname", "holder"),
    ("103", "number", "number"),
    ("103", "expiry_date", "expiry"),
    ("106", "fullname", "holder"),
    ("106", "number", "number"),
    ("106", "issue_date", "issued"),
    ("106", "expiry_date", "expiry"),
    ("108", "name", "holder"),
    ("108", "number", "number"),
    ("109", "network_name", "ssid"),
    ("109", "wireless_password", "password"),
    ("109", "wireless_security", "security"),
    ("*", "username", "username"),
    ("*", "password", "password"),
];

fn kind_of(category: &str) -> Kind {
    match category {
        "002" => Kind::Card,
        "003" | "006" => Kind::Note,
        "004" => Kind::Identity,
        "100" => Kind::License,
        "101" => Kind::Bank,
        "103" | "106" | "108" => Kind::Document,
        "109" => Kind::Wifi,
        "114" => Kind::Ssh,
        _ => Kind::Login,
    }
}

pub fn parse(bytes: &[u8]) -> Res<Vec<Imported>> {
    let bad = || AppError::new("import.badFile");
    let mut zip = Zip::new(Cursor::new(bytes)).map_err(|_| bad())?;
    let mut json = Zeroizing::new(String::new());
    zip.by_name("export.data")
        .map_err(|_| AppError::new("import.unknown"))?
        .take(64 * 1024 * 1024)
        .read_to_string(&mut json)
        .map_err(|_| bad())?;
    let root: Value = serde_json::from_str(&json).map_err(|_| bad())?;
    let names: Vec<String> = zip.file_names().filter(|n| n.starts_with("files/")).map(String::from).collect();
    let mut out = vec![];
    for vault in root["accounts"].as_array().into_iter().flatten().flat_map(|a| a["vaults"].as_array().into_iter().flatten()) {
        let folder = s(&vault["attrs"], "name");
        for it in vault["items"].as_array().into_iter().flatten() {
            out.extend(item(it, folder, &mut |id, name| file(&mut zip, &names, id, name)));
        }
    }
    Ok(out)
}

/// Adjunto `files/<documentId>__<nombre>`. Lee como mucho un byte más del límite: si se pasa, al importarlo se
/// omite por tamaño (y no se llena la memoria con un archivo enorme).
fn file(zip: &mut Zip, names: &[String], id: &str, name: &str) -> Option<ImportedFile> {
    let path = names.iter().find(|n| !id.is_empty() && n.starts_with(&format!("files/{id}")))?;
    let mut data = vec![];
    zip.by_name(path).ok()?.take(MAX_ATTACHMENT + 1).read_to_end(&mut data).ok()?;
    Some(ImportedFile { name: if name.is_empty() { path.rsplit("__").next().unwrap_or(path).into() } else { name.into() }, data })
}

enum Val {
    Text(String, bool),
    Totp(String),
    File(String, String),
    Ssh(String, String),
    Skip,
}

/// Un valor de 1Password es un objeto con una sola clave que dice su tipo: `{"concealed": "…"}`, `{"date": 1700000000}`…
fn value(v: &Value) -> Val {
    let Some((t, x)) = v.as_object().and_then(|o| o.iter().next()) else { return Val::Skip };
    let text = || x.as_str().unwrap_or("").to_string();
    match t.as_str() {
        "concealed" | "creditCardNumber" => Val::Text(text(), true),
        "totp" => Val::Totp(text()),
        "email" => Val::Text(x.as_str().unwrap_or(s(x, "email_address")).into(), false),
        "date" => Val::Text(
            x.as_i64()
                .map(|t| {
                    let d = stamp(t);
                    format!("{}-{}-{}", &d[..4], &d[4..6], &d[6..8])
                })
                .unwrap_or_default(),
            false,
        ),
        "monthYear" => Val::Text(x.as_i64().map(|n| format!("{:02}/{}", n % 100, n / 100)).unwrap_or_default(), false),
        "address" => Val::Text(
            ["street", "zip", "city", "state", "country"].iter().map(|k| s(x, k)).filter(|p| !p.is_empty()).collect::<Vec<_>>().join(", "),
            false,
        ),
        // `metadata.privateKey` (PKCS#8) es el fiable: el `privateKey` de fuera no siempre viene en el mismo formato
        "sshKey" => {
            let m = &x["metadata"];
            Val::Ssh(if s(m, "privateKey").is_empty() { s(x, "privateKey") } else { s(m, "privateKey") }.into(), s(m, "publicKey").into())
        }
        "file" => Val::File(s(x, "documentId").into(), s(x, "fileName").into()),
        "reference" => Val::Skip,
        _ => Val::Text(text(), false),
    }
}

/// Campos de la entrada: cada valor va a su campo si está libre; si no (o no tiene), como campo personalizado.
#[derive(Default)]
struct Fields(Vec<ImportedField>);

impl Fields {
    fn put(&mut self, key: Option<&'static str>, label: &str, value: &str, secret: bool) {
        let field = match key {
            Some(k) if !self.0.iter().any(|x| x.key == k) => f(k, value),
            _ => custom(label, value, secret),
        };
        self.0.extend(field);
    }
}

fn item(it: &Value, folder: &str, file: &mut dyn FnMut(&str, &str) -> Option<ImportedFile>) -> Option<Imported> {
    let state = s(it, "state");
    if !matches!(state, "active" | "archived") {
        return None;
    }
    let category = s(it, "categoryUuid");
    let kind = kind_of(category);
    let (ov, d) = (&it["overview"], &it["details"]);
    let mut fields = Fields::default();
    let mut files = vec![];
    for lf in d["loginFields"].as_array().into_iter().flatten() {
        if let designation @ ("username" | "password") = s(lf, "designation") {
            fields.put(Some(if designation == "username" { "username" } else { "password" }), designation, s(lf, "value"), designation == "password");
        }
    }
    fields.put(Some("password"), "password", s(d, "password"), true);
    let url = s(ov, "url");
    fields.put(Some("url"), "URL", url, false);
    for u in ov["urls"].as_array().into_iter().flatten().filter(|u| s(u, "url") != url) {
        fields.put(None, if s(u, "label").is_empty() { "URL" } else { s(u, "label") }, s(u, "url"), false);
    }
    let mut name = vec![];
    for field in d["sections"].as_array().into_iter().flatten().flat_map(|sec| sec["fields"].as_array().into_iter().flatten()) {
        let id = s(field, "id");
        let label = if s(field, "title").is_empty() { id } else { s(field, "title") };
        match value(&field["value"]) {
            Val::Text(v, _) if kind == Kind::Identity && matches!(id, "firstname" | "initial" | "lastname") => name.push(v),
            Val::Text(v, secret) => {
                let key = MAP.iter().find(|(c, i, _)| (*c == category || *c == "*") && *i == id).map(|(_, _, k)| *k);
                fields.put(key, label, &v, secret);
            }
            Val::Totp(v) => fields.put(Some("totp"), label, &v, true),
            Val::Ssh(private, public) => {
                fields.put(Some("privatekey"), label, &private, true);
                fields.put(Some("publickey"), label, &public, false);
            }
            Val::File(id, n) => files.extend(file(&id, &n)),
            Val::Skip => {}
        }
    }
    let full = name.iter().map(|n| n.trim()).filter(|n| !n.is_empty()).collect::<Vec<_>>().join(" ");
    fields.put(Some("fullname"), "name", &full, false);
    let doc = &d["documentAttributes"];
    files.extend(file(s(doc, "documentId"), s(doc, "fileName")));
    fields.put(Some(if kind == Kind::Note { "content" } else { "notes" }), "notes", s(d, "notesPlain"), true);
    let mut title = s(ov, "title").trim().to_string();
    if title.is_empty() {
        title = host_title(url);
    }
    let mut out = Imported::new(
        kind,
        if title.is_empty() { "Sin título".into() } else { title },
        (!folder.is_empty()).then(|| folder.to_string()),
        it["favIndex"].as_i64().unwrap_or(0) > 0,
        fields.0,
    );
    out.tags = ov["tags"].as_array().into_iter().flatten().filter_map(Value::as_str).map(String::from).collect();
    out.archived = state == "archived";
    out.files = files;
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::super::parse as parse_any;
    use super::*;
    use std::io::Write;

    fn val<'a>(i: &'a Imported, k: &str) -> &'a str {
        i.fields.iter().find(|f| f.key == k).map(|f| f.value.as_str()).unwrap_or("")
    }

    #[test]
    fn onepux_all_kinds() {
        let data = r#"{"accounts":[{"attrs":{"name":"Poxi"},"vaults":[{"attrs":{"name":"Personal"},"items":[
          {"uuid":"a","favIndex":1,"state":"active","categoryUuid":"001",
           "overview":{"title":"GitHub","url":"https://github.com","urls":[{"label":"","url":"https://github.com"},{"label":"Gist","url":"https://gist.github.com"}],"tags":["curro"]},
           "details":{"loginFields":[{"value":"poxi","designation":"username"},{"value":"pw","designation":"password"},{"value":"on","name":"remember"}],
             "notesPlain":"nota","sections":[{"title":"","fields":[
               {"title":"one-time password","id":"TOTP_1","value":{"totp":"otpauth://totp/x?secret=JBSWY3DPEHPK3PXP"}},
               {"title":"PIN","id":"p1","value":{"concealed":"1234"}},
               {"title":"Ref","id":"r","value":{"reference":"zzz"}},
               {"title":"Contrato","id":"f","value":{"file":{"fileName":"contrato.txt","documentId":"doc1"}}}]}]}},
          {"uuid":"b","favIndex":0,"state":"archived","categoryUuid":"002","overview":{"title":"Visa"},
           "details":{"sections":[{"fields":[
               {"title":"titular","id":"cardholder","value":{"string":"Poxi"}},
               {"title":"número","id":"ccnum","value":{"creditCardNumber":"4111111111111111"}},
               {"title":"caduca","id":"expiry","value":{"monthYear":203012}},
               {"title":"desde","id":"validFrom","value":{"monthYear":202001}}]}]}},
          {"uuid":"c","state":"active","categoryUuid":"004","overview":{"title":"Yo"},
           "details":{"sections":[{"fields":[
               {"title":"nombre","id":"firstname","value":{"string":"Poxi"}},
               {"title":"apellido","id":"lastname","value":{"string":"TV"}},
               {"title":"nacimiento","id":"birthdate","value":{"date":946684800}},
               {"title":"email","id":"email","value":{"email":{"email_address":"a@b.c","provider":null}}},
               {"title":"dirección","id":"address","value":{"address":{"street":"Calle 1","city":"Madrid","zip":"28001","state":"","country":"es"}}}]}]}},
          {"uuid":"d","state":"active","categoryUuid":"005","overview":{"title":"Router"},"details":{"password":"solo"}},
          {"uuid":"e","state":"active","categoryUuid":"006","overview":{"title":"Escritura"},
           "details":{"documentAttributes":{"fileName":"casa.pdf","documentId":"doc2"}}},
          {"uuid":"f","state":"active","categoryUuid":"114","overview":{"title":"Servidor"},
           "details":{"sections":[{"fields":[{"title":"clave","id":"private_key","value":{"sshKey":{"privateKey":"raro","metadata":{"privateKey":"-----BEGIN-----","publicKey":"ssh-ed25519 AAA"}}}}]}]}},
          {"uuid":"g","state":"deleted","categoryUuid":"001","overview":{"title":"Borrada"},"details":{}}
        ]}]}]}"#;
        let mut zip = zip::ZipWriter::new(Cursor::new(vec![]));
        let opts = zip::write::SimpleFileOptions::default();
        zip.start_file("export.data", opts).unwrap();
        zip.write_all(data.as_bytes()).unwrap();
        zip.start_file("files/doc1__contrato.txt", opts).unwrap();
        zip.write_all(b"hola").unwrap();
        zip.start_file("files/doc2__casa.pdf", opts).unwrap();
        zip.write_all(b"%PDF").unwrap();
        let bytes = zip.finish().unwrap().into_inner();

        let (fmt, v) = parse_any(&bytes, None).unwrap();
        assert_eq!(fmt, "1Password (1PUX)");
        assert_eq!(v.len(), 6, "lo borrado no se importa");
        let gh = &v[0];
        assert_eq!((gh.kind, gh.folder.as_deref(), gh.favorite, gh.archived), (Kind::Login, Some("Personal"), true, false));
        assert_eq!((val(gh, "username"), val(gh, "password"), val(gh, "url")), ("poxi", "pw", "https://github.com"));
        assert_eq!((val(gh, "totp"), val(gh, "notes")), ("otpauth://totp/x?secret=JBSWY3DPEHPK3PXP", "nota"));
        assert!(gh.fields.iter().any(|f| f.label == "PIN" && f.secret && f.value == "1234"));
        assert!(gh.fields.iter().any(|f| f.label == "Gist" && f.value == "https://gist.github.com"));
        assert!(!gh.fields.iter().any(|f| f.value == "on" || f.value == "zzz"));
        assert_eq!(gh.tags, vec!["curro"]);
        assert_eq!((gh.files[0].name.as_str(), gh.files[0].data.as_slice()), ("contrato.txt", &b"hola"[..]));
        let visa = &v[1];
        assert_eq!((visa.kind, visa.archived, val(visa, "number"), val(visa, "expiry")), (Kind::Card, true, "4111111111111111", "12/2030"));
        assert!(visa.fields.iter().any(|f| f.label == "desde" && f.value == "01/2020"));
        let yo = &v[2];
        assert_eq!((val(yo, "fullname"), val(yo, "email"), val(yo, "address")), ("Poxi TV", "a@b.c", "Calle 1, 28001, Madrid, es"));
        assert!(yo.fields.iter().any(|f| f.label == "nacimiento" && f.value == "2000-01-01"));
        assert_eq!((v[3].kind, val(&v[3], "password")), (Kind::Login, "solo"));
        assert_eq!((v[4].kind, v[4].files[0].name.as_str()), (Kind::Note, "casa.pdf"));
        assert_eq!((v[5].kind, val(&v[5], "privatekey"), val(&v[5], "publickey")), (Kind::Ssh, "-----BEGIN-----", "ssh-ed25519 AAA"));
        assert_eq!(parse_any(b"PK\x03\x04nope", None).unwrap_err().code, "import.badFile");
    }
}
