# Up/Down Arbitrage Bot - User Guide

## Overview

The Up/Down bot exploits price imbalances in Polymarket's 15-minute BTC/ETH/SOL/XRP markets by simultaneously buying both YES and NO when their sum is below $1.00.

**Strategy**: If YES=28¢ and NO=66¢, total cost is 94¢. When market settles (15 min), one side pays $1.00 → guaranteed 6¢ profit per dollar invested.

## Files

- **`src/bin/updown_bot.rs`** - Main bot (with position tracking)
- **`src/updown_scanner.rs`** - Market discovery module
- **`src/bin/test_updown_scanner.rs`** - Scanner test utility
- **`positions_updown.json`** - Position tracking file (auto-created)

## Running the Bot

```bash
# Test mode (dry run - recommended first!)
DRY_RUN=1 cargo run --release --bin updown_bot

# Live execution (real money!)
DRY_RUN=0 cargo run --release --bin updown_bot
```

## Configuration

Edit constants in `src/bin/updown_bot.rs`:

```rust
const ARB_THRESHOLD: f64 = 0.995;        // Execute when YES+NO < 99.5¢ (0.5% profit min)
const MIN_TRADE_SIZE: f64 = 1.0;         // $1 minimum per leg
const MAX_TRADE_SIZE: f64 = 50.0;        // $50 max per leg
const PRELOAD_BUFFER_SECS: u64 = 60;     // Preload next markets 60s early
```

### Preload Buffer Tuning

**Conservative (30s):**
```rust
const PRELOAD_BUFFER_SECS: u64 = 30;
```
- Pros: Minimal WebSocket load overlap
- Cons: Less time to catch early arbs

**Balanced (60s) - Recommended:**
```rust
const PRELOAD_BUFFER_SECS: u64 = 60;
```
- Pros: Good balance, catches most early opportunities
- Cons: Slight increase in WebSocket traffic for 60s

**Aggressive (120s):**
```rust
const PRELOAD_BUFFER_SECS: u64 = 120;
```
- Pros: Maximum time to detect arbs in next interval
- Cons: 2 minutes of 16 token subscriptions (8 current + 8 next)

## Environment Variables

Required in `.env`:

```bash
POLY_PRIVATE_KEY=0x...        # Your wallet private key
POLY_FUNDER=0x...             # Your wallet address
DRY_RUN=1                     # Set to 0 for live trading
```

## Position Tracking Features

### Automatic Tracking
- **Every fill recorded** - Both YES and NO sides tracked separately
- **Persistent storage** - Survives crashes/restarts (`positions_updown.json`)
- **Cost basis** - Knows exactly what you paid (including slippage)
- **Guaranteed profit** - Calculates locked-in profit for matched pairs

### Real-time Monitoring

**Immediate alerts on execution:**
```
🎯 ARBITRAGE FOUND: BTC
   Bitcoin Up or Down | YES=0.280 + NO=0.660 = 0.940 → 6.0¢ profit
   Size: $50.00/leg | Profit: $3.00
   ⚡ Executing...
   ✅ FILLED in 142ms
      YES: 50.00 @ 0.280 = $14.00
      NO:  50.00 @ 0.660 = $33.00
      Profit: $3.00
   ⚠️  UNMATCHED: 2.00 contracts (YES side)  # Warning if partial fill!
```

**Periodic summary (every 5 minutes):**
```
📊 POSITION SUMMARY
   Open positions: 3
   Total cost basis: $141.00
   Guaranteed profit: $8.47 (6.0%)
   Unmatched exposure: 0 ✅
   Daily P&L: $24.50
   All-time P&L: $387.25
   Open markets:
     • Bitcoin Up or Down | profit: $3.00 | 50 contracts
     • Ethereum Up or Down | profit: $2.97 | 49 contracts
     • Solana Up or Down | profit: $2.50 | 45 contracts
```

### Understanding Unmatched Exposure

**Perfect arb (no risk):**
```json
{
  "poly_yes": { "contracts": 50, "cost_basis": 14.0 },
  "poly_no": { "contracts": 50, "cost_basis": 33.0 },
  "matched_contracts": 50,      // Both sides filled equally
  "unmatched_exposure": 0,      // No directional risk ✅
  "guaranteed_profit": 3.0      // Locked in regardless of outcome
}
```

**Partial fill (has risk!):**
```json
{
  "poly_yes": { "contracts": 50, "cost_basis": 14.0 },
  "poly_no": { "contracts": 45, "cost_basis": 29.7 },  // Only 45 filled!
  "matched_contracts": 45,      // 45 matched pairs
  "unmatched_exposure": 5.0,    // 5 YES contracts exposed ⚠️
  "guaranteed_profit": 1.3      // Less profit due to partial fill
}
```

**Why this matters:**
- **Matched pairs** = guaranteed profit (no market risk)
- **Unmatched exposure** = directional bet (YES or NO must win)
- Bot warns immediately if exposure detected

## Position File Structure

`positions_updown.json` example:

```json
{
  "positions": {
    "Bitcoin Up or Down - December 18, 7:00PM-7:15PM ET": {
      "market_id": "Bitcoin Up or Down - December 18, 7:00PM-7:15PM ET",
      "description": "Bitcoin Up or Down - December 18, 7:00PM-7:15PM ET",
      "kalshi_yes": { "contracts": 0, "cost_basis": 0, "avg_price": 0 },
      "kalshi_no": { "contracts": 0, "cost_basis": 0, "avg_price": 0 },
      "poly_yes": { "contracts": 50, "cost_basis": 14.0, "avg_price": 0.28 },
      "poly_no": { "contracts": 50, "cost_basis": 33.0, "avg_price": 0.66 },
      "total_fees": 0.0,
      "opened_at": "2025-12-18T23:30:00Z",
      "status": "open",
      "realized_pnl": null
    }
  },
  "daily_realized_pnl": 24.50,
  "trading_date": "2025-12-18",
  "all_time_pnl": 387.25
}
```

## How It Works

### Smart Interval-Based Scanning (Event-Driven)

The bot uses **intelligent event-driven scanning** instead of polling:

```
T=0:00    Current interval starts (7:00-7:15 PM)
          Scanner discovers: BTC, ETH, SOL, XRP
          WebSocket subscribes to 8 tokens (YES + NO for each)

T=13:00   Preload trigger (60s before expiry) ⚡
          Scanner fetches NEXT interval (7:15-7:30 PM)
          WebSocket subscribes to 8 MORE tokens

          Now watching BOTH intervals simultaneously:
          - Current: 4 markets (7:00-7:15) - 2 min remaining
          - Next: 4 markets (7:15-7:30) - preloaded!

T=15:00   Current interval expires
          Cleanup: Remove expired markets
          Continue: Already monitoring next interval ✅

          Zero downtime, seamless transition!
```

**Key Features:**
- **93% fewer API calls** - Scans only on market expiry (not every 30s)
- **60-second preload** - Starts watching next markets before they open
- **Zero-latency transitions** - Already subscribed when interval switches
- **Continuous coverage** - No gaps between intervals

### Execution Flow

1. **Scanner** discovers current + preloads next interval
   - Generates slugs: `btc-updown-15m-1766100600`
   - Queries Gamma API for token IDs
   - Preloads next interval 60s early

2. **WebSocket** subscribes to price feeds
   - Monitors best ask for YES and NO tokens
   - Real-time orderbook updates
   - Handles both current AND next markets during preload period

3. **Arbitrage detection** triggers when `YES_price + NO_price < threshold`
   - Example: 0.28 + 0.66 = 0.94 < 0.995 → EXECUTE!
   - Can detect arbs in NEXT interval before it officially starts

4. **Parallel execution** buys both legs simultaneously
   - IOC (Immediate-Or-Cancel) orders
   - Minimizes latency and slippage

5. **Position tracking** records all fills
   - Calculates guaranteed profit
   - Monitors unmatched exposure
   - Persists to disk

## Safety Features

- **Dry run mode** - Test without real money
- **Size limits** - MIN/MAX trade size protection
- **Exposure warnings** - Alerts on partial fills
- **Persistent positions** - Never lose track of open positions
- **No fees on Polymarket** - 0% maker fees = higher profits
- **Auto-reconnect** - WebSocket reconnects on disconnection

## Performance Tips

1. **Run on low-latency server** - Cloud instance near Polymarket servers (US East Coast)
2. **Adjust threshold** - Start conservative (99.5¢), tighten as you observe fills
3. **Monitor fill rates** - If always 100% filled, increase size
4. **Watch unmatched exposure** - Partial fills reduce profit and add risk

## Monitoring

**Key metrics to watch:**
- Fill rate (% of orders fully filled)
- Unmatched exposure (should be 0 or near 0)
- Daily P&L trend
- Execution latency (should be <200ms)

**Scanner health indicators:**

```
[SCANNER] 4 active markets | preload in 813s
```
✅ Good - Scanner operating normally, preload scheduled

```
[SCANNER] Preloaded 4 next markets | total active: 8
```
✅ Good - Preload successful, watching both intervals

```
[SCANNER] Cleaned up 4 expired markets | 4 remain
```
✅ Good - Transition complete, seamless handoff

```
[WS] Subscribed to 16 tokens
```
✅ Expected during preload period (8 current + 8 next)

```
[WS] Subscribed to 8 tokens
```
✅ Normal load after cleanup

**Red flags:**
- High unmatched exposure (>5% of positions)
- Declining fill rates (may need to adjust size)
- Execution latency >500ms (network issues)
- `[SCANNER] Failed to preload next markets` - Will still work but with gap
- `[SCANNER] No active markets found` - Check Gamma API or slug format

## Troubleshooting

**No arbitrages found:**
- Markets are efficient (try lower threshold, e.g., 99.8¢)
- Low volatility period
- Check WebSocket connected and subscribed

**Partial fills:**
- Reduce trade size
- Market liquidity dried up (expected near interval end)

**WebSocket disconnects:**
- Normal - bot auto-reconnects in 5s
- If frequent, check network/VPN

**Scanner issues:**

```
[SCANNER] No active markets found
```
- Slug format may have changed - check Polymarket website for current format
- Gamma API may be down - check API status
- May be between intervals - wait 10s for retry

```
[SCANNER] Failed to preload next markets
```
- Non-critical - bot will still scan at expiry (but with brief gap)
- Check network connectivity
- Gamma API rate limiting (unlikely with only 2 calls per 15 min)

**WebSocket shows 16 tokens but only 4 markets:**
- Normal during 60s preload period (8 current + 8 next)
- Will drop to 8 tokens after cleanup

**Arbitrage found in "future" market:**
- Expected behavior! Preload allows detecting arbs before interval starts
- Market won't settle for 60s, giving time to execute
- This is a feature, not a bug

**Position file corrupted:**
- Delete `positions_updown.json` and restart
- Note: Loses historical P&L tracking

## Example Session

```
🎯 Up/Down Arbitrage Bot
   Threshold: <100¢ (0.5% profit)
   Size: $1-$50 per leg
   Mode: DRY RUN

[POLYMARKET] Client ready
[POSITIONS] Loaded from positions_updown.json
   Open positions: 0
   Daily P&L: $0.00
   All-time P&L: $0.00

[SCANNER] Current: BTC (ends in 873s)
[SCANNER] Current: ETH (ends in 873s)
[SCANNER] Current: SOL (ends in 873s)
[SCANNER] Current: XRP (ends in 873s)
[SCANNER] 4 active markets | preload in 813s | next scan at expiry+60s

[WS] Connected
[WS] Subscribed to 8 tokens

🎯 ARBITRAGE FOUND: BTC
   Bitcoin Up or Down | YES=0.495 + NO=0.499 = 0.994 → 0.6¢ profit
   Size: $50.00/leg | Profit: $0.30
   ⚠️  DRY RUN - Skipping execution

[13 minutes later... preload trigger]

[SCANNER] Preloading next interval (60s early)...
[SCANNER] Next: BTC (starts in 60s)
[SCANNER] Next: ETH (starts in 60s)
[SCANNER] Next: SOL (starts in 60s)
[SCANNER] Next: XRP (starts in 60s)
[SCANNER] Preloaded 4 next markets | total active: 8
[WS] Subscribed to 16 tokens  ← Watching BOTH intervals!

🎯 ARBITRAGE FOUND: BTC
   Bitcoin Up or Down - December 18, 7:15PM-7:30PM ET  ← NEXT interval!
   YES=0.492 + NO=0.503 = 0.995 → 0.5¢ profit
   Size: $50.00/leg | Profit: $0.25
   ⚠️  DRY RUN - Skipping execution

[60 seconds later... current markets expire]

[SCANNER] Waiting 65s for current markets to expire...
[SCANNER] Cleaned up 4 expired markets | 4 remain
[SCANNER] Current: BTC (ends in 893s)  ← Now monitoring what was "next"
[SCANNER] 4 active markets | preload in 833s
[WS] Subscribed to 8 tokens  ← Back to normal

...continues seamlessly...
```

## Advanced: Manual Position Resolution

When markets settle (after 15 minutes), you can manually mark them resolved:

```rust
// In Rust code or modify bot to auto-resolve:
tracker.write().await.resolve_position(
    "Bitcoin Up or Down - December 18, 7:00PM-7:15PM ET",
    true  // or false depending on outcome
);
```

This updates `realized_pnl` and moves position to "resolved" status.

## Files Created

- `positions_updown.json` - Position tracking database
- Logs to stdout (redirect to file if needed)

## Support

For issues or questions:
- Check logs for error messages
- Verify `.env` credentials are correct
- Test scanner separately: `cargo run --release --bin test_updown_scanner`
