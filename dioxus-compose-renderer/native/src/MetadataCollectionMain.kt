package org.thisisthepy.dioxus.compose.nativeimage

import org.thisisthepy.dioxus.compose.renderer.m0DemoHost

/**
 * JVM entry point for collecting reachability metadata with the GraalVM tracing agent.
 * See `scripts/collect-metadata.sh`.
 *
 * A scripted Host drives it: a JVM run has no Rust executable to resolve the
 * `dioxus_compose_host_*` symbols against, and `NativeHostConnection` needs GraalVM word
 * types that exist only inside a native image (SPEC NFR-5).
 */
fun main() {
    runRenderer(System.getenv("DIOXUS_COMPOSE_AUTOEXIT_MS")?.toLongOrNull()) { m0DemoHost() }
}
