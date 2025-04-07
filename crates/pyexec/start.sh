#!/bin/sh
sed -i "s|WS_SERVER_HOST|${SERVER_HOST:-ws://localhost:8080}|g" /app/test-client.html
exec http-server -p 3000 --cors