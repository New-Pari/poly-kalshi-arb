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
echo "6. Verifying route for Polymarket IPs:"
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
if [ "$RESPONSE" = "403" ]; then
    echo "❌ VPN TEST FAILED - Still getting 403"
    echo ""
    echo "Troubleshooting:"
    echo "  1. Verify your WireGuard config routes ALL traffic (0.0.0.0/0)"
    echo "  2. Check if your VPN exit node is actually in Argentina"
    echo "  3. Try a different VPN server/location"
    echo "  4. Check DNS leaks: curl -s https://ipleak.net/json/"
else
    echo "✅ VPN TEST PASSED - Ready to run bot"
fi
echo "=========================================="
