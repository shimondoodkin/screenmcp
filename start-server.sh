#!/bin/bash
# ScreenMCP server startup script
# Starts socat SSL terminators + Docker Compose

CERT=/etc/letsencrypt/live/server10.doodkin.com/fullchain.pem
KEY=/etc/letsencrypt/live/server10.doodkin.com/privkey.pem

echo 'Stopping old socat...'
pkill -f 'socat OPENSSL-LISTEN:443' 2>/dev/null
pkill -f 'socat OPENSSL-LISTEN:8443' 2>/dev/null
sleep 1

echo 'Starting socat SSL terminators...'
echo '  :443  (HTTPS) -> localhost:3000 (Next.js)'
echo '  :8443 (WSS)   -> localhost:8080 (Rust worker)'
nohup socat OPENSSL-LISTEN:443,fork,reuseaddr,verify=0,cert=$CERT,key=$KEY TCP:localhost:3000 > /dev/null 2>&1 &
nohup socat OPENSSL-LISTEN:8443,fork,reuseaddr,verify=0,cert=$CERT,key=$KEY TCP:localhost:8080 > /dev/null 2>&1 &
sleep 1

echo 'Starting Docker Compose...'
cd /home/user/screenmcp
# Env vars read from .env automatically by Docker Compose
docker compose up --build
