use std::time::{SystemTime, UNIX_EPOCH};

/// Segundos desde 1970 (UTC)
pub fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// `AAAAMMDD-HHMMSS` en UTC (nombres de las copias de seguridad)
pub fn stamp(secs: i64) -> String {
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Calendario civil a partir de días (algoritmo de Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// Días desde 1970 de una fecha civil (algoritmo de Howard Hinnant)
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Día (desde 1970) en que caduca algo escrito como `MM/AA`, `MM/AAAA`, `AAAA-MM-DD` o `DD/MM/AAAA`.
/// Con solo mes, caduca el último día de ese mes.
pub fn expiry_day(s: &str) -> Option<i64> {
    let n: Vec<i64> = s.split(|c: char| !c.is_ascii_digit()).filter(|x| !x.is_empty()).filter_map(|x| x.parse().ok()).collect();
    let year = |y: i64| if y < 100 { 2000 + y } else { y };
    let (y, m, d) = match n.as_slice() {
        [m, y] if (1..=12).contains(m) => {
            let y = year(*y);
            // Último día del mes: el día antes del 1 del mes siguiente
            return Some(if *m == 12 { days_from_civil(y + 1, 1, 1) } else { days_from_civil(y, m + 1, 1) } - 1);
        }
        [y, m, d] if *y > 31 => (*y, *m, *d),
        [d, m, y] => (year(*y), *m, *d),
        _ => return None,
    };
    ((1..=12).contains(&m) && (1..=31).contains(&d)).then(|| days_from_civil(y, m, d))
}

#[cfg(test)]
mod tests {
    #[test]
    fn stamp_formats_utc() {
        assert_eq!(super::stamp(0), "19700101-000000");
        assert_eq!(super::stamp(1_791_885_296), "20261013-095456");
        assert_eq!(super::stamp(951_782_400), "20000229-000000");
    }

    #[test]
    fn expiry_formats() {
        use super::{days_from_civil, expiry_day};
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(expiry_day("12/30"), Some(days_from_civil(2030, 12, 31)));
        assert_eq!(expiry_day("02/2028"), Some(days_from_civil(2028, 2, 29)));
        assert_eq!(expiry_day("2031-05-17"), Some(days_from_civil(2031, 5, 17)));
        assert_eq!(expiry_day("17/05/2031"), Some(days_from_civil(2031, 5, 17)));
        assert_eq!(expiry_day("nunca"), None);
        assert_eq!(expiry_day("13/30"), None);
    }
}
