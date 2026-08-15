//! 64-bit FNV-1a content hash for local cache keys.
//!
//! This is not a cryptographic or security boundary. It replaces 32-bit
//! hashes in render/editor caches so accidental collisions are no longer a
//! realistic correctness risk while keeping the API synchronous.

const FNV_OFFSET_BASIS = 0xcbf29ce484222325n;
const FNV_PRIME = 0x100000001b3n;

export function contentHash64(value: string): string {
  let hash = FNV_OFFSET_BASIS;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= BigInt(value.charCodeAt(index));
    hash = BigInt.asUintN(64, hash * FNV_PRIME);
  }
  return hash.toString(16).padStart(16, "0");
}
