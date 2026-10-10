/* Minimal libretro v1 and ARM64 glibc declarations; see ABI-LICENSE.txt. */
#ifndef ARKOS_CORE_PROBE_ABI_H
#define ARKOS_CORE_PROBE_ABI_H

typedef __SIZE_TYPE__ size_t;
typedef __INT16_TYPE__ int16_t;
typedef __builtin_va_list va_list;
typedef struct _IO_FILE FILE;
typedef _Bool bool;
#define true 1
#define false 0
#define NULL ((void *)0)

extern void *dlopen(const char *, int);
extern void *dlsym(void *, const char *);
extern char *dlerror(void);
extern int dlclose(void *);
extern int printf(const char *, ...);
extern int snprintf(char *, size_t, const char *, ...);
extern int vsnprintf(char *, size_t, const char *, va_list);
extern __PTRDIFF_TYPE__ write(int, const void *, size_t);
extern FILE *fopen(const char *, const char *);
extern char *fgets(char *, int, FILE *);
extern int ferror(FILE *);
extern int fclose(FILE *);
extern void *calloc(size_t, size_t);
extern void free(void *);
extern char *strdup(const char *);
extern size_t strlen(const char *);
extern int strcmp(const char *, const char *);
extern char *strchr(const char *, int);
extern void *memcpy(void *, const void *, size_t);
extern void exit(int);
struct timespec { long tv_sec; long tv_nsec; };
extern int clock_gettime(int, struct timespec *);
#if defined(__APPLE__)
#define CLOCK_MONOTONIC 6
#else
#define CLOCK_MONOTONIC 1
#endif
#define RTLD_NOW 2
#define RTLD_LOCAL 0

typedef bool (*retro_environment_t)(unsigned, void *);
struct retro_system_info {
    const char *library_name, *library_version, *valid_extensions;
    bool need_fullpath, block_extract;
};
struct retro_game_info { const char *path; const void *data; size_t size; const char *meta; };
struct retro_variable { const char *key, *value; };
struct retro_log_callback { void (*log)(int, const char *, ...); };
struct retro_message { const char *msg; unsigned frames; };

#define RETRO_ENVIRONMENT_GET_CAN_DUPE 3
#define RETRO_ENVIRONMENT_SET_MESSAGE 6
#define RETRO_ENVIRONMENT_SHUTDOWN 7
#define RETRO_ENVIRONMENT_SET_PERFORMANCE_LEVEL 8
#define RETRO_ENVIRONMENT_GET_SYSTEM_DIRECTORY 9
#define RETRO_ENVIRONMENT_SET_PIXEL_FORMAT 10
#define RETRO_ENVIRONMENT_SET_INPUT_DESCRIPTORS 11
#define RETRO_ENVIRONMENT_GET_VARIABLE 15
#define RETRO_ENVIRONMENT_SET_VARIABLES 16
#define RETRO_ENVIRONMENT_GET_VARIABLE_UPDATE 17
#define RETRO_ENVIRONMENT_SET_SUPPORT_NO_GAME 18
#define RETRO_ENVIRONMENT_GET_LOG_INTERFACE 27
#define RETRO_ENVIRONMENT_GET_CORE_ASSETS_DIRECTORY 30
#define RETRO_ENVIRONMENT_GET_SAVE_DIRECTORY 31
#define RETRO_ENVIRONMENT_SET_SYSTEM_AV_INFO 32
#define RETRO_ENVIRONMENT_SET_SUBSYSTEM_INFO 34
#define RETRO_ENVIRONMENT_SET_CONTROLLER_INFO 35
#define RETRO_ENVIRONMENT_SET_GEOMETRY 37
#define RETRO_ENVIRONMENT_GET_LANGUAGE 39
#define RETRO_ENVIRONMENT_SET_SERIALIZATION_QUIRKS 44
#define RETRO_ENVIRONMENT_GET_AUDIO_VIDEO_ENABLE (47 | 0x10000)
#define RETRO_ENVIRONMENT_GET_CORE_OPTIONS_VERSION 52
#define RETRO_ENVIRONMENT_SET_CORE_OPTIONS_DISPLAY 55
#define RETRO_ENVIRONMENT_GET_SAVESTATE_CONTEXT (72 | 0x10000)
#define RETRO_SAVESTATE_CONTEXT_ROLLBACK_NETPLAY 3

#endif
