package org.thisisthepy.dioxus.compose.nativeimage

import org.graalvm.nativeimage.hosted.Feature
import org.graalvm.nativeimage.hosted.RuntimeJNIAccess
import org.graalvm.nativeimage.hosted.RuntimeReflection
import java.lang.reflect.Modifier

/**
 * Registers the AWT text input path for the image (SPEC §6, INTENT D9-macOS).
 *
 * The platform calls into Java through JNI to drive an input method: it asks which character
 * a point maps to, where the caret is, what the composed text is. Those Java methods are
 * private and reached only from Objective-C, so the closed-world analysis cannot see them.
 * Missing one does not fail the build; it aborts the process as an Objective-C exception the
 * first time an IME touches a text field, which is how this was found.
 *
 * Listing methods by hand is the wrong shape of solution: the list is long, it is invisible
 * to the compiler, and it silently rots when the JDK changes. This registers whole packages
 * instead, letting the image carry a little more than it strictly needs.
 *
 * Native methods are deliberately skipped. Registering one makes the image link against its
 * symbol, and the JDK declares `CInputMethod.nativeHandleEvent` without shipping it, so the
 * library then fails to load. The JDK's own registration covers the native side.
 */
class ImeReachabilityFeature : Feature {

    override fun getDescription(): String = "Registers the AWT input method path for JNI"

    override fun beforeAnalysis(access: Feature.BeforeAnalysisAccess) {
        val module = ModuleLayer.boot().findModule("java.desktop").orElse(null) ?: return
        val classes = classesIn(module, PACKAGES) + EXTRA_CLASSES
        for (name in classes) {
            val type = runCatching { Class.forName(name, false, ClassLoader.getSystemClassLoader()) }
                .getOrNull() ?: continue
            register(type)
        }
    }

    private fun register(type: Class<*>) {
        RuntimeReflection.register(type)
        RuntimeJNIAccess.register(type)
        runCatching {
            val members = type.declaredMethods.filterNot { Modifier.isNative(it.modifiers) }
            RuntimeReflection.register(*members.toTypedArray())
            RuntimeJNIAccess.register(*members.toTypedArray())
            RuntimeReflection.register(*type.declaredConstructors)
            RuntimeJNIAccess.register(*type.declaredConstructors)
            RuntimeReflection.register(*type.declaredFields)
            RuntimeJNIAccess.register(*type.declaredFields)
        }
    }

    /** Class names in [module] whose package is one of [packages]. */
    private fun classesIn(module: Module, packages: Set<String>): List<String> =
        module.packages
            .filter { it in packages }
            .flatMap { pkg ->
                runCatching {
                    ModuleLayer.boot().configuration().findModule(module.name).get()
                        .reference().open().use { reader ->
                            reader.list().use { entries ->
                                entries.filter { it.endsWith(".class") }
                                    .map { it.removeSuffix(".class").replace('/', '.') }
                                    .filter { it.substringBeforeLast('.') == pkg }
                                    .toList()
                            }
                        }
                }.getOrDefault(emptyList())
            }

    private companion object {
        /** Everything that participates in input method handling on the AWT side. */
        val PACKAGES = setOf("sun.lwawt.macosx", "sun.awt.im", "java.awt.im", "java.awt.im.spi")

        /** Types the input method exchanges with the platform, from packages too broad to take whole. */
        val EXTRA_CLASSES = listOf(
            "java.awt.event.InputMethodEvent",
            "java.awt.font.TextAttribute",
            "java.awt.font.TextHitInfo",
            "java.text.AttributedString",
            "java.text.AttributedCharacterIterator",
            "java.text.AttributedCharacterIterator\$Attribute",
        )
    }
}
