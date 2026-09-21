package dioxus.compose.ui.platform

import dioxus.compose.protocol.HostEvent
import dioxus.compose.runtime.DioxusHost

/**
 * The process-wide Host.
 *
 * Android recreates an Activity for a configuration change but not the process, and the
 * VirtualDom is kept for the life of the process. Holding the interpreted tree here, and
 * not in the composition, is what makes a rotation cost nothing: the new Activity composes
 * the same node table again, with no boundary call and no state lost.
 *
 * Everything here runs on the UI thread, which is where the VirtualDom lives.
 */
object DioxusRuntime {
    /**
     * The application's cdylib. It is the Rust side of the app: it links this crate,
     * declares its root component through `dioxus_compose::android_main!`, and carries the
     * generated JNI shims and `JNI_OnLoad` with it.
     */
    private const val LIBRARY = "android_demo"

    private val connection = AndroidHostConnection()
    private var host: DioxusHost? = null

    init {
        try {
            System.loadLibrary(LIBRARY)
        } catch (missing: UnsatisfiedLinkError) {
            // The default message names the library and the directories it looked in,
            // which leaves the reader with a file name and no way to get the file. What
            // produces it is one script, so the message says so.
            throw UnsatisfiedLinkError(
                "lib$LIBRARY.so is not in this APK, so there is no Host to draw with. " +
                    "Build it with android/scripts/build-host.sh, which writes it into " +
                    "android/jniLibs/<abi>/ where the APK picks it up. " +
                    "(${missing.message})",
            )
        }
    }

    /** The Host, started on first use and kept until the process ends. */
    fun host(): DioxusHost {
        host?.let { return it }
        val created = DioxusHost(connection)
        created.start()
        host = created
        return created
    }

    /** The UI came back on screen. Timers and animations resume. */
    fun start() {
        host?.dispatch(HostEvent.LifecycleStart(NO_NODE, NO_HANDLER))
    }

    /** The UI left the screen. The Host suppresses timers and animations. */
    fun stop() {
        host?.dispatch(HostEvent.LifecycleStop(NO_NODE, NO_HANDLER))
    }

    /**
     * Asks for the whole tree again, for a Renderer that lost its node table.
     *
     * Nothing calls it on the ordinary path, and that is the point: the two sides share a
     * process, so the table can only be lost if the Renderer throws it away, and this
     * object does not. It is here for a Renderer that chooses to, and it costs component
     * state, because the Host keeps no shadow of the tree it already sent.
     */
    fun resync() {
        host?.resync()
    }

    /** True once the Host exists, so a recreated Activity can tell the two cases apart. */
    val isStarted: Boolean get() = host != null

    /** A lifecycle event addresses the Host itself, so it carries no node and no handler. */
    private const val NO_NODE = 0
    private const val NO_HANDLER = 0L
}
