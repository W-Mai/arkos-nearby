/* Load an arcade archive and check rollback state support without advancing it. */
#include "abi.h"

#define OPTION_LIMIT 4096
#define TEXT_LIMIT 8192
#define STATE_LIMIT (64u * 1024u * 1024u)
struct option { char *key, *value; };
static struct option options[OPTION_LIMIT];
static size_t option_count;
/* Some installed cores copy MAX_PATH bytes from directory callback values. */
static char system_directory[32768], save_directory[32768];
static bool stopped;
static size_t logged_bytes;

static void message(const char *text) {
    size_t length = strlen(text);
    if (length > 4096) length = 4096;
    if (logged_bytes + length > 16384) return;
    (void)write(2, text, length);
    logged_bytes += length;
}

static void fail(const char *text) {
    message("[CoreProbe] ");
    message(text);
    message("\n");
}

static double milliseconds(void) {
    struct timespec now;
    if (clock_gettime(CLOCK_MONOTONIC, &now)) exit(20);
    return now.tv_sec * 1000.0 + now.tv_nsec / 1000000.0;
}

static bool blank(char value) {
    return value == ' ' || value == '\t' || value == '\r' || value == '\n';
}

static char *trim(char *text) {
    while (blank(*text)) text++;
    size_t size = strlen(text);
    while (size && blank(text[size - 1])) text[--size] = 0;
    return text;
}

static bool put_option(const char *key, const char *value, bool replace) {
    size_t index;
    for (index = 0; index < option_count; index++) {
        if (!strcmp(options[index].key, key)) break;
    }
    if (index < option_count && !replace) return true;
    if (index == OPTION_LIMIT || strlen(key) > 1024 || strlen(value) > 4096) return false;
    char *copy = strdup(value);
    if (!copy) return false;
    if (index == option_count) {
        options[index].key = strdup(key);
        if (!options[index].key) { free(copy); return false; }
        option_count++;
    }
    free(options[index].value);
    options[index].value = copy;
    return true;
}

static bool read_options(const char *path) {
    FILE *file = fopen(path, "r");
    if (!file) return false;
    char line[TEXT_LIMIT];
    size_t total = 0;
    bool valid = true;
    while (fgets(line, sizeof(line), file)) {
        size_t size = strlen(line);
        total += size;
        if (size == sizeof(line) - 1 || total > 8u * 1024u * 1024u) { valid = false; break; }
        char *key = trim(line);
        if (!*key || *key == '#') continue;
        char *equals = strchr(key, '=');
        if (!equals) { valid = false; break; }
        *equals = 0;
        key = trim(key);
        char *value = trim(equals + 1);
        if (*value == '"') {
            value++;
            char *end = strchr(value, '"');
            if (!end) { valid = false; break; }
            *end++ = 0;
            end = trim(end);
            if (*end && *end != '#') { valid = false; break; }
        } else {
            char *comment = strchr(value, '#');
            if (comment) *comment = 0;
            value = trim(value);
        }
        if (!*key || !put_option(key, value, true)) { valid = false; break; }
    }
    if (ferror(file)) valid = false;
    fclose(file);
    return valid;
}

static void logger(int level, const char *format, ...) {
    if (level < 2) return;
    char text[4096];
    va_list arguments;
    __builtin_va_start(arguments, format);
    (void)vsnprintf(text, sizeof(text), format, arguments);
    __builtin_va_end(arguments);
    message(text);
}

static bool environment(unsigned command, void *data) {
    switch (command) {
    case RETRO_ENVIRONMENT_GET_SYSTEM_DIRECTORY:
    case RETRO_ENVIRONMENT_GET_CORE_ASSETS_DIRECTORY:
        if (data) *(const char **)data = system_directory;
        return true;
    case RETRO_ENVIRONMENT_GET_SAVE_DIRECTORY:
        if (data) *(const char **)data = save_directory;
        return true;
    case RETRO_ENVIRONMENT_GET_SAVESTATE_CONTEXT:
        if (data) *(unsigned *)data = RETRO_SAVESTATE_CONTEXT_ROLLBACK_NETPLAY;
        return true;
    case RETRO_ENVIRONMENT_GET_AUDIO_VIDEO_ENABLE:
        if (data) *(int *)data = 7;
        return true;
    case RETRO_ENVIRONMENT_GET_CAN_DUPE:
        if (data) *(bool *)data = true;
        return true;
    case RETRO_ENVIRONMENT_SET_PIXEL_FORMAT:
        return data && *(unsigned *)data <= 2;
    case RETRO_ENVIRONMENT_GET_LANGUAGE:
    case RETRO_ENVIRONMENT_GET_CORE_OPTIONS_VERSION:
        if (data) *(unsigned *)data = 0;
        return true;
    case RETRO_ENVIRONMENT_GET_LOG_INTERFACE:
        if (data) ((struct retro_log_callback *)data)->log = logger;
        return true;
    case RETRO_ENVIRONMENT_GET_VARIABLE_UPDATE:
        if (data) *(bool *)data = false;
        return true;
    case RETRO_ENVIRONMENT_GET_VARIABLE: {
        struct retro_variable *variable = data;
        if (!variable || !variable->key) return false;
        variable->value = NULL;
        for (size_t i = 0; i < option_count; i++) {
            if (!strcmp(variable->key, options[i].key)) {
                variable->value = options[i].value;
                return true;
            }
        }
        return false;
    }
    case RETRO_ENVIRONMENT_SET_VARIABLES: {
        const struct retro_variable *variable = data;
        size_t count = 0;
        for (; variable && variable->key; variable++) {
            if (++count > OPTION_LIMIT) { stopped = true; return false; }
            const char *start = strchr(variable->value ? variable->value : "", ';');
            if (!start) continue;
            while (blank(*++start)) {}
            char value[4097];
            size_t size = 0;
            while (start[size] && start[size] != '|' && size < sizeof(value) - 1) size++;
            if (start[size] && start[size] != '|') { stopped = true; return false; }
            memcpy(value, start, size);
            value[size] = 0;
            if (!put_option(variable->key, value, false)) { stopped = true; return false; }
        }
        return true;
    }
    case RETRO_ENVIRONMENT_SHUTDOWN:
        stopped = true;
        return true;
    case RETRO_ENVIRONMENT_SET_MESSAGE:
        if (data && ((struct retro_message *)data)->msg) message(((struct retro_message *)data)->msg);
        return true;
    case RETRO_ENVIRONMENT_SET_INPUT_DESCRIPTORS:
    case RETRO_ENVIRONMENT_SET_CONTROLLER_INFO:
    case RETRO_ENVIRONMENT_SET_SUBSYSTEM_INFO:
    case RETRO_ENVIRONMENT_SET_GEOMETRY:
    case RETRO_ENVIRONMENT_SET_SYSTEM_AV_INFO:
    case RETRO_ENVIRONMENT_SET_PERFORMANCE_LEVEL:
    case RETRO_ENVIRONMENT_SET_SUPPORT_NO_GAME:
    case RETRO_ENVIRONMENT_SET_SERIALIZATION_QUIRKS:
    case RETRO_ENVIRONMENT_SET_CORE_OPTIONS_DISPLAY:
        return true;
    default:
        return false;
    }
}

static void video(const void *data, unsigned width, unsigned height, size_t pitch) {
    (void)data; (void)width; (void)height; (void)pitch;
}
static void audio(int16_t left, int16_t right) { (void)left; (void)right; }
static size_t audio_batch(const int16_t *data, size_t frames) { (void)data; return frames; }
static void poll(void) {}
static int16_t input(unsigned port, unsigned device, unsigned index, unsigned id) {
    (void)port; (void)device; (void)index; (void)id;
    return 0;
}

int main(int argc, char **argv) {
    if (argc != 6) { fail("Expected CORE ROM SYSTEM_DIR PRIVATE_SAVE_DIR OPTIONS_FILE"); return 2; }
    if (strlen(argv[3]) >= sizeof(system_directory) || strlen(argv[4]) >= sizeof(save_directory)) {
        fail("Directory path is too long"); return 2;
    }
    memcpy(system_directory, argv[3], strlen(argv[3]) + 1);
    memcpy(save_directory, argv[4], strlen(argv[4]) + 1);
    if (!read_options(argv[5])) { fail("Invalid core options file"); return 3; }
    double begin = milliseconds();
    void *library = dlopen(argv[1], RTLD_NOW | RTLD_LOCAL);
    if (!library) { fail(dlerror()); return 4; }
#define FN(result, name, arguments) result (*name) arguments = (result (*) arguments)dlsym(library, #name); if (!name) { fail("Missing " #name); dlclose(library); return 5; }
    FN(void, retro_get_system_info, (struct retro_system_info *));
    FN(void, retro_set_environment, (retro_environment_t));
    FN(void, retro_set_video_refresh, (void (*)(const void *, unsigned, unsigned, size_t)));
    FN(void, retro_set_audio_sample, (void (*)(int16_t, int16_t)));
    FN(void, retro_set_audio_sample_batch, (size_t (*)(const int16_t *, size_t)));
    FN(void, retro_set_input_poll, (void (*)(void)));
    FN(void, retro_set_input_state, (int16_t (*)(unsigned, unsigned, unsigned, unsigned)));
    FN(void, retro_init, (void));
    FN(void, retro_deinit, (void));
    FN(bool, retro_load_game, (const struct retro_game_info *));
    FN(void, retro_unload_game, (void));
    FN(size_t, retro_serialize_size, (void));
    FN(bool, retro_serialize, (void *, size_t));
    FN(bool, retro_unserialize, (const void *, size_t));
    struct retro_system_info info = {0};
    retro_get_system_info(&info);
    if (!info.library_name || strcmp(info.library_name, "FinalBurn Neo") || !info.need_fullpath || !info.block_extract) {
        fail("Expected a full-path FinalBurn Neo core"); dlclose(library); return 6;
    }
    retro_set_environment(environment);
    retro_set_video_refresh(video);
    retro_set_audio_sample(audio);
    retro_set_audio_sample_batch(audio_batch);
    retro_set_input_poll(poll);
    retro_set_input_state(input);
    double opened = milliseconds();
    retro_init();
    double initialized = milliseconds();
    struct retro_game_info game = {argv[2], NULL, 0, NULL};
    bool loaded = retro_load_game(&game);
    double loaded_at = milliseconds();
    int result = 0;
    size_t size = loaded && !stopped ? retro_serialize_size() : 0;
    void *state = NULL;
    if (!loaded || stopped) { fail("Core rejected the archive"); result = 7; }
    else if (!size || size > STATE_LIMIT) { fail("Archive has no usable rollback state (missing ROM, BIOS or driver)"); result = 8; }
    else if (!(state = calloc(1, size))) { fail("State allocation failed"); result = 9; }
    else if (!retro_serialize(state, size) || stopped) { fail("Rollback state serialization failed"); result = 10; }
    else if (!retro_unserialize(state, size) || stopped) { fail("Rollback state restoration failed"); result = 11; }
    double checked = milliseconds();
    free(state);
    if (loaded) retro_unload_game();
    retro_deinit();
    dlclose(library);
    double cleaned = milliseconds();
    for (size_t i = 0; i < option_count; i++) { free(options[i].key); free(options[i].value); }
    if (!result) {
        printf("{\"schema\":1,\"loaded\":true,\"state_bytes\":%zu,\"timings_ms\":{\"open\":%.3f,\"init\":%.3f,\"load\":%.3f,\"state\":%.3f,\"cleanup\":%.3f,\"total\":%.3f}}\n",
            size, opened - begin, initialized - opened, loaded_at - initialized, checked - loaded_at, cleaned - checked, cleaned - begin);
    }
    return result;
}
