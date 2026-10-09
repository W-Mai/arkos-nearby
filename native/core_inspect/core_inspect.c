/* Inspect the installed libretro ABI without initializing a game frontend. */

extern void *dlopen(const char *, int);
extern void *dlsym(void *, const char *);
extern char *dlerror(void);
extern int dlclose(void *);
extern int printf(const char *, ...);
extern int putchar(int);

struct retro_system_info {
    const char *library_name;
    const char *library_version;
    const char *valid_extensions;
    _Bool need_fullpath;
    _Bool block_extract;
};

static void json_string(const char *text) {
    if (!text) {
        printf("null");
        return;
    }
    putchar('"');
    for (const unsigned char *p = (const unsigned char *)text; *p; ++p) {
        if (*p == '"' || *p == '\\') {
            putchar('\\');
            putchar(*p);
        } else if (*p < 32) {
            printf("\\u%04x", (unsigned int)*p);
        } else {
            putchar(*p);
        }
    }
    putchar('"');
}

int main(int argc, char **argv) {
    if (argc != 2) return 2;
    void *library = dlopen(argv[1], 1); /* RTLD_LAZY | RTLD_LOCAL on Linux. */
    if (!library) {
        printf("{\"error\":");
        json_string(dlerror());
        printf("}\n");
        return 3;
    }
    void (*get_info)(struct retro_system_info *) =
        (void (*)(struct retro_system_info *))dlsym(library, "retro_get_system_info");
    if (!get_info) {
        printf("{\"error\":\"Missing retro_get_system_info\"}\n");
        dlclose(library);
        return 4;
    }
    struct retro_system_info info = {0};
    get_info(&info);
    printf("{\"pointer_bits\":%u,\"library_name\":", (unsigned int)sizeof(void *) * 8);
    json_string(info.library_name);
    printf(",\"library_version\":");
    json_string(info.library_version);
    printf(",\"valid_extensions\":");
    json_string(info.valid_extensions);
    printf(",\"need_fullpath\":%s,\"block_extract\":%s}\n",
           info.need_fullpath ? "true" : "false",
           info.block_extract ? "true" : "false");
    dlclose(library);
    return 0;
}
