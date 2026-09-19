/*
 * Public C entry points of the renderer library (SPEC PR-2).
 *
 * GraalVM @CEntryPoint functions need an isolate thread argument. The Host must not have
 * to know about isolates, so this shim owns the single isolate per process and exposes
 * argument-free symbols that forward to the Kotlin `_impl` entry points.
 */
#include <dlfcn.h>
#include <libgen.h>
#include <limits.h>
#include <pthread.h>
#include <stdatomic.h>
#include <stdint.h>
#include <string.h>

typedef struct graal_isolate_t graal_isolate_t;
typedef struct graal_isolatethread_t graal_isolatethread_t;

int graal_create_isolate(void *params, graal_isolate_t **isolate, graal_isolatethread_t **thread);
int graal_attach_thread(graal_isolate_t *isolate, graal_isolatethread_t **thread);
graal_isolatethread_t *graal_get_current_thread(graal_isolate_t *isolate);

int32_t dioxus_compose_renderer_run_impl(graal_isolatethread_t *thread, const char *library_dir);
void dioxus_compose_renderer_request_frame_impl(graal_isolatethread_t *thread);

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

static _Atomic(graal_isolate_t *) renderer_isolate;

struct renderer_run {
    char library_dir[PATH_MAX];
    int32_t status;
    atomic_bool finished;
};

/* Creates the isolate on the calling thread and runs the renderer until its window closes. */
static void *renderer_thread(void *arg) {
    struct renderer_run *run = arg;
    graal_isolate_t *isolate;
    graal_isolatethread_t *thread;
    if (graal_create_isolate(NULL, &isolate, &thread) != 0) {
        run->status = RUN_ISOLATE_FAILED;
    } else {
        atomic_store(&renderer_isolate, isolate);
        run->status = dioxus_compose_renderer_run_impl(thread, run->library_dir);
    }
    atomic_store(&run->finished, true);
#ifdef __APPLE__
    dioxus_compose_stop_main_thread();
#endif
    return NULL;
}

int32_t dioxus_compose_renderer_run(void) {
    static atomic_bool started;
    if (atomic_exchange(&started, true)) {
        return RUN_ALREADY_RUNNING;
    }
#ifdef __APPLE__
    if (!pthread_main_np()) {
        return RUN_NOT_MAIN_THREAD;
    }
#endif

    static struct renderer_run run;

    /* AWT and Skiko resolve their files relative to this library, not the executable. */
    Dl_info info;
    char library_path[PATH_MAX];
    if (!dladdr((const void *)&dioxus_compose_renderer_run, &info) || info.dli_fname == NULL) {
        return RUN_LIBRARY_PATH_UNKNOWN;
    }
    strlcpy(library_path, info.dli_fname, sizeof library_path);
    strlcpy(run.library_dir, dirname(library_path), sizeof run.library_dir);

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
 * so repeated requests do not pay the attach cost (SPEC PR-3).
 */
void dioxus_compose_renderer_request_frame(void) {
    graal_isolate_t *isolate = atomic_load(&renderer_isolate);
    if (isolate == NULL) {
        return;
    }
    graal_isolatethread_t *thread = graal_get_current_thread(isolate);
    if (thread == NULL && graal_attach_thread(isolate, &thread) != 0) {
        return;
    }
    dioxus_compose_renderer_request_frame_impl(thread);
}
