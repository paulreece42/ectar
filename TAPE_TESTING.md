# Testing LTO Tape Drive Support

## Prerequisites

### Hardware Requirements
- **3 LTO tape drives** (LTO-6, LTO-7, LTO-8, or LTO-9 recommended)
- **SCSI/SAS tape controller** or **USB-attached drives**
- **Blank or erasable LTO tapes** (one per drive)

### Software Requirements
```bash
# Ubuntu/Debian
sudo apt update
sudo apt install mt-st lsscsi sg3-utils

# CentOS/RHEL
sudo yum install mt-st lsscsi sg3_utils

# Verify installation
mt --version
lsscsi
```

### Device Detection
```bash
# List all SCSI devices
lsscsi

# List tape devices specifically
lsscsi | grep tape

# Check device permissions
ls -la /dev/st* /dev/nst*
```

## Safety Considerations

### ⚠️ Critical Warnings

1. **Data Loss Risk**: Tape operations are destructive - existing data will be overwritten
2. **Hardware Damage**: Incorrect block sizes can damage tape drives
3. **Cost**: LTO tapes are expensive (~$50-100 each)
4. **Time**: Tape operations are slow (10-50 MB/s typical)

### 🛡️ Safety Measures

1. **Use blank tapes only**
2. **Verify device paths before testing**
3. **Start with small test files**
4. **Monitor drive status during operations**

## Testing Steps

### 1. Device Preparation
```bash
# Load tapes into drives
mt -f /dev/st0 load
mt -f /dev/st1 load
mt -f /dev/st2 load

# Rewind all tapes
mt -f /dev/st0 rewind
mt -f /dev/st1 rewind
mt -f /dev/st2 rewind

# Check status
mt -f /dev/st0 status
mt -f /dev/st1 status
mt -f /dev/st2 status
```

### 2. Determine Block Size
```bash
# Check drive capabilities
mt -f /dev/st0 stoptions

# Common LTO block sizes:
# LTO-6/LTO-7: 512 bytes (fixed) or variable
# LTO-8/LTO-9: 512 bytes or larger

# For testing, start with 512 bytes
BLOCK_SIZE="512"
```

### 3. Create Test Data
```bash
# Create test directory with various file types
mkdir -p /tmp/tape_test
cd /tmp/tape_test

# Create test files
echo "Test file 1" > file1.txt
echo "Test file 2" > file2.txt
dd if=/dev/urandom of=random_10MB.dat bs=1M count=10
tar czf archive.tar.gz file1.txt file2.txt

# Create subdirectory
mkdir subdir
cp file1.txt subdir/
```

### 4. Test Archive Creation
```bash
# Basic test with small files
./target/release/ectar create \
  --tape-devices /dev/st0,/dev/st1,/dev/st2 \
  --block-size $BLOCK_SIZE \
  --output tape_backup \
  /tmp/tape_test/file1.txt

# Monitor progress
watch -n 5 'mt -f /dev/st0 status; mt -f /dev/st1 status; mt -f /dev/st2 status'
```

### 5. Test Archive Listing
```bash
# This currently won't work with tapes (needs file-based index)
# Future enhancement needed for tape-based index reading
```

### 6. Test Archive Extraction
```bash
# Rewind tapes first
mt -f /dev/st0 rewind
mt -f /dev/st1 rewind
mt -f /dev/st2 rewind

# Extract to different location
mkdir -p /tmp/tape_restore
./target/release/ectar extract \
  --tape-devices /dev/st0,/dev/st1,/dev/st2 \
  --input tape_backup.c*.s* \
  --output /tmp/tape_restore
```

### 7. Verify Results
```bash
# Compare original and restored files
diff /tmp/tape_test/file1.txt /tmp/tape_restore/file1.txt

# Check file sizes and permissions
ls -la /tmp/tape_test/ /tmp/tape_restore/
```

## Expected Behavior

### Successful Operation
- Archive creation completes without errors
- Progress indication (when implemented)
- Each tape drive receives different shard data
- Index file created (for file-based archives)

### Error Scenarios to Test
1. **Missing tape device**: Should fail gracefully
2. **Write-protected tape**: Should detect and report
3. **End of tape**: Should handle tape changes (future feature)
4. **Invalid block size**: Should validate against drive capabilities

## Performance Expectations

- **Write Speed**: 10-50 MB/s per drive (depending on LTO generation)
- **RAIT Overhead**: Minimal (primarily coordination overhead)
- **Recovery Speed**: Similar to write speed for reconstruction

## Troubleshooting

### Common Issues

1. **Permission Denied**
   ```bash
   sudo chmod 666 /dev/st*
   # OR add user to tape group
   sudo usermod -a -G tape $USER
   ```

2. **Device Busy**
   ```bash
   # Check what's using the device
   fuser /dev/st0
   # Kill interfering processes or reboot
   ```

3. **Tape Not Loaded**
   ```bash
   mt -f /dev/st0 load
   mt -f /dev/st0 status  # Verify BOT (Beginning of Tape)
   ```

4. **Block Size Mismatch**
   ```bash
   # Check drive's current block size
   mt -f /dev/st0 stsetoptions scsi2logical
   mt -f /dev/st0 status
   ```

### Debug Commands
```bash
# Enable verbose logging
export RUST_LOG=debug
./target/release/ectar create --tape-devices /dev/st0,/dev/st1,/dev/st2 --output test /tmp/data

# Monitor system logs
sudo dmesg | tail -f | grep -i tape

# Check kernel messages
sudo journalctl -f | grep -i tape
```

## Future Enhancements Needed

1. **Tape Library Support**: Automated tape loading/unloading
2. **End-of-Tape Handling**: Automatic tape changes during large archives
3. **Tape Status Monitoring**: Real-time drive and media status
4. **Tape Cataloging**: Track what archives are on which tapes
5. **Multi-Volume Archives**: Archives spanning multiple tapes

## Cleanup

```bash
# Rewind and unload tapes
mt -f /dev/st0 rewind
mt -f /dev/st1 rewind
mt -f /dev/st2 rewind

mt -f /dev/st0 offline
mt -f /dev/st1 offline
mt -f /dev/st2 offline

# Remove test data
rm -rf /tmp/tape_test /tmp/tape_restore
```