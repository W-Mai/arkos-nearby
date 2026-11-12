#define _GNU_SOURCE
#include <SDL.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <time.h>
#include "chord.h"

#define FILTER_PATH "/opt/howff/assets/home-input.so"
#define FRONTEND_PATH "/usr/bin/emulationstation/emulationstation"

static bool enabled;
static struct nearby_chord chord;
static SDL_JoystickID owner = -1;
static int (*poll_event)(SDL_Event *);
static int (*wait_event)(SDL_Event *, int);
static SDL_Joystick *(*joystick_for_id)(SDL_JoystickID);
static const char *(*joystick_name)(SDL_Joystick *);
static int (*joystick_buttons)(SDL_Joystick *);

/* Scope the adapter to this frontend; launched games inherit other preloads. */
__attribute__((constructor)) static void initialize(void) {
    const char *preload = getenv("LD_PRELOAD");
    if (preload) {
        char *copy = strdup(preload);
        if (copy) {
            char *save = NULL;
            size_t used = 0;
            for (char *part = strtok_r(copy, ": \t", &save); part;
                 part = strtok_r(NULL, ": \t", &save)) {
                if (!strcmp(part, FILTER_PATH)) continue;
                size_t length = strlen(part);
                if (used) copy[used++] = ':';
                memmove(copy + used, part, length);
                used += length;
            }
            copy[used] = '\0';
            if (used) setenv("LD_PRELOAD", copy, 1);
            else unsetenv("LD_PRELOAD");
            free(copy);
        }
    }
    char path[PATH_MAX];
    ssize_t length = readlink("/proc/self/exe", path, sizeof(path) - 1);
    if (length < 0) return;
    path[length] = '\0';
    enabled = !strcmp(path, FRONTEND_PATH);
    poll_event = dlsym(RTLD_NEXT, "SDL_PollEvent");
    wait_event = dlsym(RTLD_NEXT, "SDL_WaitEventTimeout");
    joystick_for_id = dlsym(RTLD_NEXT, "SDL_JoystickFromInstanceID");
    joystick_name = dlsym(RTLD_NEXT, "SDL_JoystickName");
    joystick_buttons = dlsym(RTLD_NEXT, "SDL_JoystickNumButtons");
}

static bool consume(SDL_Event *event) {
    if (!enabled || !event || !joystick_for_id || !joystick_name || !joystick_buttons)
        return false;
    if (event->type == SDL_JOYDEVICEREMOVED && event->jdevice.which == owner) {
        owner = -1;
        chord = (struct nearby_chord){0};
    }
    if (event->type != SDL_JOYBUTTONDOWN && event->type != SDL_JOYBUTTONUP)
        return false;
    SDL_Joystick *joystick = joystick_for_id(event->jbutton.which);
    const char *name = joystick ? joystick_name(joystick) : NULL;
    if (!name || strcmp(name, "GO-Super Gamepad") || joystick_buttons(joystick) != 17)
        return false;
    if (owner != event->jbutton.which) {
        owner = event->jbutton.which;
        chord = (struct nearby_chord){0};
    }
    return nearby_consume(&chord, event->jbutton.button, event->type == SDL_JOYBUTTONDOWN);
}

__attribute__((visibility("default"))) int SDL_PollEvent(SDL_Event *event) {
    if (!poll_event) return 0;
    while (poll_event(event)) {
        if (!consume(event)) return 1;
    }
    return 0;
}

static int64_t milliseconds(void) {
    struct timespec time;
    clock_gettime(CLOCK_MONOTONIC, &time);
    return (int64_t)time.tv_sec * 1000 + time.tv_nsec / 1000000;
}

__attribute__((visibility("default"))) int SDL_WaitEventTimeout(SDL_Event *event, int timeout) {
    if (!wait_event) return 0;
    int64_t deadline = timeout >= 0 ? milliseconds() + timeout : 0;
    int remaining = timeout;
    while (wait_event(event, remaining)) {
        if (!consume(event)) return 1;
        if (timeout >= 0) {
            int64_t left = deadline - milliseconds();
            if (left <= 0) return SDL_PollEvent(event);
            remaining = (int)left;
        }
    }
    return 0;
}
