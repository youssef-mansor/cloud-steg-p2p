#!/bin/bash

# Simple netcat listener on port 8000
# Usage: ./listen.sh

PORT=8000
echo "Starting netcat listener on port $PORT..."
echo "Waiting for connections... (Ctrl+C to stop)"
echo ""

nc -l $PORT
