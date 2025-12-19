// Up/Down Arbitrage Bot
//
// Strategy: Buy YES + NO when sum < 100¢ (e.g., 28¢ + 66¢ = 94¢ → 6% profit)
// Markets: BTC, ETH, SOL, XRP 15-minute Up/Down markets

use anyhow::{Context, Result};
use arb_bot::config::POLYMARKET_WS_URL;
use arb_bot::polymarket_clob::{PolymarketAsyncClient, PreparedCreds, SharedAsyncClient};
use arb_bot::updown_scanner::{ActiveUpDownMarket, UpDownScanner};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::{interval, sleep, Instant};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{error, info, warn};

/// Polymarket CLOB API host
const POLY_CLOB_HOST: &str = "https://clob.polymarket.com";
/// Polygon chain ID
const POLYGON_CHAIN_ID: u64 = 137;

/// Arbitrage threshold - sum of YES + NO must be below this for execution
/// Example: 0.94 means 94¢, which gives 6% profit (100¢ - 94¢ = 6¢)
const ARB_THRESHOLD: f64 = 0.94;

/// Minimum size to trade (in dollars)
const MIN_TRADE_SIZE: f64 = 10.0;

/// Maximum size to trade per leg (in dollars)
const MAX_TRADE_SIZE: f64 = 100.0;

/// WebSocket book snapshot
#[derive(Deserialize, Debug)]
struct BookSnapshot {
    asset_id: String,
    #[allow(dead_code)]
    bids: Vec<PriceLevel>,
    asks: Vec<PriceLevel>,
}

#[derive(Deserialize, Debug)]
struct PriceLevel {
    price: String,
    size: String,
}

/// Market state with current prices
#[derive(Debug, Clone)]
struct MarketState {
    asset: String,
    question: String,
    yes_token: String,
    no_token: String,
    yes_price: f64,
    no_price: f64,
    yes_size: f64,
    no_size: f64,
    last_update: Instant,
}

impl MarketState {
    fn new(market: &ActiveUpDownMarket) -> Self {
        Self {
            asset: market.asset.clone(),
            question: market.question.clone(),
            yes_token: market.yes_token.clone(),
            no_token: market.no_token.clone(),
            yes_price: 0.0,
            no_price: 0.0,
            yes_size: 0.0,
            no_size: 0.0,
            last_update: Instant::now(),
        }
    }

    /// Check if arbitrage exists
    fn has_arb(&self) -> bool {
        if self.yes_price <= 0.0 || self.no_price <= 0.0 {
            return false;
        }

        let sum = self.yes_price + self.no_price;
        sum < ARB_THRESHOLD
    }

    /// Calculate expected profit in cents
    fn profit_cents(&self) -> f64 {
        if self.yes_price <= 0.0 || self.no_price <= 0.0 {
            return 0.0;
        }
        (1.0 - (self.yes_price + self.no_price)) * 100.0
    }

    /// Calculate tradeable size based on available liquidity
    fn trade_size(&self) -> f64 {
        // Use the smaller of the two sides to ensure we can fill both
        let available = self.yes_size.min(self.no_size);
        available.min(MAX_TRADE_SIZE).max(MIN_TRADE_SIZE)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("arb_bot=info".parse().unwrap())
                .add_directive("updown_bot=info".parse().unwrap()),
        )
        .init();

    // Load .env file
    dotenvy::dotenv().ok();

    info!("🎯 Up/Down Arbitrage Bot");
    info!("   Threshold: <{:.0}¢ ({:.1}% profit)", ARB_THRESHOLD * 100.0, (1.0 - ARB_THRESHOLD) * 100.0);
    info!("   Size: ${:.0}-${:.0} per leg", MIN_TRADE_SIZE, MAX_TRADE_SIZE);

    // Check for dry run mode
    let dry_run = std::env::var("DRY_RUN")
        .map(|v| v == "1" || v == "true")
        .unwrap_or(true);

    if dry_run {
        info!("   Mode: DRY RUN (set DRY_RUN=0 to execute)");
    } else {
        warn!("   Mode: LIVE EXECUTION");
    }

    // Load Polymarket credentials
    let poly_private_key = std::env::var("POLY_PRIVATE_KEY")
        .context("POLY_PRIVATE_KEY not set")?;
    let poly_funder = std::env::var("POLY_FUNDER")
        .context("POLY_FUNDER not set (your wallet address)")?;

    // Create async Polymarket client
    info!("[POLYMARKET] Creating async client...");
    let poly_async_client = PolymarketAsyncClient::new(
        POLY_CLOB_HOST,
        POLYGON_CHAIN_ID,
        &poly_private_key,
        &poly_funder,
    )?;
    let api_creds = poly_async_client.derive_api_key(0).await?;
    let prepared_creds = PreparedCreds::from_api_creds(&api_creds)?;
    let poly_client = Arc::new(SharedAsyncClient::new(
        poly_async_client,
        prepared_creds,
        POLYGON_CHAIN_ID,
    ));

    info!("[POLYMARKET] Client ready");

    // Create scanner
    let scanner = UpDownScanner::new();

    // Shared state for active markets
    let markets: Arc<RwLock<HashMap<String, MarketState>>> = Arc::new(RwLock::new(HashMap::new()));

    // Market scanner task - refreshes every 30s
    let scanner_markets = markets.clone();
    let scanner_handle = tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(30));
        loop {
            interval.tick().await;

            match scanner.scan_active_markets().await {
                Ok(active_markets) => {
                    let mut map = scanner_markets.write().await;

                    // Remove expired markets
                    let active_tokens: Vec<String> = active_markets
                        .iter()
                        .flat_map(|m| vec![m.yes_token.clone(), m.no_token.clone()])
                        .collect();

                    map.retain(|_, state| {
                        active_tokens.contains(&state.yes_token)
                    });

                    // Add new markets
                    for market in &active_markets {
                        if !map.contains_key(&market.yes_token) {
                            map.insert(market.yes_token.clone(), MarketState::new(market));
                        }
                    }

                    info!("[SCANNER] {} active markets: {}",
                          map.len(),
                          map.values()
                            .map(|m| m.asset.to_uppercase())
                            .collect::<Vec<_>>()
                            .join(", "));
                }
                Err(e) => {
                    warn!("[SCANNER] Failed: {}", e);
                }
            }
        }
    });

    // WebSocket price feed task
    let ws_markets = markets.clone();
    let ws_poly_client = poly_client.clone();
    let ws_handle = tokio::spawn(async move {
        loop {
            if let Err(e) = run_ws_feed(ws_markets.clone(), ws_poly_client.clone(), dry_run).await {
                error!("[WS] Disconnected: {} - reconnecting in 5s...", e);
                sleep(Duration::from_secs(5)).await;
            }
        }
    });

    // Wait for tasks
    let _ = tokio::join!(scanner_handle, ws_handle);

    Ok(())
}

/// Run WebSocket price feed
async fn run_ws_feed(
    markets: Arc<RwLock<HashMap<String, MarketState>>>,
    poly_client: Arc<SharedAsyncClient>,
    dry_run: bool,
) -> Result<()> {
    // Get token list
    let tokens = {
        let map = markets.read().await;
        map.values()
            .flat_map(|m| vec![m.yes_token.clone(), m.no_token.clone()])
            .collect::<Vec<_>>()
    };

    if tokens.is_empty() {
        info!("[WS] No markets to monitor, waiting...");
        sleep(Duration::from_secs(10)).await;
        return Ok(());
    }

    info!("[WS] Connecting to Polymarket WebSocket...");
    let (ws_stream, _) = connect_async(POLYMARKET_WS_URL).await?;
    info!("[WS] Connected");

    let (mut write, mut read) = ws_stream.split();

    // Subscribe to all tokens
    let subscribe_msg = serde_json::json!({
        "assets_ids": tokens,
        "type": "market"
    });

    write
        .send(Message::Text(serde_json::to_string(&subscribe_msg)?))
        .await?;
    info!("[WS] Subscribed to {} tokens", tokens.len());

    let mut ping_interval = interval(Duration::from_secs(30));
    let mut last_message = Instant::now();

    loop {
        tokio::select! {
            _ = ping_interval.tick() => {
                if let Err(e) = write.send(Message::Ping(vec![])).await {
                    error!("[WS] Failed to send ping: {}", e);
                    break;
                }
            }

            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        last_message = Instant::now();

                        // Try to parse as book snapshot
                        if let Ok(books) = serde_json::from_str::<Vec<BookSnapshot>>(&text) {
                            for book in &books {
                                if let Err(e) = process_book(&markets, &poly_client, book, dry_run).await {
                                    warn!("[WS] Error processing book: {}", e);
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        let _ = write.send(Message::Pong(data)).await;
                        last_message = Instant::now();
                    }
                    Some(Ok(Message::Pong(_))) => {
                        last_message = Instant::now();
                    }
                    Some(Ok(Message::Close(frame))) => {
                        warn!("[WS] Server closed: {:?}", frame);
                        break;
                    }
                    Some(Err(e)) => {
                        error!("[WS] Error: {}", e);
                        break;
                    }
                    None => {
                        warn!("[WS] Stream ended");
                        break;
                    }
                    _ => {}
                }
            }
        }

        // Check for stale connection
        if last_message.elapsed() > Duration::from_secs(120) {
            warn!("[WS] Stale connection, reconnecting...");
            break;
        }
    }

    Ok(())
}

/// Process book snapshot and check for arbitrage
async fn process_book(
    markets: &Arc<RwLock<HashMap<String, MarketState>>>,
    poly_client: &Arc<SharedAsyncClient>,
    book: &BookSnapshot,
    dry_run: bool,
) -> Result<()> {
    // Find best ask (lowest price for buying)
    let best_ask = book
        .asks
        .iter()
        .filter_map(|l| {
            let price: f64 = l.price.parse().ok()?;
            let size: f64 = l.size.parse().ok()?;
            if price > 0.0 && size > 0.0 {
                Some((price, size))
            } else {
                None
            }
        })
        .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
        .unwrap_or((0.0, 0.0));

    if best_ask.0 == 0.0 {
        return Ok(());
    }

    // Update market state
    let mut map = markets.write().await;

    // Find which market this token belongs to
    let mut updated_market: Option<MarketState> = None;

    for state in map.values_mut() {
        if state.yes_token == book.asset_id {
            state.yes_price = best_ask.0;
            state.yes_size = best_ask.1;
            state.last_update = Instant::now();

            // Check for arb after update
            if state.has_arb() {
                updated_market = Some(state.clone());
            }
            break;
        } else if state.no_token == book.asset_id {
            state.no_price = best_ask.0;
            state.no_size = best_ask.1;
            state.last_update = Instant::now();

            // Check for arb after update
            if state.has_arb() {
                updated_market = Some(state.clone());
            }
            break;
        }
    }

    drop(map); // Release lock before execution

    // Execute if arb found
    if let Some(state) = updated_market {
        execute_arb(poly_client, &state, dry_run).await?;
    }

    Ok(())
}

/// Execute arbitrage trade
async fn execute_arb(
    poly_client: &Arc<SharedAsyncClient>,
    state: &MarketState,
    dry_run: bool,
) -> Result<()> {
    let profit = state.profit_cents();
    let size = state.trade_size();

    info!("");
    info!("🎯 ARBITRAGE FOUND: {}", state.asset.to_uppercase());
    info!("   {} | YES={:.3} + NO={:.3} = {:.3} → {:.1}¢ profit",
          state.question.split('-').next().unwrap_or(&state.question),
          state.yes_price,
          state.no_price,
          state.yes_price + state.no_price,
          profit);
    info!("   Size: ${:.2}/leg | Profit: ${:.2}",
          size,
          (size * profit) / 100.0);

    if dry_run {
        info!("   ⚠️  DRY RUN - Skipping execution");
        return Ok(());
    }

    // Execute both legs in parallel
    info!("   ⚡ Executing...");
    let start = Instant::now();

    let yes_fut = poly_client.buy_ioc(&state.yes_token, state.yes_price, size);
    let no_fut = poly_client.buy_ioc(&state.no_token, state.no_price, size);

    let (yes_result, no_result) = tokio::join!(yes_fut, no_fut);

    let elapsed = start.elapsed();

    match (yes_result, no_result) {
        (Ok(yes_fill), Ok(no_fill)) => {
            let total_cost = yes_fill.fill_cost + no_fill.fill_cost;
            let actual_profit = (yes_fill.filled_size.min(no_fill.filled_size)) - total_cost;

            info!("   ✅ FILLED in {:.0}ms", elapsed.as_millis());
            info!("      YES: {:.2} @ {:.3} = ${:.2}",
                  yes_fill.filled_size, state.yes_price, yes_fill.fill_cost);
            info!("      NO:  {:.2} @ {:.3} = ${:.2}",
                  no_fill.filled_size, state.no_price, no_fill.fill_cost);
            info!("      Profit: ${:.2}", actual_profit);
        }
        (Err(e), _) | (_, Err(e)) => {
            error!("   ❌ FAILED: {}", e);
        }
    }

    Ok(())
}
