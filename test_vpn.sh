#!/bin/bash
# Test script to verify VPN routing to Argentina for Polymarket

set -e

echo "=========================================="
echo "Testing Polymarket CLOB via Argentina VPN"
echo "=========================================="
echo ""

echo "1. Checking WireGuard status:"
if sudo wg show &> /dev/null; then
    sudo wg show
    echo "✅ WireGuard is running"
else
    echo "❌ WireGuard is NOT running! Start it first."
    exit 1
fi

echo ""
echo "2. Checking your public IP (should be Argentina):"
PUBLIC_IP=$(curl -s https://api.ipify.org)
echo "Your IP: $PUBLIC_IP"

# Check geolocation
echo "IP Location:"
curl -s "https://ipapi.co/$PUBLIC_IP/json/" | jq -r '"Country: \(.country_name) (\(.country_code))\nCity: \(.city)\nISP: \(.org)"' 2>/dev/null || echo "Install jq for better output"

echo ""
echo "3. Testing connectivity to clob.polymarket.com:"
RESPONSE=$(curl -s -o /dev/null -w "%{http_code}" https://clob.polymarket.com)
if [ "$RESPONSE" = "200" ] || [ "$RESPONSE" = "301" ] || [ "$RESPONSE" = "302" ]; then
    echo "✅ clob.polymarket.com: HTTP $RESPONSE (accessible)"
else
    echo "❌ clob.polymarket.com: HTTP $RESPONSE (blocked?)"
fi

echo ""
echo "4. Testing with bot-like headers (simulating actual bot request):"
RESPONSE=$(curl -s -o response.html -w "%{http_code}" \
    -H "User-Agent: py_clob_client" \
    -H "Accept: */*" \
    -H "Connection: keep-alive" \
    -H "Content-Type: application/json" \
    https://clob.polymarket.com)

if [ "$RESPONSE" = "403" ]; then
    echo "❌ HTTP 403 Forbidden - Cloudflare is blocking!"
    echo "Response preview:"
    head -20 response.html
    rm response.html
    exit 1
elif [ "$RESPONSE" = "200" ] || [ "$RESPONSE" = "301" ] || [ "$RESPONSE" = "302" ]; then
    echo "✅ HTTP $RESPONSE - No blocking detected"
    rm -f response.html
else
    echo "⚠️  HTTP $RESPONSE - Unexpected response"
    rm -f response.html
fi

echo ""
echo "5. Testing Gamma API (market data):"
RESPONSE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "User-Agent: py_clob_client" \
    https://gamma-api.polymarket.com/markets)

if [ "$RESPONSE" = "200" ]; then
    echo "✅ gamma-api.polymarket.com: HTTP $RESPONSE (accessible)"
else
    echo "❌ gamma-api.polymarket.com: HTTP $RESPONSE"
fi

echo ""
echo "6. Measuring API latency & speed:"
echo ""

# DNS resolution time
echo "DNS Resolution Time:"
DNS_START=$(date +%s%N)
dig +short clob.polymarket.com > /dev/null
DNS_END=$(date +%s%N)
DNS_MS=$(( ($DNS_END - $DNS_START) / 1000000 ))
echo "  clob.polymarket.com: ${DNS_MS}ms"
if [ $DNS_MS -lt 50 ]; then
    echo "  ✅ Excellent DNS speed"
elif [ $DNS_MS -lt 200 ]; then
    echo "  ⚠️  Acceptable DNS speed"
else
    echo "  ❌ Slow DNS (may need better DNS server)"
fi

echo ""
echo "API Request Timing (clob.polymarket.com):"
TIMING=$(curl -s -o /dev/null -w "\
  DNS lookup:     %{time_namelookup}s\n\
  TCP connect:    %{time_connect}s\n\
  TLS handshake:  %{time_appconnect}s\n\
  First byte:     %{time_starttransfer}s\n\
  Total time:     %{time_total}s" \
    https://clob.polymarket.com)
echo "$TIMING"

# Extract total time for threshold check
TOTAL_TIME=$(echo "$TIMING" | grep "Total time" | awk '{print $3}' | sed 's/s//')
TOTAL_MS=$(echo "$TOTAL_TIME * 1000" | bc)

echo ""
if (( $(echo "$TOTAL_MS < 100" | bc -l) )); then
    echo "  ✅ Excellent latency (<100ms) - Perfect for HFT!"
elif (( $(echo "$TOTAL_MS < 200" | bc -l) )); then
    echo "  ✅ Good latency (<200ms) - Suitable for arb bot"
elif (( $(echo "$TOTAL_MS < 500" | bc -l) )); then
    echo "  ⚠️  Acceptable latency (<500ms) - May miss some opportunities"
else
    echo "  ❌ High latency (>500ms) - Consider different VPN/server"
fi

echo ""
echo "API Request Timing (gamma-api.polymarket.com):"
GAMMA_TIMING=$(curl -s -o /dev/null -w "\
  DNS lookup:     %{time_namelookup}s\n\
  TCP connect:    %{time_connect}s\n\
  TLS handshake:  %{time_appconnect}s\n\
  First byte:     %{time_starttransfer}s\n\
  Total time:     %{time_total}s" \
    https://gamma-api.polymarket.com/markets?limit=1)
echo "$GAMMA_TIMING"

echo ""
echo "Multiple Request Speed Test (5 consecutive API calls):"
START_TIME=$(date +%s%N)
for i in {1..5}; do
    curl -s -o /dev/null https://clob.polymarket.com
done
END_TIME=$(date +%s%N)
AVG_MS=$(( ($END_TIME - $START_TIME) / 5000000 ))
echo "  Average per request: ${AVG_MS}ms"

if [ $AVG_MS -lt 150 ]; then
    echo "  ✅ Fast connection - Keep-alive working well"
elif [ $AVG_MS -lt 300 ]; then
    echo "  ⚠️  Moderate speed"
else
    echo "  ❌ Slow connection - Check network quality"
fi

echo ""
echo "WebSocket Connection Test (ws.polymarket.com):"
# Test WebSocket connection time (if wscat is installed)
if command -v wscat &> /dev/null; then
    WS_START=$(date +%s%N)
    timeout 2 wscat -c wss://ws-subscriptions-clob.polymarket.com/ws/market --execute 'ping' &> /dev/null || true
    WS_END=$(date +%s%N)
    WS_MS=$(( ($WS_END - $WS_START) / 1000000 ))

    if [ $WS_MS -lt 2000 ]; then
        echo "  WebSocket connect time: ${WS_MS}ms"
        if [ $WS_MS -lt 300 ]; then
            echo "  ✅ Fast WebSocket connection"
        else
            echo "  ⚠️  Slow WebSocket connection"
        fi
    else
        echo "  ⚠️  WebSocket test timed out (may not accept test connections)"
    fi
else
    echo "  ⏭️  Skipped (install wscat with: npm install -g wscat)"
fi

echo ""
echo "7. Verifying route for Polymarket IPs:"
POLY_IPS=$(dig +short clob.polymarket.com | grep -E '^[0-9]+\.' | head -3)
echo "Polymarket IPs:"
echo "$POLY_IPS"

echo ""
echo "Checking routes for these IPs:"
for ip in $POLY_IPS; do
    echo "  $ip:"
    netstat -rn | grep "$ip" || echo "    → Using default route (through VPN)"
done

echo ""
echo "=========================================="
echo "TEST RESULTS SUMMARY"
echo "=========================================="

if [ "$RESPONSE" = "403" ]; then
    echo "❌ VPN TEST FAILED - Still getting 403"
    echo ""
    echo "Troubleshooting:"
    echo "  1. Verify your WireGuard config routes ALL traffic (0.0.0.0/0)"
    echo "  2. Check if your VPN exit node is actually in Argentina"
    echo "  3. Try a different VPN server/location"
    echo "  4. Check DNS leaks: curl -s https://ipleak.net/json/"
else
    echo "✅ VPN ACCESS: Working"

    # Speed assessment
    echo ""
    echo "SPEED ASSESSMENT:"
    if (( $(echo "$TOTAL_MS < 200" | bc -l) )); then
        echo "  ✅ EXCELLENT - Latency ${TOTAL_MS}ms is great for arbitrage"
        echo "     Your bot should catch most opportunities!"
    elif (( $(echo "$TOTAL_MS < 500" | bc -l) )); then
        echo "  ⚠️  ACCEPTABLE - Latency ${TOTAL_MS}ms is usable"
        echo "     You may miss some fast-moving arbs"
        echo "     Consider: VPN server closer to Polymarket (US East Coast)"
    else
        echo "  ❌ SLOW - Latency ${TOTAL_MS}ms is too high"
        echo "     Recommendation: Find a faster VPN or cloud server"
        echo "     Optimal location: AWS us-east-1 (Virginia)"
    fi

    echo ""
    echo "RECOMMENDATIONS:"
    echo "  • For best results: <100ms latency (HFT-grade)"
    echo "  • Acceptable range: 100-200ms (good for arb bot)"
    echo "  • If >200ms: Consider cloud server (AWS/GCP in us-east-1)"
fi
echo "=========================================="
