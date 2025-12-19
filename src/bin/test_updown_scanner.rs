// Test program for Up/Down market scanner

use arb_bot::updown_scanner::UpDownScanner;
use tracing::info;

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("arb_bot=debug".parse().unwrap()),
        )
        .init();

    info!("🔍 Testing Up/Down Market Scanner");

    let scanner = UpDownScanner::new();

    // Do a single scan
    match scanner.scan_active_markets().await {
        Ok(markets) => {
            info!("✅ Found {} active Up/Down markets:", markets.len());
            for market in &markets {
                info!("");
                info!("  Market: {}", market.question);
                info!("  Slug:   {}", market.slug);
                info!("  Asset:  {}", market.asset.to_uppercase());
                info!("  YES (Up) token:   {}", market.yes_token);
                info!("  NO (Down) token:  {}", market.no_token);
                info!("  Ends at: {} (Unix timestamp)", market.end_timestamp);
            }

            if markets.is_empty() {
                info!("");
                info!("⚠️  No active Up/Down markets found.");
                info!("    This could mean:");
                info!("    - No markets are currently active");
                info!("    - The slug format has changed");
                info!("    - Markets aren't listed yet (check closer to interval boundary)");
            }
        }
        Err(e) => {
            eprintln!("❌ Scanner failed: {}", e);
        }
    }
}
