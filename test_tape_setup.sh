#!/bin/bash
# Tape Drive Testing Preparation Script

set -e

echo "🔍 Checking for tape devices..."
if ! ls /dev/st* /dev/nst* 2>/dev/null; then
    echo "❌ No tape devices found. Install tape drive or use tape library."
    echo "   Common device paths: /dev/st0, /dev/st1, /dev/nst0, /dev/nst1"
    exit 1
fi

echo "📋 Available tape devices:"
ls -la /dev/st* /dev/nst* 2>/dev/null || true

echo "🔧 Checking tape tools..."
if ! command -v mt >/dev/null 2>&1; then
    echo "⚠️  mt command not found. Install mt-st package:"
    echo "   Ubuntu/Debian: sudo apt install mt-st"
    echo "   CentOS/RHEL: sudo yum install mt-st"
    exit 1
fi

echo "✅ Tape environment ready!"
echo ""
echo "📖 Usage Examples:"
echo "1. Load tapes: mt -f /dev/st0 load"
echo "2. Rewind: mt -f /dev/st0 rewind"
echo "3. Status: mt -f /dev/st0 status"
echo ""
echo "🧪 Test Commands:"
echo "ectar create --tape-devices /dev/st0,/dev/st1,/dev/st2 --block-size 64KB --output backup /data"
echo "ectar extract --tape-devices /dev/st0,/dev/st1,/dev/st2 --input backup.c*.s* --output /restore"