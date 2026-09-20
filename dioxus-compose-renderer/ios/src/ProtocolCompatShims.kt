package dioxus.compose.protocol

import java.nio.charset.Charset

/**
 * `String.toByteArray(Charset)` for Kotlin/Native, so the generated codec compiles unchanged.
 *
 * The codec only ever asks for UTF-8, which is what Kotlin/Native's no-argument
 * `toByteArray` produces, so the charset argument is checked rather than interpreted.
 */
internal fun String.toByteArray(charset: Charset): ByteArray {
    require(charset.name == "UTF-8") {
        "this shim only encodes UTF-8, and ${charset.name} was requested. " +
            "Every string on the wire is UTF-8, so a different charset here means the " +
            "caller and the wire format disagree about what the bytes mean."
    }
    return encodeToByteArray()
}
