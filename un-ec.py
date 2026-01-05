#!/usr/bin/env python3
#
# Small utility to un-erasure-code erasure coded files created using the ectar utility
# using commonly available Python packages:
#
# pip install zfec
# uv install zfec
#
# Usage:
#
# Must know the original k+m values, and which shards you have available to decode
#
# k = data shards
# m = parity shards
# n = k+m, total shards
#
# For example, if you have foobar.c001.s00 and foobar.c001.s02, but are missing
# foobar.c001.s01, you would do:
#
# $ python un-ec.py -k 2 -n 3 -o test.tar.zst foobar.c001.s0* --indices 0 2
# Successfully reconstructed: test.tar.zst
#
# IMPORTANT: Reed-Solomon encoding adds padding bytes. To get a valid output,
# you must specify the --size parameter with the exact compressed_size from
# the index file. Without this, the output will have trailing padding bytes
# that corrupt compressed data (zstd will fail with "unknown header").
#
# Example with size parameter:
# $ python un-ec.py -k 3 -n 5 --size 1048177 -o chunk001.zst archive.c001.s0* --indices 0 1 2
# Successfully reconstructed: chunk001.zst (truncated to 1048177 bytes)
#
# To find the correct size, decompress and read the index file:
# $ zstd -d -c archive.index.zst | python3 -m json.tool
# Look for the "compressed_size" field for each chunk.
#
# If using ectar: https://github.com/paulreece42/ectar
# the resultant file is a standard .tar.zst file, which can be further
# decoded using GNU tar and zstd
#
# - 2026-01-04: Paul Reece <paulreece42@gmail.com>
# - 2026-01-05: Nevermind this is already obsolete, you can now
#   simply use the unfec tool available from pip
#

import argparse
import zfec

def decode_raw_shares(input_files, share_indices, k, n, output_path, output_size=None):
    # n = total shares originally created
    # k = pieces actually needed to rebuild

    # 1. Validation: Ensure we have at least k shares
    if len(input_files) < k:
        print(f"Error: Need at least {k} shares, but only {len(input_files)} provided.")
        return

    # 2. Fix: Sub-select exactly k shares and indices if more are provided
    # zfec.decode() strictly requires exactly k blocks.
    if len(input_files) > k:
        print(f"Note: {len(input_files)} shares provided. Using the first {k} to reconstruct.")
        input_files = input_files[:k]
        share_indices = share_indices[:k]

    # Initialize the decoder (zfec uses k for required and n for total)
    decoder = zfec.Decoder(k, n)

    # 3. Read data from the chosen k files
    shares_data = []
    for file_path in input_files:
        with open(file_path, 'rb') as f:
            shares_data.append(f.read())

    # 4. Final check for block consistency
    if not all(len(s) == len(shares_data[0]) for s in shares_data):
        print("Error: All input shares must be the exact same byte length.")
        return

    try:
        # Perform the Reed-Solomon reconstruction
        decoded_blocks = decoder.decode(shares_data, share_indices)

        # 5. Write out the reconstructed data
        with open(output_path, 'wb') as f_out:
            bytes_written = 0
            for block in decoded_blocks:
                if output_size is not None:
                    # Truncate to the specified size to remove RS padding
                    remaining = output_size - bytes_written
                    if remaining <= 0:
                        break
                    block = block[:remaining]
                f_out.write(block)
                bytes_written += len(block)

        if output_size is not None:
            print(f"Successfully reconstructed: {output_path} (truncated to {output_size} bytes)")
        else:
            print(f"Successfully reconstructed: {output_path}")

    except Exception as e:
        print(f"Decoding failed: {e}")

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Decode raw ZFEC shares with k/n subsetting.")
    parser.add_argument("-k", type=int, required=True, help="Minimum shares required (k)")
    parser.add_argument("-n", type=int, required=True, help="Total shares originally created (n)")
    parser.add_argument("-o", "--output", required=True, help="Output file path")
    parser.add_argument("-s", "--size", type=int, default=None,
                        help="Output size in bytes (truncates RS padding). Get this from the 'compressed_size' field in the index file.")
    parser.add_argument("shares", nargs="+", help="Share file paths")
    parser.add_argument("--indices", type=int, nargs="+", required=True,
                        help="Share indices (0 to n-1) for each file in order")

    args = parser.parse_args()

    if len(args.shares) != len(args.indices):
        print("Error: Number of share files must match number of indices provided.")
    else:
        decode_raw_shares(args.shares, args.indices, args.k, args.n, args.output, args.size)

