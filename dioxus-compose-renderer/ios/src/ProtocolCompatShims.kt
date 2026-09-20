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

/**
 * `String(bytes, offset, length, charset)` for Kotlin/Native.
 *
 * The generated codec reads a string straight out of a slice of the arena, which on the JVM
 * is a constructor of `java.lang.String`. Kotlin/Native has no such constructor, so the same
 * name is declared here, in the codec's own package, where it is found ahead of the
 * `kotlin.String` factories.
 *
 * Decoding is strict: malformed UTF-8 throws rather than turning into replacement
 * characters, because a bad string means the two sides disagree about the arena and the
 * Renderer has to say so rather than draw nonsense.
 */
@Suppress("FunctionName")
internal fun String(bytes: ByteArray, offset: Int, length: Int, charset: Charset): kotlin.String {
    require(charset.name == "UTF-8") {
        "this shim only decodes UTF-8, and ${charset.name} was requested. " +
            "Every string on the wire is UTF-8, so a different charset here means the " +
            "caller and the wire format disagree about what the bytes mean."
    }
    return bytes.decodeToString(offset, offset + length, throwOnInvalidSequence = true)
}
