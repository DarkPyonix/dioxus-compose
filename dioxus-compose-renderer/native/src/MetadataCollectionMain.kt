package org.thisisthepy.dioxus.compose.nativeimage

/**
 * JVM entry point for collecting reachability metadata with the GraalVM tracing agent.
 * See `scripts/collect-metadata.sh`.
 */
fun main() {
    runRenderer(System.getenv("DIOXUS_COMPOSE_AUTOEXIT_MS")?.toLongOrNull())
}
