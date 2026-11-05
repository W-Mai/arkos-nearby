/* Inspect the installed libretro ABI without initializing a game frontend. */

extern void *dlopen(const char *, int);
extern void *dlsym(void *, const char *);
extern char *dlerror(void);
extern int dlclose(void *);
extern int printf(const char *, ...);
extern int putchar(int);
extern int strcmp(const char *, const char *);

struct retro_system_info {
    const char *library_name;
    const char *library_version;
    const char *valid_extensions;
    _Bool need_fullpath;
    _Bool block_extract;
};

struct retro_variable { const char *key; const char *value; };
struct option_value { const char *value; const char *label; };
struct option_v1 { const char *key; const char *desc; const char *info; struct option_value values[128]; const char *default_value; };
struct option_v2 { const char *key; const char *desc; const char *desc_categorized; const char *info; const char *info_categorized; const char *category_key; struct option_value values[128]; const char *default_value; };
struct options_v2 { const void *categories; const struct option_v2 *definitions; };
struct options_intl { const void *us; const void *local; };
static struct retro_variable variables[1024];
static unsigned variable_count;
static char defaults[1024][256];
extern char *strdup(const char *);
extern void free(void *);

static void clear_options(void) {
    for (unsigned i = 0; i < variable_count; ++i) {
        free((void *)variables[i].key); free((void *)variables[i].value);
    }
    variable_count = 0;
}
static _Bool append_text(char *text, unsigned *length, const char *value) {
    while (value && *value) { if (*length >= 65534) return 0; text[(*length)++] = *value++; }
    text[*length] = 0; return 1;
}
static _Bool store_option(const char *key, const char *definition) {
    if (variable_count >= 1024) return 0;
    char *name = strdup(key), *text = strdup(definition ? definition : "");
    if (!name || !text) { free(name); free(text); return 0; }
    unsigned i = variable_count++;
    variables[i].key = name; variables[i].value = text;
    const char *value = text;
    while (*value && *value != ';') ++value;
    if (*value) ++value;
    while (*value == ' ') ++value;
    unsigned j = 0;
    while (*value && *value != '|' && j < 255) defaults[i][j++] = *value++;
    defaults[i][j] = 0; return 1;
}
static _Bool modern_option(const char *key, const char *desc, const struct option_value *values, const char *initial) {
    char text[65536]; unsigned length = 0;
    const char *first = initial ? initial : values[0].value;
    if (!first) return 1;
    if (!append_text(text,&length,desc ? desc : key) || !append_text(text,&length,"; ") || !append_text(text,&length,first)) return 0;
    for (unsigned i = 0; i < 128 && values[i].value; ++i) {
        if (!strcmp(first, values[i].value)) continue;
        if (!append_text(text,&length,"|") || !append_text(text,&length,values[i].value)) return 0;
    }
    return store_option(key,text);
}

static _Bool options_environment(unsigned command, void *data) {
    if ((command == 52 || command == 39) && data) { *(unsigned *)data = command == 52 ? 2 : 0; return 1; }
    if ((command == 53 || command == 54) && data) {
        const struct option_v1 *defs = command == 54 ? ((const struct options_intl *)data)->us : data;
        clear_options();
        for (unsigned i=0; defs && i<1024 && defs[i].key; ++i)
            if (!modern_option(defs[i].key,defs[i].desc,defs[i].values,defs[i].default_value)) return 0;
        return 1;
    }
    if ((command == 67 || command == 68) && data) {
        const struct options_v2 *options = command == 68 ? ((const struct options_intl *)data)->us : data;
        const struct option_v2 *defs = options ? options->definitions : 0;
        clear_options();
        for (unsigned i=0; defs && i<1024 && defs[i].key; ++i)
            if (!modern_option(defs[i].key,defs[i].desc,defs[i].values,defs[i].default_value)) return 0;
        return 1;
    }
    if (command == 15 && data) {
        struct retro_variable *query = data;
        query->value = 0;
        for (unsigned i = 0; i < variable_count; ++i)
            if (!strcmp(query->key, variables[i].key)) { query->value = defaults[i]; return 1; }
        return 0;
    }
    if (command == 16 && data) {
        const struct retro_variable *incoming = data;
        clear_options();
        for (unsigned i = 0; i < 1024 && incoming[i].key; ++i) {
            if (!store_option(incoming[i].key,incoming[i].value)) return 0;
        }
        return 1;
    }
    if (command == 17 && data) { *(unsigned char *)data = 0; return 1; }
    return 0;
}

static void begin_options(void *library) {
    void (*set_environment)(_Bool (*)(unsigned, void *)) =
        (void (*)(_Bool (*)(unsigned, void *)))dlsym(library, "retro_set_environment");
    if (set_environment) set_environment(options_environment);
}

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
    if (argc != 2 && (argc != 3 || strcmp(argv[2], "--options"))) return 2;
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
    if (argc == 3) begin_options(library);
    printf("{\"pointer_bits\":%u,\"library_name\":", (unsigned int)sizeof(void *) * 8);
    json_string(info.library_name);
    printf(",\"library_version\":");
    json_string(info.library_version);
    printf(",\"valid_extensions\":");
    json_string(info.valid_extensions);
    printf(",\"need_fullpath\":%s,\"block_extract\":%s",
           info.need_fullpath ? "true" : "false",
           info.block_extract ? "true" : "false");
    if (argc == 3) {
        printf(",\"options\":[");
        for (unsigned i = 0; i < variable_count; ++i) {
            if (i) putchar(','); printf("{\"key\":"); json_string(variables[i].key);
            printf(",\"definition\":"); json_string(variables[i].value); putchar('}');
        }
        putchar(']');
    }
    printf("}\n");
    clear_options();
    dlclose(library);
    return 0;
}
