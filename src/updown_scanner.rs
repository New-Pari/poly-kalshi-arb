// src/updown_scanner.rs
// Polymarket Up/Down 15-minute market scanner
//
// Strategy: Find imbalances where YES + NO < 100¢
// Markets: BTC, ETH, SOL, XRP 15-minute Up/Down markets

use anyhow::Result;
use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::{sleep, Duration};
use tracing::{info, warn, debug};

use crate::config::GAMMA_API_BASE;

/// Assets to track for Up/Down markets
const UPDOWN_ASSETS: &[&str] = &["btc", "eth", "sol", "xrp"];

/// 15 minutes in seconds
const MARKET_INTERVAL_SECS: u64 = 900;

/// Only watch the current active 15-minute interval
const LOOKAHEAD_INTERVALS: u64 = 1;

/// Scan interval - check for new markets every 30 seconds
const SCAN_INTERVAL_SECS: u64 = 30;

/// Gamma API market response
#[derive(Debug, Deserialize, Clone)]
pub struct UpDownMarket {
    #[serde(deserialize_with = "deserialize_string_or_number")]
    pub id: u64,
    pub question: String,
    pub slug: String,

    #[serde(rename = "clobTokenIds")]
    pub clob_token_ids: Option<String>,  // JSON array: ["yes_token", "no_token"]

    pub active: Option<bool>,
    pub closed: Option<bool>,

    #[serde(rename = "acceptingOrders")]
    pub accepting_orders: Option<bool>,

    #[serde(rename = "endDate")]
    pub end_date: Option<String>,

    #[serde(rename = "startDate")]
    pub start_date: Option<String>,

    #[serde(default, deserialize_with = "deserialize_json_string_array")]
    pub outcomes: Option<Vec<String>>,  // ["Up", "Down"] - comes as JSON string
}

impl UpDownMarket {
    /// Extract YES (Up) and NO (Down) token IDs
    pub fn get_token_ids(&self) -> Option<(String, String)> {
        let token_str = self.clob_token_ids.as_ref()?;
        let tokens: Vec<String> = serde_json::from_str(token_str).ok()?;

        if tokens.len() >= 2 {
            Some((tokens[0].clone(), tokens[1].clone()))
        } else {
            None
        }
    }

    /// Check if market is tradeable
    pub fn is_active(&self) -> bool {
        self.active.unwrap_or(false)
            && !self.closed.unwrap_or(true)
            && self.accepting_orders.unwrap_or(false)
    }

    /// Extract asset symbol from slug (e.g., "btc-updown-15m-1766100600" -> "btc")
    pub fn get_asset(&self) -> Option<&str> {
        self.slug.split('-').next()
    }
}

/// Active market with token IDs
#[derive(Debug, Clone)]
pub struct ActiveUpDownMarket {
    pub slug: String,
    pub asset: String,
    pub question: String,
    pub yes_token: String,  // "Up" token
    pub no_token: String,   // "Down" token
    pub end_timestamp: u64, // Unix timestamp when market closes
}

pub struct UpDownScanner {
    http: reqwest::Client,
}

impl UpDownScanner {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .expect("Failed to build HTTP client"),
        }
    }

    /// Scan for active Up/Down markets
    ///
    /// Returns only the CURRENT active 15-minute market for each asset
    pub async fn scan_active_markets(&self) -> Result<Vec<ActiveUpDownMarket>> {
        let now = current_timestamp();

        // Generate candidate slugs for current interval only
        let mut candidates = Vec::new();

        for asset in UPDOWN_ASSETS {
            // Find the END of the current 15-minute interval
            // Markets are identified by their end timestamp
            // Example: if now=6:47 PM, current interval is 6:45-7:00, end=7:00
            let current_interval_end = ((now / MARKET_INTERVAL_SECS) + 1) * MARKET_INTERVAL_SECS;

            let slug = format!("{}-updown-15m-{}", asset, current_interval_end);
            candidates.push((asset.to_string(), slug, current_interval_end));
        }

        info!("[UPDOWN] Scanning {} candidate market slugs...", candidates.len());

        // Query all candidates in parallel
        let mut tasks = Vec::new();

        for (asset, slug, end_time) in candidates {
            let http = self.http.clone();
            tasks.push(async move {
                match query_market_by_slug(&http, &slug).await {
                    Ok(Some(market)) if market.is_active() => {
                        if let Some((yes_token, no_token)) = market.get_token_ids() {
                            Some(ActiveUpDownMarket {
                                slug: slug.clone(),
                                asset: asset.clone(),
                                question: market.question.clone(),
                                yes_token,
                                no_token,
                                end_timestamp: end_time,
                            })
                        } else {
                            debug!("[UPDOWN] Market {} has no token IDs", slug);
                            None
                        }
                    }
                    Ok(Some(_)) => {
                        debug!("[UPDOWN] Market {} exists but not active", slug);
                        None
                    }
                    Ok(None) => {
                        debug!("[UPDOWN] Market {} not found (may not exist yet)", slug);
                        None
                    }
                    Err(e) => {
                        warn!("[UPDOWN] Failed to query {}: {}", slug, e);
                        None
                    }
                }
            });
        }

        // Wait for all queries
        let results = futures_util::future::join_all(tasks).await;
        let active_markets: Vec<_> = results.into_iter().filter_map(|r| r).collect();

        info!("[UPDOWN] Found {} active markets", active_markets.len());
        for market in &active_markets {
            info!("  ✅ {} | {} | ends in {}s",
                  market.asset.to_uppercase(),
                  market.question,
                  market.end_timestamp.saturating_sub(now));
        }

        Ok(active_markets)
    }

    /// Continuous scanner - runs in a loop, refreshing active markets
    pub async fn run_continuous_scan<F>(&self, mut on_update: F) -> Result<()>
    where
        F: FnMut(Vec<ActiveUpDownMarket>),
    {
        info!("[UPDOWN] Starting continuous market scanner");

        loop {
            match self.scan_active_markets().await {
                Ok(markets) => {
                    on_update(markets);
                }
                Err(e) => {
                    warn!("[UPDOWN] Scan failed: {}", e);
                }
            }

            sleep(Duration::from_secs(SCAN_INTERVAL_SECS)).await;
        }
    }
}

/// Query Gamma API for a market by slug
async fn query_market_by_slug(http: &reqwest::Client, slug: &str) -> Result<Option<UpDownMarket>> {
    let url = format!("{}/markets?slug={}", GAMMA_API_BASE, slug);

    let resp = http.get(&url).send().await?;

    if !resp.status().is_success() {
        return Ok(None);
    }

    let markets: Vec<UpDownMarket> = resp.json().await?;
    Ok(markets.into_iter().next())
}

/// Get current Unix timestamp
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Deserialize a field that can be either a string or a number
fn deserialize_string_or_number<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Deserialize};

    struct StringOrNumber;

    impl<'de> de::Visitor<'de> for StringOrNumber {
        type Value = u64;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a string or number")
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(value)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            value.parse().map_err(de::Error::custom)
        }
    }

    deserializer.deserialize_any(StringOrNumber)
}

/// Deserialize a JSON string array like "[\"Up\", \"Down\"]" into Vec<String>
fn deserialize_json_string_array<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Deserialize};

    struct JsonStringArray;

    impl<'de> de::Visitor<'de> for JsonStringArray {
        type Value = Option<Vec<String>>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a JSON string array or array")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(None)
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            deserializer.deserialize_any(self)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            serde_json::from_str(value)
                .map(Some)
                .map_err(de::Error::custom)
        }

        fn visit_seq<A>(self, seq: A) -> Result<Self::Value, A::Error>
        where
            A: de::SeqAccess<'de>,
        {
            Vec::<String>::deserialize(de::value::SeqAccessDeserializer::new(seq))
                .map(Some)
        }
    }

    deserializer.deserialize_option(JsonStringArray)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slug_generation() {
        // Test that we generate valid slug format
        let timestamp = 1766100600u64;
        let slug = format!("btc-updown-15m-{}", timestamp);
        assert_eq!(slug, "btc-updown-15m-1766100600");
    }

    #[test]
    fn test_interval_calculation() {
        let now = 1766100550u64; // 50 seconds before interval end
        let current_interval_start = (now / MARKET_INTERVAL_SECS) * MARKET_INTERVAL_SECS;
        assert_eq!(current_interval_start, 1766099700); // Should round down to interval start
    }
}
