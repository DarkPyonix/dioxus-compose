#ifndef __APPLE__
#define _GNU_SOURCE
#endif

/*
 * Public C entry points of the renderer library.
 *
 * The Host/Renderer boundary is plain C: only primitives, pointers and lengths cross it.
 *
 * GraalVM @CEntryPoint functions need an isolate thread argument. The Host must not have
 * to know about isolates, so this shim owns the single isolate per process and exposes
 * argument-free symbols that forward to the Kotlin `_impl` entry points.
 */
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <wchar.h>
#define PATH_MAX (32768 * 4)
#else
#include <dlfcn.h>
#include <libgen.h>
#include <limits.h>
#include <pthread.h>
#include <stdatomic.h>
#endif

typedef struct graal_isolate_t graal_isolate_t;
typedef struct graal_isolatethread_t graal_isolatethread_t;

int graal_create_isolate(void *params, graal_isolate_t **isolate, graal_isolatethread_t **thread);
int graal_attach_thread(graal_isolate_t *isolate, graal_isolatethread_t **thread);
graal_isolatethread_t *graal_get_current_thread(graal_isolate_t *isolate);

int32_t dioxus_compose_renderer_run_impl(graal_isolatethread_t *thread, const char *library_dir);
void dioxus_compose_renderer_request_frame_impl(graal_isolatethread_t *thread);
int32_t dioxus_compose_renderer_run(void);

#ifdef __APPLE__
void dioxus_compose_prepare_main_thread(void);
void dioxus_compose_park_main_thread(atomic_bool *finished);
void dioxus_compose_stop_main_thread(void);
#endif

enum {
    RUN_OK = 0,
    RUN_ISOLATE_FAILED = -1,
    RUN_ALREADY_RUNNING = -2,
    RUN_LIBRARY_PATH_UNKNOWN = -3,
    RUN_NOT_MAIN_THREAD = -4,
    RUN_THREAD_FAILED = -5,
};

#ifdef _WIN32
static graal_isolate_t *volatile renderer_isolate;
#else
static _Atomic(graal_isolate_t *) renderer_isolate;
#endif

#ifndef _WIN32
/* strlcpy is available on macOS but not glibc. Keep path copying local and portable. */
static void copy_path(char *destination, size_t capacity, const char *source) {
    if (capacity > 0) {
        snprintf(destination, capacity, "%s", source);
    }
}
#endif

struct renderer_run {
    char library_dir[PATH_MAX];
    int32_t status;
#ifndef _WIN32
    atomic_bool finished;
#endif
};

#ifdef _WIN32
/*
 * PE/COFF cannot leave the Host symbols unresolved and bind them from the executable at
 * DLL load time, as the macOS linker does with dynamic_lookup. These definitions satisfy
 * the renderer link and forward the same C ABI, unchanged, to exports on the host
 * executable.
 * The host must export the five dioxus_compose_host_* functions.
 *
 * UNTESTED: this forwarding path has not been compiled or run in this repository. Verify it
 * on Windows x64 with desktop/scripts/smoke-test-windows.ps1 -RequireClick.
 */
typedef struct {
    const uint8_t *ptr;
    uint32_t len;
    int64_t result;
} MutationBatch;

typedef int32_t (__cdecl *host_init_fn)(const uint8_t *, uint32_t, MutationBatch *);
typedef int32_t (__cdecl *host_dispatch_event_fn)(const uint8_t *, uint32_t, MutationBatch *);
typedef int32_t (__cdecl *host_render_frame_fn)(uint64_t, MutationBatch *);
typedef void (__cdecl *host_release_batch_fn)(MutationBatch *);
typedef void (__cdecl *host_shutdown_fn)(void);

struct host_exports {
    host_init_fn init;
    host_dispatch_event_fn dispatch_event;
    host_render_frame_fn render_frame;
    host_release_batch_fn release_batch;
    host_shutdown_fn shutdown;
};

static INIT_ONCE host_exports_once = INIT_ONCE_STATIC_INIT;
static struct host_exports host_exports;

static BOOL CALLBACK load_host_exports(PINIT_ONCE once, PVOID parameter, PVOID *context) {
    HMODULE host = GetModuleHandleW(NULL);
    (void)once;
    (void)parameter;
    (void)context;
    if (host == NULL) {
        fprintf(stderr, "dioxus-compose: GetModuleHandleW(NULL) failed (%lu)\n", GetLastError());
        return FALSE;
    }

#define LOAD_HOST_EXPORT(field, name) \
    host_exports.field = (name##_fn)GetProcAddress(host, "dioxus_compose_host_" #field)
    LOAD_HOST_EXPORT(init, host_init);
    LOAD_HOST_EXPORT(dispatch_event, host_dispatch_event);
    LOAD_HOST_EXPORT(render_frame, host_render_frame);
    LOAD_HOST_EXPORT(release_batch, host_release_batch);
    LOAD_HOST_EXPORT(shutdown, host_shutdown);
#undef LOAD_HOST_EXPORT

    if (host_exports.init == NULL || host_exports.dispatch_event == NULL ||
        host_exports.render_frame == NULL || host_exports.release_batch == NULL ||
        host_exports.shutdown == NULL) {
        fprintf(stderr,
                "dioxus-compose: host executable must export all dioxus_compose_host_* functions\n");
        return FALSE;
    }
    return TRUE;
}

static int host_exports_available(void) {
    return InitOnceExecuteOnce(&host_exports_once, load_host_exports, NULL, NULL) != 0;
}

int32_t dioxus_compose_host_init(const uint8_t *handshake, uint32_t length, MutationBatch *out) {
    return host_exports_available() ? host_exports.init(handshake, length, out) : -1;
}

int32_t dioxus_compose_host_dispatch_event(
    const uint8_t *event, uint32_t length, MutationBatch *out
) {
    return host_exports_available() ? host_exports.dispatch_event(event, length, out) : -1;
}

int32_t dioxus_compose_host_render_frame(uint64_t frame_time_nanos, MutationBatch *out) {
    return host_exports_available() ? host_exports.render_frame(frame_time_nanos, out) : -1;
}

void dioxus_compose_host_release_batch(MutationBatch *batch) {
    if (host_exports_available()) {
        host_exports.release_batch(batch);
    }
}

void dioxus_compose_host_shutdown(void) {
    if (host_exports_available()) {
        host_exports.shutdown();
    }
}

static int renderer_library_dir(char *out, size_t out_size) {
    HMODULE module;
    wchar_t path[32768];
    DWORD length;
    wchar_t *separator;
    int bytes;

    if (!GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            (LPCWSTR)(uintptr_t)&dioxus_compose_renderer_run,
            &module)) {
        return 0;
    }
    length = GetModuleFileNameW(module, path, (DWORD)(sizeof path / sizeof path[0]));
    if (length == 0 || length == sizeof path / sizeof path[0]) {
        return 0;
    }
    separator = wcsrchr(path, L'\\');
    if (separator == NULL) {
        return 0;
    }
    *separator = L'\0';
    bytes = WideCharToMultiByte(CP_UTF8, WC_ERR_INVALID_CHARS, path, -1,
                                out, (int)out_size, NULL, NULL);
    return bytes > 0;
}
#endif

/* Creates the isolate on the calling thread and runs the renderer until its window closes. */
static void *renderer_thread(void *arg) {
    struct renderer_run *run = arg;
    graal_isolate_t *isolate;
    graal_isolatethread_t *thread;
    if (graal_create_isolate(NULL, &isolate, &thread) != 0) {
        run->status = RUN_ISOLATE_FAILED;
    } else {
#ifdef _WIN32
        InterlockedExchangePointer((PVOID volatile *)&renderer_isolate, isolate);
#else
        atomic_store(&renderer_isolate, isolate);
#endif
        run->status = dioxus_compose_renderer_run_impl(thread, run->library_dir);
    }
#ifndef _WIN32
    atomic_store(&run->finished, true);
#endif
#ifdef __APPLE__
    dioxus_compose_stop_main_thread();
#endif
    return NULL;
}

#ifdef _WIN32
/**
 * Writes to the terminal this was started from, where there is one.
 *
 * The executable is linked for the window subsystem, so double clicking it does not open a
 * console beside the window. That is what anyone expects of an application and it costs
 * something: a process with no console has no standard error, and the messages that
 * explain an empty window, a renderer built from another schema among them, would go
 * nowhere.
 *
 * Attaching to the parent's console gives both. Started from a terminal, it writes there
 * as it always did. Started from Explorer there is no parent console, AttachConsole fails,
 * and nothing is opened.
 */
static void dioxus_compose_attach_parent_console(void) {
    if (!AttachConsole(ATTACH_PARENT_PROCESS)) {
        return;
    }
    // The streams were opened against a console that did not exist, so they are reopened
    // against the one just attached. Failures are ignored: the alternative to writing a
    // diagnostic is not writing one, and refusing to draw over it would be worse.
    FILE *stream = NULL;
    (void)freopen_s(&stream, "CONOUT$", "w", stderr);
    (void)freopen_s(&stream, "CONOUT$", "w", stdout);
}
#endif

int32_t dioxus_compose_renderer_run(void) {
#ifdef _WIN32
    dioxus_compose_attach_parent_console();
    static LONG started;
    if (InterlockedExchange(&started, 1)) {
        return RUN_ALREADY_RUNNING;
    }
#else
    static atomic_bool started;
    if (atomic_exchange(&started, true)) {
        return RUN_ALREADY_RUNNING;
    }
#endif
#ifdef __APPLE__
    if (!pthread_main_np()) {
        return RUN_NOT_MAIN_THREAD;
    }
#endif

    static struct renderer_run run;

    /* AWT and Skiko resolve their files relative to this library, not the executable. */
#ifdef _WIN32
    if (!renderer_library_dir(run.library_dir, sizeof run.library_dir)) {
        return RUN_LIBRARY_PATH_UNKNOWN;
    }
#else
    Dl_info info;
    char library_path[PATH_MAX];
    if (!dladdr((const void *)&dioxus_compose_renderer_run, &info) || info.dli_fname == NULL) {
        return RUN_LIBRARY_PATH_UNKNOWN;
    }
    copy_path(library_path, sizeof library_path, info.dli_fname);
    copy_path(run.library_dir, sizeof run.library_dir, dirname(library_path));
#endif

#ifdef __APPLE__
    dioxus_compose_prepare_main_thread();
    pthread_t thread;
    if (pthread_create(&thread, NULL, renderer_thread, &run) != 0) {
        return RUN_THREAD_FAILED;
    }
    dioxus_compose_park_main_thread(&run.finished);
    pthread_join(thread, NULL);
#else
    renderer_thread(&run);
#endif
    return run.status;
}

/*
 * Thread-safe. A Host worker thread is attached on its first call and stays attached,
 * so repeated requests do not pay the attach cost. Attaching per call would put that cost
 * on every frame request, which the frame budget does not have room for.
 */
void dioxus_compose_renderer_request_frame(void) {
#ifdef _WIN32
    graal_isolate_t *isolate = InterlockedCompareExchangePointer(
        (PVOID volatile *)&renderer_isolate, NULL, NULL
    );
#else
    graal_isolate_t *isolate = atomic_load(&renderer_isolate);
#endif
    if (isolate == NULL) {
        return;
    }
    graal_isolatethread_t *thread = graal_get_current_thread(isolate);
    if (thread == NULL && graal_attach_thread(isolate, &thread) != 0) {
        return;
    }
    dioxus_compose_renderer_request_frame_impl(thread);
}
