#!/bin/bash

set -e

ECTAR=../target/debug/ectar

echo "=== Ectar Round-Trip Tests ==="
echo

# Test 1: Small files in single chunk
echo "Test 1: Small files (single chunk)"
echo "-----------------------------------"
rm -rf test1-data test1-out my-backup*
mkdir test1-data
echo "Hello World!" > test1-data/file1.txt
echo "Test file 2" > test1-data/file2.txt
mkdir test1-data/subdir
echo "File in subdirectory" > test1-data/subdir/file3.txt

$ECTAR create --output my-backup --data-shards 6 --parity-shards 3 --chunk-size 1MB test1-data
mkdir test1-out
$ECTAR extract --input "my-backup.c*.s*" --output test1-out

if diff -r test1-data test1-out/test1-data > /dev/null 2>&1; then
    echo "✓ Test 1 PASSED: All files extracted correctly"
else
    echo "✗ Test 1 FAILED: Files differ"
    exit 1
fi
echo

# Test 2: File larger than chunk size (spanning multiple chunks)
echo "Test 2: Large file spanning multiple chunks"
echo "--------------------------------------------"
rm -rf test2-data test2-out spanning-test*
mkdir test2-data
dd if=/dev/urandom of=test2-data/large-file.bin bs=1024 count=300 2>/dev/null
echo "Before" > test2-data/before.txt
echo "After" > test2-data/after.txt

$ECTAR create --output spanning-test --data-shards 10 --parity-shards 5 --chunk-size 50KB test2-data
mkdir test2-out
$ECTAR extract --input "spanning-test.c*.s*" --output test2-out

# Verify checksums
ORIG_SHA=$(shasum -a 256 test2-data/large-file.bin | awk '{print $1}')
EXTRACTED_SHA=$(shasum -a 256 test2-out/test2-data/large-file.bin | awk '{print $1}')

if [ "$ORIG_SHA" = "$EXTRACTED_SHA" ]; then
    echo "✓ Test 2 PASSED: Large file extracted correctly"
    echo "  Original SHA256:  $ORIG_SHA"
    echo "  Extracted SHA256: $EXTRACTED_SHA"
else
    echo "✗ Test 2 FAILED: Checksums don't match"
    echo "  Original:  $ORIG_SHA"
    echo "  Extracted: $EXTRACTED_SHA"
    exit 1
fi
echo

# Test 3: Recovery with missing shards
echo "Test 3: Recovery with missing shards"
echo "-------------------------------------"
rm -rf test3-out
# Delete 3 shards from chunk 2 (can lose up to 5)
rm spanning-test.c002.s00 spanning-test.c002.s03 spanning-test.c002.s07
REMAINING=$(ls spanning-test.c002.s* 2>/dev/null | wc -l)
echo "Deleted 3 shards from chunk 2, $REMAINING remaining (need 10)"

mkdir test3-out
$ECTAR extract --input "spanning-test.c*.s*" --output test3-out

RECOVERED_SHA=$(shasum -a 256 test3-out/test2-data/large-file.bin | awk '{print $1}')

if [ "$ORIG_SHA" = "$RECOVERED_SHA" ]; then
    echo "✓ Test 3 PASSED: File recovered correctly with missing shards"
    echo "  Recovered SHA256: $RECOVERED_SHA"
else
    echo "✗ Test 3 FAILED: Recovery produced incorrect data"
    exit 1
fi
echo

# Test 4: Insufficient shards
echo "Test 4: Insufficient shards (should fail)"
echo "------------------------------------------"
rm -rf test4-out
# Delete 3 more shards from chunk 2 (now only 9 remaining, need 10)
rm spanning-test.c002.s01 spanning-test.c002.s04 spanning-test.c002.s08
REMAINING=$(ls spanning-test.c002.s* 2>/dev/null | wc -l)
echo "Deleted 3 more shards from chunk 2, $REMAINING remaining (need 10)"

mkdir test4-out
if $ECTAR extract --input "spanning-test.c*.s*" --output test4-out 2>&1 | grep -q "insufficient shards"; then
    echo "✓ Test 4 PASSED: Correctly failed with insufficient shards"
else
    echo "✗ Test 4 FAILED: Should have failed but didn't"
    exit 1
fi
echo

# Test 5: Partial extraction flag behavior
echo "Test 5: Partial extraction --partial flag"
echo "------------------------------------------"
# Recreate spanning-test with all chunks intact, then delete only the LAST chunk
# This way chunks 1-6 form a contiguous tar stream
rm -rf test5-out spanning-test*
$ECTAR create --output spanning-test --data-shards 10 --parity-shards 5 --chunk-size 50KB test2-data > /dev/null 2>&1
rm spanning-test.c007.s* 2>/dev/null || true
echo "Deleted all shards from last chunk (chunk 7)"

mkdir test5-out
# This should fail without --partial
if $ECTAR extract --input "spanning-test.c*.s*" --output test5-out 2>&1 | grep -q "Failed to recover"; then
    echo "  Without --partial: correctly failed (expected)"
else
    echo "✗ Test 5a FAILED: Should have failed without --partial flag"
    exit 1
fi

# With --partial, should recover chunks 1-6 and attempt extraction
rm -rf test5-out
mkdir test5-out
if $ECTAR extract --input "spanning-test.c*.s*" --output test5-out --partial 2>&1 | grep -q "Chunks recovered: 6/7"; then
    echo "✓ Test 5 PASSED: --partial flag allows extraction with 6/7 chunks recovered"
else
    echo "✗ Test 5 FAILED: Partial extraction did not recover expected chunks"
    exit 1
fi
echo

echo "=== All Tests Passed! ==="
echo
echo "Summary:"
echo "  ✓ Single chunk archives"
echo "  ✓ Multi-chunk archives (files spanning chunks)"
echo "  ✓ Reed-Solomon error correction (missing shards)"
echo "  ✓ Insufficient shards detection"
echo "  ✓ Partial extraction mode"
