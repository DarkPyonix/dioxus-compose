package org.thisisthepy.dioxus.compose.nativeimage

import org.graalvm.nativeimage.hosted.Feature
import org.graalvm.nativeimage.hosted.RuntimeResourceAccess

/**
 * Registers the AWT accessibility bridge for the image (SPEC NFR-8, §7, INTENT D9-macOS).
 *
 * VoiceOver reaches a Compose window through AWT. Compose asks Skiko to attach the window's
 * `Accessible` to the platform, Skiko's native `initializeCAccessible` looks up
 * `sun.lwawt.macosx.CAccessible` through JNI, and from then on AppKit drives the whole tree
 * from Objective-C: for each element it calls back into `CAccessibility` to ask for the
 * children, the role, the name and the value.
 *
 * This Feature removes one proven gap on that path and does not yet make it work.
 *
 * The proven part is the resource bundle. Asked for a child's role,
 * `CAccessibility.getAccessibleRole` calls `AccessibleRole.toString`, which resolves the
 * role's display name through the bundle below. A resource bundle is not reachable unless
 * it is registered, so on an image without it the lookup raised
 * `MissingResourceRegistrationError` from inside `getAccessibleRole`. Registering the
 * bundle makes that error go away, which was confirmed by building with
 * `--exact-reachability-metadata -R:MissingRegistrationReportingMode=Warn` and watching it
 * disappear from the run.
 *
 * The classes are registered on the same grounds as the input method path: the platform
 * enters them from Objective-C rather than from Java, so the closed-world analysis has no
 * reason to keep their members. That registration is not known to be either necessary or
 * sufficient here, because no tool reported it missing. The `sun.lwawt.macosx` half is
 * already covered by [ImeReachabilityFeature], which takes that package whole.
 *
 * What remains: with the bundle registered and no missing registration reported anywhere,
 * an accessibility query still aborts the process. AppKit raises
 * `NSInvalidArgumentException: object cannot be nil` while `childrenOfParent` builds the
 * array of children, so something on that path still hands Objective-C a null and the
 * closed-world analysis is no longer the one complaining. See
 * `experiments/accessibility/README.md` for the evidence and the next step.
 */
class AccessibilityReachabilityFeature : Feature {

    override fun getDescription(): String = "Registers the AWT accessibility bridge"

    override fun beforeAnalysis(access: Feature.BeforeAnalysisAccess) {
        val desktop = ModuleLayer.boot().findModule("java.desktop").orElse(null)
        if (desktop != null) {
            for (bundle in BUNDLES) {
                runCatching { RuntimeResourceAccess.addResourceBundle(desktop, bundle) }
            }
        }
        val classes = ReachabilityRegistration.classesIn("java.desktop", PACKAGES) + EXTRA_CLASSES
        ReachabilityRegistration.registerAll(classes)
    }

    private companion object {
        /**
         * Display names for the accessibility vocabulary. `AccessibleRole.toString` and
         * `AccessibleState.toString` go through these, and the platform asks for a role on
         * every element it walks, so a missing bundle takes out the whole tree rather than
         * one label.
         */
        val BUNDLES = listOf("com.sun.accessibility.internal.resources.accessibility")

        /** The accessibility vocabulary the platform exchanges with `CAccessibility`. */
        val PACKAGES = setOf("javax.accessibility")

        /** AWT types in the signatures Objective-C resolves, from packages too broad to take whole. */
        val EXTRA_CLASSES = listOf(
            "java.awt.Component",
            "java.awt.Container",
            "java.awt.Dimension",
            "java.awt.Point",
            "java.awt.Rectangle",
            "java.lang.Number",
        )
    }
}
