/*
 * Stand-in for the Rust Host: links the renderer library and runs it, the way
 * `dioxus_compose::launch` will with LoopMode::Renderer (SPEC PR-2).
 */
#include <stdint.h>
#include <stdio.h>

int32_t dioxus_compose_renderer_run(void);

int main(void) {
    int32_t status = dioxus_compose_renderer_run();
    printf("dioxus_compose_renderer_run returned %d\n", status);
    return status == 0 ? 0 : 1;
}
