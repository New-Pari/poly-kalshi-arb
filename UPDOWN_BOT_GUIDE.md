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
const ARB_THRESHOLD: f64 = 0.995;     // Execute when YES+NO < 99.5¢ (0.5% profit min)
const MIN_TRADE_SIZE: f64 = 10.0;     // $10 minimum per leg
const MAX_TRADE_SIZE: f64 = 100.0;    // $100 max per leg
```

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

1. **Scanner** discovers current 15-min markets (runs every 30s)
   - Generates slugs: `btc-updown-15m-1766100600`
   - Queries Gamma API for token IDs
   - Auto-rotates to next interval every 15 minutes

2. **WebSocket** subscribes to price feeds
   - Monitors best ask for YES and NO tokens
   - Real-time orderbook updates

3. **Arbitrage detection** triggers when `YES_price + NO_price < threshold`
   - Example: 0.28 + 0.66 = 0.94 < 0.995 → EXECUTE!

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

**Red flags:**
- High unmatched exposure (>5% of positions)
- Declining fill rates (may need to adjust size)
- Execution latency >500ms (network issues)

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

**Position file corrupted:**
- Delete `positions_updown.json` and restart
- Note: Loses historical P&L tracking

## Example Session

```
🎯 Up/Down Arbitrage Bot
   Threshold: <100¢ (0.5% profit)
   Size: $10-$100 per leg
   Mode: DRY RUN

[POLYMARKET] Client ready
[POSITIONS] Loaded from positions_updown.json
   Open positions: 0
   Daily P&L: $0.00
   All-time P&L: $0.00

[UPDOWN] Found 4 active markets
  ✅ BTC | Bitcoin Up or Down - December 18, 7:00PM-7:15PM ET
  ✅ ETH | Ethereum Up or Down - December 18, 7:00PM-7:15PM ET
  ✅ SOL | Solana Up or Down - December 18, 7:00PM-7:15PM ET
  ✅ XRP | XRP Up or Down - December 18, 7:00PM-7:15PM ET

[WS] Connected
[WS] Subscribed to 8 tokens
[SCANNER] 4 active markets: BTC, ETH, SOL, XRP

🎯 ARBITRAGE FOUND: BTC
   Bitcoin Up or Down | YES=0.495 + NO=0.499 = 0.994 → 0.6¢ profit
   Size: $100.00/leg | Profit: $0.60
   ⚠️  DRY RUN - Skipping execution

[Market ends, scanner auto-rotates to next interval]

[UPDOWN] Found 4 active markets
  ✅ BTC | Bitcoin Up or Down - December 18, 7:15PM-7:30PM ET
  ...
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
