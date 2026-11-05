use makora_lib::categorization::application::usage::{DailyModelTokens, ModelUsage, UsageTotals};
use makora_lib::categorization::ports::RunUsage;
use makora_lib::settings::ModelPrice;

use super::duration::format_seconds;
use super::number::{js_round, to_fixed, to_locale_string};

/// The token counts a cost is estimated from.
pub trait TokenCounts {
    fn input_tokens(&self) -> u64;
    fn cached_input_tokens(&self) -> u64;
    fn output_tokens(&self) -> u64;
}

/// The request timings speeds are measured from.
pub trait Timings: TokenCounts {
    fn requests(&self) -> u64;
    fn busy_ms(&self) -> u64;
    fn prefill_ms(&self) -> u64;
    fn decode_ms(&self) -> u64;
}

macro_rules! impl_token_counts {
    ($($ty:ty),*) => {$(
        impl TokenCounts for $ty {
            fn input_tokens(&self) -> u64 {
                self.input_tokens
            }
            fn cached_input_tokens(&self) -> u64 {
                self.cached_input_tokens
            }
            fn output_tokens(&self) -> u64 {
                self.output_tokens
            }
        }
    )*};
}

macro_rules! impl_timings {
    ($($ty:ty),*) => {$(
        impl Timings for $ty {
            fn requests(&self) -> u64 {
                self.requests
            }
            fn busy_ms(&self) -> u64 {
                self.busy_ms
            }
            fn prefill_ms(&self) -> u64 {
                self.prefill_ms
            }
            fn decode_ms(&self) -> u64 {
                self.decode_ms
            }
        }
    )*};
}

impl_token_counts!(RunUsage, UsageTotals, DailyModelTokens);
impl_timings!(RunUsage, UsageTotals);

/// `950`, `12.3k`, `4M`.
pub fn format_tokens(count: u64) -> String {
    let count = count as f64;
    if count >= 1_000_000.0 {
        format!("{}M", trim(count / 1_000_000.0))
    } else if count >= 1_000.0 {
        format!("{}k", trim(count / 1_000.0))
    } else {
        format!("{count}")
    }
}

fn trim(value: f64) -> String {
    if value >= 100.0 {
        to_fixed(value, 0)
    } else {
        let fixed = to_fixed(value, 1);
        fixed
            .strip_suffix(".0")
            .map_or_else(|| fixed.clone(), str::to_owned)
    }
}

/// `850 ms`, `4.2 s`, then minutes and hours.
pub fn format_ms(ms: f64) -> String {
    if ms < 1_000.0 {
        format!("{} ms", js_round(ms))
    } else if ms < 60_000.0 {
        format!("{} s", to_fixed(ms / 1_000.0, 1))
    } else {
        format_seconds(ms / 1_000.0)
    }
}

pub fn format_rate(value: Option<f64>, unit: &str) -> String {
    match value {
        None => "—".to_owned(),
        Some(value) => format!(
            "{} {unit}",
            to_fixed(value, if value >= 100.0 { 0 } else { 1 })
        ),
    }
}

pub fn format_percent(ratio: Option<f64>) -> String {
    ratio.map_or_else(
        || "—".to_owned(),
        |ratio| format!("{}%", js_round(ratio * 100.0)),
    )
}

/// Output speed is timed over decoding alone for the local model, and over the whole request for
/// a remote one, whose generation time is not reported apart.
fn generation_ms(usage: &impl Timings, local: bool) -> u64 {
    if local {
        usage.decode_ms()
    } else {
        usage.busy_ms()
    }
}

fn per_second(count: u64, ms: u64) -> Option<f64> {
    (ms > 0 && count > 0).then(|| count as f64 * 1_000.0 / ms as f64)
}

pub fn output_tokens_per_second(usage: &impl Timings, local: bool) -> Option<f64> {
    per_second(usage.output_tokens(), generation_ms(usage, local))
}

/// Output speed across models, each timed its own way.
pub fn overall_output_tokens_per_second(models: &[ModelUsage]) -> Option<f64> {
    let (tokens, ms) = models.iter().fold((0, 0), |(tokens, ms), model| {
        (
            tokens + model.totals.output_tokens,
            ms + generation_ms(&model.totals, model.provider.is_none()),
        )
    });
    per_second(tokens, ms)
}

/// Local prefill speed: the prompt tokens that were not already in the KV cache.
pub fn prompt_tokens_per_second(usage: &impl Timings) -> Option<f64> {
    let fresh = usage
        .input_tokens()
        .saturating_sub(usage.cached_input_tokens());
    per_second(fresh, usage.prefill_ms())
}

pub fn cache_hit_ratio(usage: &impl TokenCounts) -> Option<f64> {
    (usage.input_tokens() > 0)
        .then(|| usage.cached_input_tokens() as f64 / usage.input_tokens() as f64)
}

pub fn items_per_minute(items: u64, llm_ms: u64) -> Option<f64> {
    (llm_ms > 0 && items > 0).then(|| items as f64 * 60_000.0 / llm_ms as f64)
}

pub fn average_latency_ms(usage: &impl Timings) -> Option<f64> {
    (usage.requests() > 0).then(|| usage.busy_ms() as f64 / usage.requests() as f64)
}

/// `12.3k tokens in 45.0 s.`, for a finished run's toast.
pub fn describe_run_usage(usage: &RunUsage) -> Option<String> {
    let tokens = usage.input_tokens + usage.output_tokens;
    let ms = usage.embed_ms + usage.llm_ms;
    if tokens == 0 && ms == 0 {
        return None;
    }
    Some(format!(
        "{} tokens in {}.",
        format_tokens(tokens),
        format_ms(ms as f64)
    ))
}

/// A tooltip for a speed cell: the local model's prefill speed, which a remote one does not report.
pub fn speed_detail(usage: &impl Timings, local: bool) -> Option<String> {
    if !local {
        return None;
    }
    prompt_tokens_per_second(usage)
        .map(|prompt| format!("Prompt: {}", format_rate(Some(prompt), "tok/s")))
}

/// The price set for `model` on `provider`; the local model (`None`) never has one.
pub fn find_price<'a>(
    prices: &'a [ModelPrice],
    provider: Option<&str>,
    model: &str,
) -> Option<&'a ModelPrice> {
    let provider = provider?;
    prices
        .iter()
        .find(|price| price.provider == provider && price.model == model)
}

/// US dollars, with cached prompt tokens at the cached price when one is set.
pub fn estimate_cost(tokens: &impl TokenCounts, price: &ModelPrice) -> f64 {
    let cached = tokens.cached_input_tokens().min(tokens.input_tokens());
    let fresh = tokens.input_tokens() - cached;
    (fresh as f64 * price.input
        + cached as f64 * price.cached_input.unwrap_or(price.input)
        + tokens.output_tokens() as f64 * price.output)
        / 1_000_000.0
}

/// `$0.00`, `<$0.01`, `$12.34`, `$1,234`.
pub fn format_cost(usd: Option<f64>) -> String {
    match usd {
        None => "—".to_owned(),
        Some(0.0) => "$0.00".to_owned(),
        Some(usd) if usd < 0.01 => "<$0.01".to_owned(),
        Some(usd) if usd < 100.0 => format!("${}", to_fixed(usd, 2)),
        Some(usd) => format!("${}", to_locale_string(js_round(usd))),
    }
}

/// The exact figure, for a tooltip.
pub fn format_cost_exact(usd: f64) -> String {
    format!("${}", to_fixed(usd, 4))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(f: impl FnOnce(&mut RunUsage)) -> RunUsage {
        let mut usage = RunUsage::default();
        f(&mut usage);
        usage
    }

    fn totals(f: impl FnOnce(&mut UsageTotals)) -> UsageTotals {
        let mut totals = UsageTotals::default();
        f(&mut totals);
        totals
    }

    fn price(cached_input: Option<f64>) -> ModelPrice {
        ModelPrice {
            provider: "openai".into(),
            model: "gpt".into(),
            input: 2.0,
            cached_input,
            output: 8.0,
        }
    }

    #[test]
    fn tokens_use_k_and_m() {
        assert_eq!(format_tokens(0), "0");
        assert_eq!(format_tokens(999), "999");
        assert_eq!(format_tokens(1_000), "1k");
        assert_eq!(format_tokens(12_345), "12.3k");
        assert_eq!(format_tokens(123_456), "123k");
        assert_eq!(format_tokens(1_000_000), "1M");
        assert_eq!(format_tokens(2_560_000), "2.6M");
        assert_eq!(format_tokens(150_000_000), "150M");
    }

    #[test]
    fn ms_switch_units() {
        assert_eq!(format_ms(0.0), "0 ms");
        assert_eq!(format_ms(850.4), "850 ms");
        assert_eq!(format_ms(999.5), "1000 ms");
        assert_eq!(format_ms(1_000.0), "1.0 s");
        assert_eq!(format_ms(45_000.0), "45.0 s");
        assert_eq!(format_ms(59_999.0), "60.0 s");
        assert_eq!(format_ms(60_000.0), "1m 0s");
        assert_eq!(format_ms(3_725_000.0), "1h 2m");
    }

    #[test]
    fn rate_and_percent() {
        assert_eq!(format_rate(None, "tok/s"), "—");
        assert_eq!(format_rate(Some(42.25), "tok/s"), "42.3 tok/s");
        assert_eq!(format_rate(Some(150.4), "tok/s"), "150 tok/s");
        assert_eq!(format_percent(None), "—");
        assert_eq!(format_percent(Some(0.0)), "0%");
        assert_eq!(format_percent(Some(0.456)), "46%");
        assert_eq!(format_percent(Some(1.0)), "100%");
    }

    #[test]
    fn output_speed_is_timed_by_decode_locally_and_busy_remotely() {
        let usage = run(|u| {
            u.output_tokens = 200;
            u.decode_ms = 1_000;
            u.busy_ms = 4_000;
        });
        assert_eq!(output_tokens_per_second(&usage, true), Some(200.0));
        assert_eq!(output_tokens_per_second(&usage, false), Some(50.0));
        assert_eq!(output_tokens_per_second(&RunUsage::default(), true), None);
        let no_decode = run(|u| u.output_tokens = 10);
        assert_eq!(output_tokens_per_second(&no_decode, true), None);
    }

    #[test]
    fn overall_speed_mixes_local_and_remote_timings() {
        let models = vec![
            ModelUsage {
                provider: None,
                model: "local".into(),
                totals: totals(|t| {
                    t.output_tokens = 100;
                    t.decode_ms = 1_000;
                    t.busy_ms = 9_000;
                }),
            },
            ModelUsage {
                provider: Some("openai".into()),
                model: "gpt".into(),
                totals: totals(|t| {
                    t.output_tokens = 300;
                    t.decode_ms = 0;
                    t.busy_ms = 3_000;
                }),
            },
        ];
        assert_eq!(overall_output_tokens_per_second(&models), Some(100.0));
        assert_eq!(overall_output_tokens_per_second(&[]), None);
    }

    #[test]
    fn prompt_speed_counts_uncached_tokens() {
        let usage = run(|u| {
            u.input_tokens = 1_500;
            u.cached_input_tokens = 500;
            u.prefill_ms = 500;
        });
        assert_eq!(prompt_tokens_per_second(&usage), Some(2_000.0));
        let all_cached = run(|u| {
            u.input_tokens = 500;
            u.cached_input_tokens = 500;
            u.prefill_ms = 500;
        });
        assert_eq!(prompt_tokens_per_second(&all_cached), None);
        let over_cached = run(|u| {
            u.input_tokens = 100;
            u.cached_input_tokens = 500;
            u.prefill_ms = 500;
        });
        assert_eq!(prompt_tokens_per_second(&over_cached), None);
    }

    #[test]
    fn cache_hit_ratio_needs_input() {
        assert_eq!(cache_hit_ratio(&RunUsage::default()), None);
        let usage = totals(|t| {
            t.input_tokens = 400;
            t.cached_input_tokens = 100;
        });
        assert_eq!(cache_hit_ratio(&usage), Some(0.25));
    }

    #[test]
    fn items_per_minute_needs_both() {
        assert_eq!(items_per_minute(30, 60_000), Some(30.0));
        assert_eq!(items_per_minute(0, 60_000), None);
        assert_eq!(items_per_minute(30, 0), None);
    }

    #[test]
    fn average_latency_needs_requests() {
        assert_eq!(average_latency_ms(&RunUsage::default()), None);
        let usage = run(|u| {
            u.requests = 4;
            u.busy_ms = 1_000;
        });
        assert_eq!(average_latency_ms(&usage), Some(250.0));
    }

    #[test]
    fn run_usage_toast() {
        assert_eq!(describe_run_usage(&RunUsage::default()), None);
        let usage = run(|u| {
            u.input_tokens = 12_000;
            u.output_tokens = 300;
            u.embed_ms = 5_000;
            u.llm_ms = 40_000;
        });
        assert_eq!(
            describe_run_usage(&usage).as_deref(),
            Some("12.3k tokens in 45.0 s.")
        );
        let embed_only = run(|u| u.embed_ms = 300);
        assert_eq!(
            describe_run_usage(&embed_only).as_deref(),
            Some("0 tokens in 300 ms.")
        );
    }

    #[test]
    fn speed_detail_is_local_only() {
        let usage = run(|u| {
            u.input_tokens = 1_000;
            u.prefill_ms = 2_000;
        });
        assert_eq!(speed_detail(&usage, false), None);
        assert_eq!(
            speed_detail(&usage, true).as_deref(),
            Some("Prompt: 500 tok/s")
        );
        assert_eq!(speed_detail(&RunUsage::default(), true), None);
    }

    #[test]
    fn price_lookup_matches_provider_and_model() {
        let prices = vec![price(None)];
        assert!(find_price(&prices, Some("openai"), "gpt").is_some());
        assert!(find_price(&prices, Some("openai"), "other").is_none());
        assert!(find_price(&prices, Some("groq"), "gpt").is_none());
        assert!(find_price(&prices, None, "gpt").is_none());
    }

    #[test]
    fn cost_prices_cached_tokens_apart() {
        let usage = run(|u| {
            u.input_tokens = 1_000_000;
            u.cached_input_tokens = 500_000;
            u.output_tokens = 250_000;
        });
        assert_eq!(estimate_cost(&usage, &price(None)), 4.0);
        assert_eq!(estimate_cost(&usage, &price(Some(0.5))), 3.25);
        let daily = DailyModelTokens {
            day: "2026-11-05".into(),
            provider: "openai".into(),
            model: "gpt".into(),
            input_tokens: 100,
            cached_input_tokens: 500,
            output_tokens: 0,
        };
        assert_eq!(estimate_cost(&daily, &price(Some(0.0))), 0.0);
    }

    #[test]
    fn cost_formats() {
        assert_eq!(format_cost(None), "—");
        assert_eq!(format_cost(Some(0.0)), "$0.00");
        assert_eq!(format_cost(Some(0.004)), "<$0.01");
        assert_eq!(format_cost(Some(0.01)), "$0.01");
        assert_eq!(format_cost(Some(12.345_6)), "$12.35");
        assert_eq!(format_cost(Some(99.99)), "$99.99");
        assert_eq!(format_cost(Some(100.0)), "$100");
        assert_eq!(format_cost(Some(1_234.5)), "$1,235");
        assert_eq!(format_cost_exact(0.123_456), "$0.1235");
        assert_eq!(format_cost_exact(3.0), "$3.0000");
    }
}
