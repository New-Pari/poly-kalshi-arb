#!/bin/bash
# Test script to verify VPN routing

echo "Testing Polymarket CLOB connection..."
echo ""

echo "1. Checking WireGuard status:"
sudo wg show

echo ""
echo "2. Testing connectivity to clob.polymarket.com:"
curl -I https://clob.polymarket.com 2>&1 | head -5

echo ""
echo "3. Checking your public IP (should NOT be VPN IP):"
curl -s https://api.ipify.org
echo ""

echo ""
echo "4. Verifying route for Polymarket IPs:"
echo "Route for 172.64.153.51:"
netstat -rn | grep 172.64.153.51 || echo "No specific route found"

echo "Route for 104.18.34.205:"
netstat -rn | grep 104.18.34.205 || echo "No specific route found"
