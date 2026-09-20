package dioxus.compose.ui.platform

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.window.ComposeUIViewController
import kotlinx.cinterop.ObjCObjectBase.OverrideInit
import dioxus.compose.runtime.DioxusContent
import dioxus.compose.runtime.HostConnection
import dioxus.compose.ui.platform.IosHostConnection
import dioxus.compose.runtime.rememberDioxusHost
import platform.Foundation.NSStringFromClass
import platform.UIKit.UIApplication
import platform.UIKit.UIApplicationDelegateProtocol
import platform.UIKit.UIApplicationDelegateProtocolMeta
import platform.UIKit.UIApplicationMain
import platform.UIKit.UIResponder
import platform.UIKit.UIResponderMeta
import platform.UIKit.UIScreen
import platform.UIKit.UIWindow

/**
 * Runs the renderer's Compose application (SPEC PR-2 `dioxus_compose_renderer_run`).
 *
 * The desktop renderer opens a window and returns when the window closes. iOS has no such
 * moment: `UIApplicationMain` installs the run loop and never returns, and the system, not
 * the application, decides when the process ends. So on iOS `run` blocks for the lifetime of
 * the process. A Rust `main` that calls `dioxus_compose::launch(app)` gets the behaviour it
 * already expects on desktop, which is that nothing after the call runs while the UI is up.
 *
 * The screen is drawn entirely from the mutation batches the Host streams (FR-1, FR-2).
 */
internal fun runRenderer(connection: () -> HostConnection): Int {
    hostConnectionFactory = connection
    UIApplicationMain(0, null, null, NSStringFromClass(RendererAppDelegate))
    return 0
}

private var hostConnectionFactory: () -> HostConnection = { IosHostConnection() }

/**
 * The minimum UIKit application: one window whose root view controller is Compose.
 *
 * There is no storyboard and no Swift, because on iOS the Host owns the process the way it
 * does on desktop. A Host that wants to embed the renderer in an existing app instead can
 * call `DioxusContent` from its own `ComposeUIViewController`, but that is not the M5 path.
 */
private class RendererAppDelegate : UIResponder, UIApplicationDelegateProtocol {
    @OverrideInit
    constructor() : super()

    companion object : UIResponderMeta(), UIApplicationDelegateProtocolMeta

    private var mainWindow: UIWindow? = null

    override fun window(): UIWindow? = mainWindow

    override fun setWindow(window: UIWindow?) {
        mainWindow = window
    }

    override fun application(
        application: UIApplication,
        didFinishLaunchingWithOptions: Map<Any?, *>?,
    ): Boolean {
        val window = UIWindow(frame = UIScreen.mainScreen.bounds)
        window.rootViewController = ComposeUIViewController {
            DioxusContent(
                rememberDioxusHost(remember { hostConnectionFactory() }),
                Modifier.fillMaxSize(),
            )
        }
        window.makeKeyAndVisible()
        mainWindow = window
        return true
    }
}
