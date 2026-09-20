package org.thisisthepy.dioxus.compose.protocol

import java.nio.charset.Charset

/**
 * `String.toByteArray(Charset)` for Kotlin/Native, so the generated codec compiles unchanged.
 *
 * The codec only ever asks for UTF-8, which is what Kotlin/Native's no-argument
 * `toByteArray` produces, so the charset argument is checked rather than interpreted.
 */
internal fun String.toByteArray(charset: Charset): ByteArray {
    require(charset.name == "UTF-8") { "the protocol encodes strings as UTF-8 (SPEC PR-4)" }
    return encodeToByteArray()
}
