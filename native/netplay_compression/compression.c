/* SPDX-License-Identifier: MIT */
#define _GNU_SOURCE
#include "util.h"
#include <fcntl.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <unistd.h>

#define OWNED_LIBRARY "/opt/arkos-nearby/assets/netplay-compression.so"
#define BACKEND_ADDRESS UINT64_C(0x11262d8)
#define NETPLAY_SET_IN_RETURN UINT64_C(0xad34e0)
#define NETPLAY_SET_OUT_RETURN UINT64_C(0xad350c)
#define NETPLAY_TRANS_RETURN UINT64_C(0xad3538)

/* ABI declaration from RetroArch v1.22.2 trans_stream.h; see ABI-COPYING.txt.
 */
struct backend {
  const char *ident;
  const struct backend *reverse;
  void *(*create)(void);
  void (*destroy)(void *);
  bool (*define)(void *, const char *, uint32_t);
  void (*set_in)(void *, const uint8_t *, uint32_t);
  void (*set_out)(void *, uint8_t *, uint32_t);
  bool (*trans)(void *, bool, uint32_t *, uint32_t *, int *);
};

static struct backend original;
static unsigned installed, completed, failures;
static __thread struct {
  void *stream;
  bool output_seen;
} pending;

static void report_failure(const char *reason) {
  if (__atomic_fetch_add(&failures, 1, __ATOMIC_RELAXED) < 4)
    fprintf(stderr, "[Nearby] Netplay compression: %s\n", reason);
}

static bool executable_matches(void) {
  static const uint8_t expected[32] = {
      0x81, 0xf3, 0xcf, 0x1b, 0x88, 0x50, 0xf9, 0x45, 0x77, 0x30, 0x34,
      0x6d, 0x3f, 0x97, 0x3d, 0xfb, 0x03, 0xd5, 0x31, 0x6d, 0x62, 0xc4,
      0xbb, 0x92, 0xb1, 0x65, 0x43, 0x72, 0xbb, 0xc8, 0x6e, 0x77};
  int fd = open("/proc/self/exe", O_RDONLY | O_CLOEXEC);
  if (fd < 0)
    return false;
  struct stat st;
  if (fstat(fd, &st) || st.st_size != 14691912) {
    close(fd);
    return false;
  }
  nearby_sha sha;
  nearby_sha_init(&sha);
  uint8_t buffer[32768], digest[32];
  ssize_t n;
  do {
    n = read(fd, buffer, sizeof buffer);
    if (n > 0)
      nearby_sha_update(&sha, buffer, (size_t)n);
  } while (n > 0);
  close(fd);
  if (n < 0)
    return false;
  nearby_sha_final(&sha, digest);
  return !memcmp(digest, expected, sizeof digest);
}

static bool owned_config(const char *path) {
  const char *prefix = "/run/arkos-nearby-rust/";
  if (strncmp(path, prefix, strlen(prefix)))
    return false;
  const char *id = path + strlen(prefix);
  return strlen(id) == 46 && strspn(id, "0123456789abcdef") == 32 &&
         !strcmp(id + 32, "/game/base.cfg");
}

static bool game_arguments(const char *bytes, size_t size) {
  if (!size || size >= 8192 || bytes[size - 1])
    return false;
  const char *args[256];
  size_t count = 0, at = 0;
  while (at < size) {
    if (count == sizeof args / sizeof args[0])
      return false;
    args[count++] = bytes + at;
    size_t length = strnlen(bytes + at, size - at);
    if (length == size - at)
      return false;
    at += length + 1;
  }
  const char *config = NULL, *append = NULL;
  bool role = false, port = false;
  for (size_t i = 1; i < count; i++) {
    if (!strcmp(args[i], "-c") && i + 1 < count)
      config = args[++i];
    else if (!strcmp(args[i], "--appendconfig") && i + 1 < count)
      append = args[++i];
    else if (!strcmp(args[i], "-H") ||
             !strcmp(args[i], "--connect=192.168.49.1"))
      role = true;
    else if (!strcmp(args[i], "--port=55435"))
      port = true;
  }
  if (!config || !append || !owned_config(config) || !role || !port)
    return false;
  size_t prefix = strlen(config) - strlen("base.cfg");
  return strlen(append) == prefix + strlen("netplay.cfg") &&
         !strncmp(config, append, prefix) &&
         !strcmp(append + prefix, "netplay.cfg");
}

static bool owned_launch(void) {
  int fd = open("/proc/self/cmdline", O_RDONLY | O_CLOEXEC);
  if (fd < 0)
    return false;
  char bytes[8192];
  ssize_t n = read(fd, bytes, sizeof bytes);
  close(fd);
  return n > 0 && game_arguments(bytes, (size_t)n);
}

static __attribute__((noinline)) void
compress_in(void *stream, const uint8_t *data, uint32_t size) {
  uintptr_t caller = (uintptr_t)__builtin_return_address(0);
  if (caller == NETPLAY_SET_IN_RETURN && stream &&
      nearby_netplay_container(data, size)) {
    if (original.define(stream, "level", 1)) {
      pending.stream = stream;
      pending.output_seen = false;
    } else {
      pending.stream = NULL;
      report_failure("level selection failed");
    }
  }
  original.set_in(stream, data, size);
}

static __attribute__((noinline)) void compress_out(void *stream, uint8_t *data,
                                                   uint32_t size) {
  if ((uintptr_t)__builtin_return_address(0) == NETPLAY_SET_OUT_RETURN &&
      pending.stream && pending.stream == stream)
    pending.output_seen = true;
  original.set_out(stream, data, size);
}

static __attribute__((noinline)) bool compress_trans(void *stream, bool flush,
                                                     uint32_t *read_bytes,
                                                     uint32_t *written,
                                                     int *error) {
  bool selected =
      (uintptr_t)__builtin_return_address(0) == NETPLAY_TRANS_RETURN &&
      pending.stream && pending.stream == stream && pending.output_seen &&
      flush;
  bool result = original.trans(stream, flush, read_bytes, written, error);
  if (selected) {
    if (!original.define(stream, "level", 9))
      report_failure("level restoration failed");
    if (result)
      __atomic_fetch_add(&completed, 1, __ATOMIC_RELAXED);
    pending.stream = NULL;
    pending.output_seen = false;
  }
  return result;
}

__attribute__((constructor)) static void initialize(void) {
  const char *preload = getenv("LD_PRELOAD");
  if (preload && !strcmp(preload, OWNED_LIBRARY))
    unsetenv("LD_PRELOAD");
  char executable[128];
  ssize_t n = readlink("/proc/self/exe", executable, sizeof executable - 1);
  if (n < 0 || (size_t)n >= sizeof executable - 1)
    return;
  executable[n] = 0;
  if (strcmp(executable, "/opt/retroarch/bin/retroarch") || !owned_launch())
    return;
  if (!executable_matches()) {
    report_failure("frontend identity mismatch");
    return;
  }
#if defined(__aarch64__)
  static const uint64_t expected_table[] = {0x1126270, 0x1126298, 0xa744bc,
                                            0xa7458c,  0xa745d8,  0xa74694,
                                            0xa7472c,  0xa74770};
  struct backend *table = (void *)(uintptr_t)BACKEND_ADDRESS;
  if (sizeof *table != sizeof expected_table ||
      memcmp(table, expected_table, sizeof expected_table) ||
      *(const uint32_t *)(uintptr_t)0xad34dc != 0xd63f0100 ||
      *(const uint32_t *)(uintptr_t)0xad3508 != 0xd63f0100 ||
      *(const uint32_t *)(uintptr_t)0xad3534 != 0xd63f0100 ||
      *(const uint32_t *)(uintptr_t)0xad3588 != 0x52800840) {
    report_failure("frontend memory mismatch");
    return;
  }
  original = *table;
  long page_size = sysconf(_SC_PAGESIZE);
  if (page_size != 4096) {
    report_failure("page size mismatch");
    return;
  }
  void *page =
      (void *)(uintptr_t)(BACKEND_ADDRESS & ~(uint64_t)(page_size - 1));
  if (mprotect(page, (size_t)page_size, PROT_READ | PROT_WRITE)) {
    report_failure("callback table protection failed");
    return;
  }
  __atomic_store_n(&table->set_in, compress_in, __ATOMIC_SEQ_CST);
  __atomic_store_n(&table->set_out, compress_out, __ATOMIC_SEQ_CST);
  __atomic_store_n(&table->trans, compress_trans, __ATOMIC_SEQ_CST);
  /* The inspected ELF maps this rodata page in its R E PT_LOAD segment. */
  if (mprotect(page, (size_t)page_size, PROT_READ | PROT_EXEC)) {
    __atomic_store_n(&table->set_in, original.set_in, __ATOMIC_SEQ_CST);
    __atomic_store_n(&table->set_out, original.set_out, __ATOMIC_SEQ_CST);
    __atomic_store_n(&table->trans, original.trans, __ATOMIC_SEQ_CST);
    (void)mprotect(page, (size_t)page_size, PROT_READ | PROT_EXEC);
    report_failure("callback table protection restoration failed");
    return;
  }
  installed = 1;
  fprintf(stderr, "[Nearby] Netplay compression ready: level=1\n");
#else
  report_failure("frontend architecture mismatch");
#endif
}

__attribute__((destructor)) static void finalize(void) {
  if (installed)
    fprintf(stderr,
            "[Nearby] Netplay compression finished: states=%u failures=%u\n",
            __atomic_load_n(&completed, __ATOMIC_RELAXED),
            __atomic_load_n(&failures, __ATOMIC_RELAXED));
}
