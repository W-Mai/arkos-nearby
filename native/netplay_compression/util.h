/* SPDX-License-Identifier: MIT */
#ifndef NEARBY_COMPRESSION_UTIL_H
#define NEARBY_COMPRESSION_UTIL_H
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

typedef struct {
  uint32_t h[8];
  uint64_t size;
  uint8_t block[64];
  size_t used;
} nearby_sha;
static uint32_t nearby_rotr(uint32_t x, unsigned n) {
  return (x >> n) | (x << (32 - n));
}
static void nearby_sha_block(nearby_sha *s, const uint8_t *p) {
  static const uint32_t k[64] = {
      0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1,
      0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
      0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
      0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
      0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147,
      0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
      0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
      0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
      0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
      0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
      0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2};
  uint32_t w[64];
  for (unsigned i = 0; i < 16; i++)
    w[i] = (uint32_t)p[i * 4] << 24 | (uint32_t)p[i * 4 + 1] << 16 |
           (uint32_t)p[i * 4 + 2] << 8 | p[i * 4 + 3];
  for (unsigned i = 16; i < 64; i++) {
    uint32_t a = nearby_rotr(w[i - 15], 7) ^ nearby_rotr(w[i - 15], 18) ^
                 (w[i - 15] >> 3);
    uint32_t b = nearby_rotr(w[i - 2], 17) ^ nearby_rotr(w[i - 2], 19) ^
                 (w[i - 2] >> 10);
    w[i] = w[i - 16] + a + w[i - 7] + b;
  }
  uint32_t a = s->h[0], b = s->h[1], c = s->h[2], d = s->h[3], e = s->h[4],
           f = s->h[5], g = s->h[6], h = s->h[7];
  for (unsigned i = 0; i < 64; i++) {
    uint32_t t1 =
        h + (nearby_rotr(e, 6) ^ nearby_rotr(e, 11) ^ nearby_rotr(e, 25)) +
        ((e & f) ^ (~e & g)) + k[i] + w[i];
    uint32_t t2 =
        (nearby_rotr(a, 2) ^ nearby_rotr(a, 13) ^ nearby_rotr(a, 22)) +
        ((a & b) ^ (a & c) ^ (b & c));
    h = g;
    g = f;
    f = e;
    e = d + t1;
    d = c;
    c = b;
    b = a;
    a = t1 + t2;
  }
  s->h[0] += a;
  s->h[1] += b;
  s->h[2] += c;
  s->h[3] += d;
  s->h[4] += e;
  s->h[5] += f;
  s->h[6] += g;
  s->h[7] += h;
}
static void nearby_sha_init(nearby_sha *s) {
  *s = (nearby_sha){.h = {0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
                          0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19}};
}
static void nearby_sha_update(nearby_sha *s, const uint8_t *p, size_t n) {
  s->size += n;
  while (n) {
    size_t take = 64 - s->used;
    if (take > n)
      take = n;
    memcpy(s->block + s->used, p, take);
    s->used += take;
    p += take;
    n -= take;
    if (s->used == 64) {
      nearby_sha_block(s, s->block);
      s->used = 0;
    }
  }
}
static void nearby_sha_final(nearby_sha *s, uint8_t out[32]) {
  uint64_t bits = s->size * 8;
  uint8_t pad[128] = {0x80};
  size_t n = s->used < 56 ? 56 - s->used : 120 - s->used;
  for (unsigned i = 0; i < 8; i++)
    pad[n + i] = (uint8_t)(bits >> (56 - 8 * i));
  nearby_sha_update(s, pad, n + 8);
  for (unsigned i = 0; i < 32; i++)
    out[i] = (uint8_t)(s->h[i / 4] >> (24 - 8 * (i % 4)));
}
static uint32_t nearby_le32(const uint8_t *p) {
  return (uint32_t)p[0] | (uint32_t)p[1] << 8 | (uint32_t)p[2] << 16 |
         (uint32_t)p[3] << 24;
}
static bool nearby_netplay_container(const uint8_t *p, uint32_t n) {
  if (!p || n < 24 || n > 64 * 1024 * 1024 || memcmp(p, "NETPLAY\1", 8))
    return false;
  size_t at = 8;
  unsigned blocks = 0;
  bool memory = false, achievements = false;
  while (at + 8 <= n && blocks++ < 3) {
    const uint8_t *tag = p + at;
    uint32_t bytes = nearby_le32(tag + 4);
    at += 8;
    if (!memcmp(tag, "END ", 4))
      return memory && bytes == 0 && at == n;
    if (!memcmp(tag, "MEM ", 4)) {
      if (memory || achievements || !bytes)
        return false;
      memory = true;
    } else if (!memcmp(tag, "ACHV", 4)) {
      if (!memory || achievements || bytes < 8)
        return false;
      achievements = true;
    } else
      return false;
    size_t aligned = ((size_t)bytes + 7) & ~(size_t)7;
    if (aligned > n - at)
      return false;
    at += aligned;
  }
  return false;
}
#endif
