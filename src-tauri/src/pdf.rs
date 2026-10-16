//! PDF del kit de emergencia hecho a mano (una página A4, Helvetica de serie): sin dependencias y sin pasar por
//! la impresión del navegador.

use serde::Deserialize;
use std::fmt::Write;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KitTexts {
    pub title: String,
    pub vault_label: String,
    pub vault: String,
    pub date_label: String,
    pub date: String,
    pub how: String,
    pub steps: String,
    pub warn: String,
    pub where_label: String,
}

const W: f32 = 595.28;
const H: f32 = 841.89;
const M: f32 = 50.0;

/// Texto en WinAnsi (cp1252, la codificación de Helvetica en PDF), escapado para ir entre paréntesis.
fn enc(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        let b: u8 = match c {
            '(' | ')' | '\\' => {
                out.push('\\');
                c as u8
            }
            '\u{20}'..='\u{7e}' => c as u8,
            '\u{a0}'..='\u{ff}' => c as u32 as u8,
            '€' => 0x80,
            '…' => 0x85,
            '•' => 0x95,
            '–' => 0x96,
            '—' => 0x97,
            '‘' => 0x91,
            '’' => 0x92,
            '“' => 0x93,
            '”' => 0x94,
            '→' => b'>',
            _ => b'?',
        };
        if b < 0x80 {
            out.push(b as char);
        } else {
            let _ = write!(out, "\\{:03o}", b);
        }
    }
    out
}

/// Ancho aproximado en Helvetica (para partir líneas; no hace falta exactitud tipográfica)
fn width(s: &str, size: f32, bold: bool) -> f32 {
    let em: f32 = s
        .chars()
        .map(|c| match c {
            'i' | 'j' | 'l' | 't' | 'f' | 'r' | 'I' | '.' | ',' | ';' | ':' | '\'' | '!' | '|' | ' ' => 0.28,
            'm' | 'w' | 'M' | 'W' => 0.83,
            c if c.is_uppercase() => 0.68,
            _ => 0.55,
        })
        .sum();
    em * size * if bold { 1.06 } else { 1.0 }
}

fn wrap(text: &str, size: f32, bold: bool, max: f32) -> Vec<String> {
    let mut lines = vec![];
    let mut cur = String::new();
    for word in text.split_whitespace() {
        let next = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
        if width(&next, size, bold) > max && !cur.is_empty() {
            lines.push(std::mem::replace(&mut cur, word.to_string()));
        } else {
            cur = next;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

struct Page(String);

impl Page {
    fn text(&mut self, x: f32, y: f32, size: f32, bold: bool, gray: f32, s: &str) {
        let _ = writeln!(self.0, "BT {gray} g /{} {size} Tf {x:.2} {y:.2} Td ({}) Tj ET", if bold { "F2" } else { "F1" }, enc(s));
    }
    fn fill(&mut self, x: f32, y: f32, w: f32, h: f32, (r, g, b): (f32, f32, f32)) {
        let _ = writeln!(self.0, "{r:.3} {g:.3} {b:.3} rg {x:.2} {y:.2} {w:.2} {h:.2} re f");
    }
    fn stroke(&mut self, x: f32, y: f32, w: f32, h: f32, gray: f32, line: f32) {
        let _ = writeln!(self.0, "{line} w {gray} G {x:.2} {y:.2} {w:.2} {h:.2} re S");
    }
    /// Párrafo partido en líneas; devuelve la `y` de debajo
    fn para(&mut self, y: f32, size: f32, bold: bool, gray: f32, s: &str) -> f32 {
        let mut y = y;
        for l in wrap(s, size, bold, W - 2.0 * M) {
            self.text(M, y, size, bold, gray, &l);
            y -= size * 1.45;
        }
        y
    }
}

/// El logo de la suite (P pixelada con relieve y candado), dibujado con rectángulos
fn logo(p: &mut Page, ox: f32, oy: f32, s: f32) {
    const P: [(f32, f32, f32, f32); 9] = [
        (0., 0., 20., 4.),
        (0., 4., 8., 4.),
        (16., 4., 8., 4.),
        (0., 8., 8., 4.),
        (16., 8., 8., 4.),
        (0., 12., 20., 4.),
        (0., 16., 8., 4.),
        (0., 20., 8., 4.),
        (0., 24., 8., 4.),
    ];
    const L: [(f32, f32, f32, f32); 4] = [(20., 20., 6., 1.), (20., 21., 1., 3.), (25., 21., 1., 3.), (19., 24., 8., 5.)];
    let mut r = |x: f32, y: f32, w: f32, h: f32, c: (f32, f32, f32)| p.fill(ox + (x + 7.) * s, oy - (y + 5. + h) * s, w * s, h * s, c);
    r(-7., -5., 40., 40., (0.024, 0.09, 0.067));
    let green = (0.204, 0.827, 0.6);
    let gold = (1.0, 0.824, 0.373);
    let k = |c: (f32, f32, f32), f: f32| (c.0 * f, c.1 * f, c.2 * f);
    for (shapes, color) in [(&P[..], green), (&L[..], gold)] {
        for (off, f) in [(2., 0.33), (1., 0.62), (0., 1.0)] {
            for &(x, y, w, h) in shapes {
                r(x + off, y + off, w, h, k(color, f));
            }
        }
    }
    r(22., 26., 2., 2., k(gold, 0.3));
}

pub fn kit(t: &KitTexts, words: &[String]) -> Vec<u8> {
    let mut p = Page(String::new());
    let top = H - M;
    logo(&mut p, M, top, 1.2);
    p.text(M + 62.0, top - 22.0, 20.0, true, 0.07, &format!("PoxiPass · {}", t.title));
    p.text(M + 62.0, top - 40.0, 10.5, false, 0.35, &format!("{}: {}     {}: {}", t.vault_label, t.vault, t.date_label, t.date));
    p.fill(M, top - 62.0, W - 2.0 * M, 1.5, (0.07, 0.07, 0.07));

    // Palabras en cuadrícula de 4 columnas
    let gap = 8.0;
    let bw = (W - 2.0 * M - 3.0 * gap) / 4.0;
    let bh = 30.0;
    let grid_top = top - 84.0;
    for (i, w) in words.iter().enumerate() {
        let (c, r) = ((i % 4) as f32, (i / 4) as f32);
        let x = M + c * (bw + gap);
        let y = grid_top - r * (bh + gap) - bh;
        p.stroke(x, y, bw, bh, 0.2, 1.0);
        p.text(x + 8.0, y + 11.0, 8.0, false, 0.5, &(i + 1).to_string());
        p.text(x + 24.0, y + 10.5, 12.5, true, 0.05, w);
    }
    let rows = words.len().div_ceil(4) as f32;
    let mut y = grid_top - rows * (bh + gap) - 22.0;

    y = p.para(y, 13.0, true, 0.07, &t.how) - 2.0;
    y = p.para(y, 10.5, false, 0.2, &t.steps) - 8.0;
    y = p.para(y, 10.5, true, 0.07, &t.warn) - 10.0;
    p.text(M, y, 10.5, true, 0.07, &t.where_label);
    p.stroke(M, y - 78.0, W - 2.0 * M, 68.0, 0.2, 1.0);

    let content = p.0;
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {W} {H}] /Contents 4 0 R /Resources << /Font << /F1 5 0 R /F2 6 0 R >> >> >>"),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>".to_string(),
    ];
    let mut out = String::from("%PDF-1.4\n");
    let mut offsets = vec![];
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        let _ = write!(out, "{} 0 obj\n{o}\nendobj\n", i + 1);
    }
    let xref = out.len();
    let _ = write!(out, "xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1);
    for o in offsets {
        let _ = writeln!(out, "{o:010} 00000 n ");
    }
    let _ = write!(out, "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1);
    out.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_valid_looking_pdf() {
        let t = KitTexts {
            title: "Kit de emergencia".into(),
            vault_label: "Llavero".into(),
            vault: "Mi llavero (casa)".into(),
            date_label: "Creado el".into(),
            date: "15/10/2026".into(),
            how: "Cómo se usa".into(),
            steps: "Abre PoxiPass → «Tengo el kit» y escribe las palabras. ".repeat(6),
            warn: "Guárdalo como guardarías dinero.".into(),
            where_label: "Dónde lo guardo:".into(),
        };
        let words: Vec<String> = (0..26).map(|i| format!("cabezón{i}")).collect();
        let pdf = String::from_utf8(kit(&t, &words)).unwrap();
        assert!(pdf.starts_with("%PDF-1.4") && pdf.ends_with("%%EOF\n"));
        assert!(pdf.contains("Mi llavero \\(casa\\)"), "paréntesis escapados");
        assert!(pdf.contains("cabez\\363n25"), "ó en WinAnsi octal");
        // El xref apunta de verdad a cada objeto
        let xref: usize = pdf.rsplit("startxref\n").next().unwrap().lines().next().unwrap().parse().unwrap();
        assert!(pdf[xref..].starts_with("xref"));
        for line in pdf[xref..].lines().skip(3).take(6) {
            let off: usize = line[..10].parse().unwrap();
            assert!(pdf[off..].split_whitespace().nth(1) == Some("0"), "{line}");
        }
    }
}
