package dioxus.compose.ui.platform

import dioxus.compose.foundation.ShellDestination
import dioxus.compose.foundation.platformNavigationShell
import dioxus.compose.protocol.IconRole
import platform.UIKit.UITabBarController
import platform.UIKit.UIViewController
import platform.UIKit.tabBarItem
import kotlin.test.AfterTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

private fun destinations() = listOf(
    ShellDestination(nodeId = 2, label = "Tasks", icon = IconRole.List, enabled = true),
    ShellDestination(nodeId = 3, label = "Done", icon = IconRole.Check, enabled = true),
)

/**
 * The system's tab bar, driven from Kotlin.
 *
 * Runs on a simulator, so these are assertions about what UIKit actually built and not
 * about what the code meant to ask for. What no test here can show is the glass itself:
 * whether the system painted the bar as Liquid Glass is a drawing decision made after this
 * code has finished, and it is checked by looking at a screenshot.
 */
class IosNavigationShellTest {
    private val tabs = UITabBarController()
    private val shell = IosNavigationShell(tabs)

    @AfterTest
    fun clearShell() {
        platformNavigationShell = null
    }

    @Test
    fun fr14_8_one_tab_is_built_for_each_destination_and_carries_its_label() {
        shell.present(destinations(), selected = 0) {}

        val built = tabs.viewControllers.orEmpty().filterIsInstance<UIViewController>()
        assertEquals(2, built.size)
        assertEquals(listOf("Tasks", "Done"), built.map { it.title })
        assertEquals(listOf("Tasks", "Done"), built.map { it.tabBarItem.title })
        assertFalse(tabs.tabBar.hidden, "the bar is on the screen once it has tabs")
    }

    /** A role that has a symbol gets one, and the tab is built either way. */
    @Test
    fun fr14_8_a_destination_without_an_icon_still_becomes_a_tab() {
        shell.present(
            listOf(ShellDestination(nodeId = 2, label = "Plain", icon = null, enabled = true)),
            selected = 0,
        ) {}

        val built = tabs.viewControllers.orEmpty().filterIsInstance<UIViewController>()
        assertEquals(1, built.size)
        assertNull(built[0].tabBarItem.image, "no role means no picture, not a missing one")
        assertEquals("Plain", built[0].tabBarItem.title)

        shell.present(destinations(), selected = 0) {}
        val withIcons = tabs.viewControllers.orEmpty().filterIsInstance<UIViewController>()
        assertNotNull(withIcons[0].tabBarItem.image, "a list role has a symbol on this system")
    }

    /** What the Host selected is what the bar shows. */
    @Test
    fun fr14_8_a_selection_from_the_host_moves_the_system_bar() {
        shell.present(destinations(), selected = 0) {}
        assertEquals(0, tabs.selectedIndex.toInt())

        shell.present(destinations(), selected = 1) {}
        assertEquals(1, tabs.selectedIndex.toInt())
    }

    /**
     * Moving the selection does not build the tabs again.
     *
     * Rebuilding them would restart the bar's own animation on every tap, and on iOS 26
     * that animation is most of what the bar is.
     */
    @Test
    fun fr14_8_moving_the_selection_keeps_the_same_tabs() {
        shell.present(destinations(), selected = 0) {}
        val first = tabs.viewControllers.orEmpty().filterIsInstance<UIViewController>()

        shell.present(destinations(), selected = 1) {}
        val second = tabs.viewControllers.orEmpty().filterIsInstance<UIViewController>()

        // Compared by value, not by Kotlin identity: an Objective-C object reaching Kotlin
        // twice can arrive in two wrappers, and `isEqual:` on a view controller is the
        // pointer comparison this is about.
        assertEquals(first[0], second[0])
        assertEquals(first[1], second[1])
    }

    /**
     * A choice made on the system's bar comes back as the index that was chosen.
     *
     * The delegate is what UIKit calls for a user's tap and not for a selection set in
     * code, which is what keeps a selection arriving from the Host from bouncing back out
     * as a click.
     */
    @Test
    fun fr14_8_a_choice_on_the_system_bar_reports_its_index() {
        var chosen = -1
        shell.present(destinations(), selected = 0) { chosen = it }

        val second = tabs.viewControllers.orEmpty().filterIsInstance<UIViewController>()[1]
        val delegate = assertNotNull(tabs.delegate, "the shell installs a delegate")
        delegate.tabBarController(tabs, didSelectViewController = second)

        assertEquals(1, chosen)
    }

    /** Nothing is left standing when the last navigation leaves the tree. */
    @Test
    fun fr14_8_dismissing_takes_the_bar_down() {
        shell.present(destinations(), selected = 0) {}
        shell.dismiss()

        assertEquals(0, tabs.viewControllers.orEmpty().size)
        assertTrue(tabs.tabBar.hidden)
    }

    /**
     * The Compose surface is put under the bar once and left there.
     *
     * The whole shape of this shell is in these assertions. The content is a child of the
     * plain container and not of the tab bar controller, its view is the bottom subview of
     * the tab bar controller's view, and neither changes when the tabs do. A change of tab
     * therefore never disturbs the composition.
     */
    @Test
    fun fr14_8_the_content_is_installed_under_the_bar_and_stays_there() {
        val content = UIViewController(nibName = null, bundle = null)
        val installed = installNativeNavigationShell(content)

        assertEquals(installed.root, content.parentViewController)
        assertEquals(0, installed.tabs.view.subviews.indexOf(content.view))
        assertTrue(installed.tabs.tabBar.hidden, "no navigation has arrived yet")
        assertEquals(installed.shell, platformNavigationShell)

        installed.shell.present(destinations(), selected = 1) {}

        // The tabs replaced the tab bar controller's children, and the content is still
        // parented where it was. Parenting it to the tab bar controller instead would have
        // lost it here, because that is what setViewControllers does to its children.
        assertEquals(installed.root, content.parentViewController)
        assertEquals(0, installed.tabs.view.subviews.indexOf(content.view))
        assertEquals(2, installed.tabs.viewControllers.orEmpty().size)
        assertEquals(1, installed.tabs.selectedIndex.toInt())
    }
}
