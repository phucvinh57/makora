//! JavaScript number formatting, so ported text matches what the web UI showed.

const TIE_PROBE: usize = 40;

/// `Number.prototype.toFixed`: an exact tie rounds away from zero, where Rust rounds to even.
pub fn to_fixed(value: f64, digits: usize) -> String {
    if !value.is_finite() {
        return js_string(value);
    }
    let value = if value == 0.0 { 0.0 } else { value };
    let rounded = format!("{value:.digits$}");
    let exact = format!("{value:.prec$}", prec = digits + TIE_PROBE);
    let (head, tail) = exact.split_at(exact.len() - TIE_PROBE);
    let tie = tail.starts_with('5') && tail[1..].bytes().all(|b| b == b'0');
    let truncated = head.trim_end_matches('.');
    if !tie || rounded != truncated {
        return rounded;
    }
    let step = 10f64.powi(-(digits as i32)).copysign(value);
    let away = truncated.parse::<f64>().unwrap_or(value) + step;
    format!("{away:.digits$}")
}

/// `Math.round`: halves round up, toward positive infinity.
pub fn js_round(value: f64) -> f64 {
    let floor = value.floor();
    if value - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    }
}

/// `String(number)` for the values the UI shows; whole numbers print without a fraction.
pub fn js_string(value: f64) -> String {
    if value.is_nan() {
        "NaN".to_owned()
    } else if value.is_infinite() {
        if value > 0.0 { "Infinity" } else { "-Infinity" }.to_owned()
    } else if value == 0.0 {
        "0".to_owned()
    } else if value.fract() == 0.0 && value.abs() < 1e21 {
        format!("{value:.0}")
    } else {
        format!("{value}")
    }
}

/// `Number.prototype.toLocaleString()` in `en-US`: grouped thousands, at most three decimals.
pub fn to_locale_string(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        return if value > 0.0 { "∞" } else { "-∞" }.to_owned();
    }
    let fixed = to_fixed(value, 3);
    let fixed = fixed.trim_end_matches('0').trim_end_matches('.');
    let (sign, unsigned) = fixed
        .strip_prefix('-')
        .map_or(("", fixed), |rest| ("-", rest));
    let (whole, fraction) = unsigned
        .split_once('.')
        .map_or((unsigned, None), |(w, f)| (w, Some(f)));
    let mut grouped = String::with_capacity(whole.len() + whole.len() / 3);
    for (index, digit) in whole.chars().enumerate() {
        if index > 0 && (whole.len() - index) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    match fraction {
        Some(fraction) => format!("{sign}{grouped}.{fraction}"),
        None => format!("{sign}{grouped}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_fixed_rounds_exact_ties_away_from_zero() {
        assert_eq!(to_fixed(2.5, 0), "3");
        assert_eq!(to_fixed(0.5, 0), "1");
        assert_eq!(to_fixed(0.25, 1), "0.3");
        assert_eq!(to_fixed(-2.5, 0), "-3");
        assert_eq!(to_fixed(1.125, 2), "1.13");
    }

    #[test]
    fn to_fixed_keeps_non_ties_and_inexact_halves() {
        assert_eq!(to_fixed(1.005, 2), "1.00");
        assert_eq!(to_fixed(1.15, 1), "1.1");
        assert_eq!(to_fixed(1.23456, 2), "1.23");
        assert_eq!(to_fixed(12.0, 1), "12.0");
        assert_eq!(to_fixed(9.96, 1), "10.0");
        assert_eq!(to_fixed(-0.0, 1), "0.0");
        assert_eq!(to_fixed(f64::NAN, 1), "NaN");
    }

    #[test]
    fn js_round_rounds_halves_up() {
        assert_eq!(js_round(2.5), 3.0);
        assert_eq!(js_round(-2.5), -2.0);
        assert_eq!(js_round(2.4), 2.0);
        assert_eq!(js_round(-2.6), -3.0);
    }

    #[test]
    fn js_string_prints_like_javascript() {
        assert_eq!(js_string(512.0), "512");
        assert_eq!(js_string(0.5), "0.5");
        assert_eq!(js_string(-0.0), "0");
        assert_eq!(js_string(1.25), "1.25");
        assert_eq!(js_string(f64::INFINITY), "Infinity");
    }

    #[test]
    fn to_locale_string_groups_thousands() {
        assert_eq!(to_locale_string(0.0), "0");
        assert_eq!(to_locale_string(999.0), "999");
        assert_eq!(to_locale_string(1_000.0), "1,000");
        assert_eq!(to_locale_string(1_234_567.0), "1,234,567");
        assert_eq!(to_locale_string(-1_234.5), "-1,234.5");
        assert_eq!(to_locale_string(1.23456), "1.235");
        assert_eq!(to_locale_string(2.0625), "2.063");
    }
}
